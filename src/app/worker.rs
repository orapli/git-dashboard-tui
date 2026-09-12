use super::types::*;
use crate::config::{self, Member};
use crate::git::{
    self, BranchInfo, CommitSummary, Contributor, StashEntry, Summary, TagInfo, TimeSpan,
    WorktreeInfo,
};
use std::collections::HashMap;
use std::ops::ControlFlow;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;

/// Spawn a single-threaded worker: jobs sent to `job_rx` run one at a time,
/// in the order they were queued. The app runs several of these on separate
/// queues (interactive, bulk, finder) plus the Home pool below; the per-job
/// work itself lives in [`run_job`] so every lane handles a job kind exactly
/// the same way.
pub fn spawn_worker(job_rx: Receiver<Job>, msg_tx: Sender<Msg>, home_gen: Arc<AtomicU64>) {
    thread::spawn(move || {
        while let Ok(job) = job_rx.recv() {
            if run_one(job, &msg_tx, &home_gen).is_break() {
                break;
            }
        }
    });
}

/// Lower bound on the Home pool, and what we fall back to when
/// `available_parallelism` cannot report (a restricted container, say).
const HOME_POOL_MIN: usize = 2;
/// Upper bound on the Home pool. These jobs are process-spawn and I/O bound
/// rather than CPU bound, so more threads than cores would still help in
/// principle — but each one spawns several `git` processes and, for a GitHub
/// remote, two `gh` processes. An unbounded pool would thrash a laptop's
/// process table and disk and hammer the GitHub API rate limit, so the win is
/// capped here well before that.
const HOME_POOL_MAX: usize = 8;

/// Size of the Home pool for a machine reporting `parallelism` usable
/// threads (`None` when `available_parallelism` failed).
pub fn home_pool_size(parallelism: Option<usize>) -> usize {
    parallelism
        .unwrap_or(HOME_POOL_MIN)
        .clamp(HOME_POOL_MIN, HOME_POOL_MAX)
}

/// Spawn the Home pool: `threads` threads sharing one queue, so that many
/// `LoadHome` jobs are analysed at once instead of the whole dashboard
/// refresh serialising behind a single worker.
///
/// The receiver is shared under a mutex that is held *only* across `recv()`
/// and released before the job runs — holding it over the work would quietly
/// re-serialise the pool into one thread with extra steps.
pub fn spawn_home_pool(
    job_rx: Receiver<Job>,
    msg_tx: Sender<Msg>,
    home_gen: Arc<AtomicU64>,
    threads: usize,
) {
    spawn_pool(job_rx, threads, move |job| run_one(job, &msg_tx, &home_gen));
}

/// Generic shared-queue pool used by [`spawn_home_pool`]. Each thread loops:
/// lock, `recv()`, unlock, then run `run` outside the lock. A thread stops on
/// its own when `run` reports `Break` (its message receiver is gone) or the
/// queue closes; the other threads keep going, matching the single worker's
/// "break out of my own loop" semantics.
fn spawn_pool<F>(job_rx: Receiver<Job>, threads: usize, run: F)
where
    F: Fn(Job) -> ControlFlow<()> + Clone + Send + 'static,
{
    let shared = Arc::new(Mutex::new(job_rx));
    for _ in 0..threads {
        let shared = Arc::clone(&shared);
        let run = run.clone();
        thread::spawn(move || {
            loop {
                let job = {
                    // Recovering from poisoning is safe here: the lock is only
                    // ever held across `recv()`, which leaves no half-updated
                    // state behind, and one panicking thread must not take the
                    // rest of the pool down with it.
                    let guard = shared.lock().unwrap_or_else(|e| e.into_inner());
                    match guard.recv() {
                        Ok(job) => job,
                        Err(_) => break,
                    }
                };
                if run(job).is_break() {
                    break;
                }
            }
        });
    }
}

/// Run one job and post its result, reporting whether this worker should keep
/// going (`Continue`) or stop because the UI's receiver is gone (`Break`).
fn run_one(job: Job, msg_tx: &Sender<Msg>, home_gen: &AtomicU64) -> ControlFlow<()> {
    let Some(msg) = run_job(job, msg_tx, home_gen) else {
        return ControlFlow::Continue(());
    };
    if msg_tx.send(msg).is_err() {
        ControlFlow::Break(())
    } else {
        ControlFlow::Continue(())
    }
}

/// Execute one job, returning the message to hand back to the UI thread, or
/// `None` when the job produced nothing to report.
///
/// Every lane runs jobs through here, so a job kind is handled in exactly one
/// place no matter which worker picked it up.
///
/// `home_gen` is the newest home-refresh generation the app has issued. The
/// queue is unbounded and holding `r` (or a bulk pull) enqueues one `LoadHome`
/// per repository per refresh, so superseded ones are dropped here rather than
/// each running a full repository analysis whose result is then discarded.
/// This check has to keep working from every pool thread, which it does: the
/// generation is a shared atomic, read at the moment the job is picked up.
fn run_job(job: Job, msg_tx: &Sender<Msg>, home_gen: &AtomicU64) -> Option<Msg> {
    let msg = match job {
        Job::ScanRepos {
            seq,
            generation,
            root,
        } => {
            let errors = super::finder::scan_repositories(
                &root,
                || generation.load(Ordering::Relaxed) != seq,
                |path| {
                    let _ = msg_tx.send(Msg::FinderRepo {
                        seq,
                        repo: super::finder::found_repository(path),
                    });
                },
            );
            Msg::FinderDone { seq, errors }
        }

        Job::LoadWorkspace {
            seq,
            generation,
            repos,
        } => {
            let errors = super::workspace::collect_workspace(
                &repos,
                || generation.load(Ordering::Relaxed) != seq,
                |row| {
                    let _ = msg_tx.send(Msg::WorkspaceRow { seq, row });
                },
            );
            Msg::WorkspaceDone { seq, errors }
        }
        Job::LoadHome {
            generation,
            index,
            path,
        } => {
            if generation < home_gen.load(Ordering::Relaxed) {
                return None;
            }
            let row = load_home_row(&path);
            Msg::HomeLoaded {
                generation,
                index,
                row,
            }
        }
        Job::LoadRepo {
            index,
            path,
            members,
            time_span,
        } => {
            let data = Box::new(load_repo(&path, &members, time_span));
            Msg::RepoLoaded { index, data }
        }
        Job::LoadDiff {
            seq,
            path,
            base,
            target,
            file,
            three_dot,
            ignore_whitespace,
            full,
        } => {
            let result = git::get_file_diff(
                &path,
                base.as_deref(),
                &target,
                &file,
                ignore_whitespace,
                full,
                three_dot,
            );
            Msg::DiffLoaded { seq, result }
        }
        Job::LoadBlame {
            seq,
            path,
            blame_ref,
            file,
        } => {
            let result = git::get_file_blame(&path, &blame_ref, &file);
            Msg::BlameLoaded { seq, result }
        }
        Job::LoadCommitMeta { seq, path, hash } => {
            let header = git::get_commit_show(&path, &hash);
            let files = git::get_changed_files(&path, None, &hash, false);
            Msg::CommitMeta { seq, header, files }
        }
        Job::LoadFiles {
            seq,
            path,
            base,
            target,
            three_dot,
            preselect,
        } => {
            let files = git::get_changed_files(&path, base.as_deref(), &target, three_dot);
            Msg::FilesLoaded {
                seq,
                files,
                preselect,
            }
        }
        Job::Pull { index, path, lang } => op_done(git::pull_repository(&path, lang), Some(index)),
        Job::Fetch { index, path, lang } => {
            op_done(git::fetch_repository(&path, lang), Some(index))
        }
        Job::StashApply { path, stash_ref } => op_done(git::apply_stash(&path, &stash_ref), None),
        Job::StashDrop { path, stash_ref } => op_done(git::drop_stash(&path, &stash_ref), None),
        Job::LoadBranchLog { path, branch } => {
            let body = git::get_branch_oneline_log(&path, &branch, 200);
            Msg::LogLoaded {
                title: branch,
                body,
            }
        }
        Job::LoadGlobalMembers {
            generation,
            repos,
            members,
        } => Msg::GlobalMembersLoaded {
            generation,
            list: compute_global_members(&repos, &members),
        },
        Job::SearchCommits { seq, query, repos } => {
            let result = search_commits_across(&query, &repos);
            Msg::CommitSearchLoaded {
                seq,
                hits: result.hits,
                failed_repos: result.failed_repos,
            }
        }
        Job::LoadCommitPreview { seq, path, hash } => {
            let header = git::get_commit_show(&path, &hash);
            let files = git::get_changed_files(&path, None, &hash, false);
            Msg::CommitPreviewLoaded {
                seq,
                hash,
                header,
                files,
            }
        }
    };
    Some(msg)
}

/// Per-repository cap on search hits, and overall cap after merging — a
/// query that matches broadly across many repositories should not flood the
/// results list past what's actually useful to scroll through.
const SEARCH_HITS_PER_REPO: usize = 50;
const SEARCH_HITS_TOTAL: usize = 300;

/// Cross-repo search result. A repo that errored (unreachable SSH host, `git`
/// missing, timeout, ...) is tracked separately from one that simply had no
/// matches — collapsing both into "no hits" (as a bare `unwrap_or_default()`
/// would) makes a failed search indistinguishable from a real empty result.
pub struct CommitSearchResult {
    pub hits: Vec<CommitSearchHit>,
    pub failed_repos: Vec<String>,
}

/// Search commit messages across every repository. Runs on a worker thread
/// for the same reason [`compute_global_members`] does: one `git log` per
/// repository, which would freeze the UI thread if run inline.
pub fn search_commits_across(
    query: &str,
    repos: &[(usize, String, PathBuf)],
) -> CommitSearchResult {
    let mut hits = Vec::new();
    let mut failed_repos = Vec::new();
    for (index, name, path) in repos {
        match git::search_commits(path, query, SEARCH_HITS_PER_REPO) {
            Ok(found) => hits.extend(found.into_iter().map(|h| CommitSearchHit {
                repo_index: *index,
                repo_name: name.clone(),
                hash: h.hash,
                author: h.author,
                date: h.date,
                message: h.message,
            })),
            Err(_) => failed_repos.push(name.clone()),
        }
    }
    hits.sort_by(|a, b| b.date.cmp(&a.date));
    hits.truncate(SEARCH_HITS_TOTAL);
    CommitSearchResult { hits, failed_repos }
}

/// Aggregate contributor statistics across every repository.
///
/// This walks `git log` once per repository, so it runs on a worker thread —
/// on the UI thread a single unreachable SSH host froze the whole event loop
/// for `repos.len() * GIT_TIMEOUT` with no redraw and no way to quit.
pub fn compute_global_members(
    repos: &[(usize, String, PathBuf)],
    members: &[Member],
) -> Vec<GlobalMember> {
    let mut member_map: HashMap<String, GlobalMember> = HashMap::new();

    // 1. Initialize registered members
    for m in members {
        member_map.insert(
            m.canonical_name.clone(),
            GlobalMember {
                canonical_name: m.canonical_name.clone(),
                aliases: m.aliases.clone(),
                is_active: m.is_active,
                total_commits: 0,
                repo_count: 0,
                latest_commit_date: String::new(),
                contributions: Vec::new(),
            },
        );
    }

    // 2. Scan all repositories for contributor statistics
    for (repo_idx, repo_name, repo_path) in repos {
        if let Ok(contributors) = git::get_contributors(repo_path, members, TimeSpan::All) {
            for c in contributors {
                let entry = member_map
                    .entry(c.name.clone())
                    .or_insert_with(|| GlobalMember {
                        canonical_name: c.name.clone(),
                        aliases: vec![c.email.clone()],
                        is_active: c.is_active,
                        total_commits: 0,
                        repo_count: 0,
                        latest_commit_date: String::new(),
                        contributions: Vec::new(),
                    });

                entry.total_commits += c.commit_count;
                if entry.latest_commit_date.is_empty() || c.last_commit > entry.latest_commit_date {
                    entry.latest_commit_date = c.last_commit.clone();
                }

                entry.contributions.push(MemberRepoContribution {
                    repo_name: repo_name.clone(),
                    repo_path: repo_path.clone(),
                    repo_index: *repo_idx,
                    commit_count: c.commit_count,
                    first_commit: c.first_commit,
                    last_commit: c.last_commit,
                });
            }
        }
    }

    let mut list: Vec<GlobalMember> = member_map.into_values().collect();
    for m in &mut list {
        m.repo_count = m.contributions.len();
        m.contributions
            .sort_by_key(|c| std::cmp::Reverse(c.commit_count));
    }

    list.sort_by(|a, b| {
        b.is_active
            .cmp(&a.is_active)
            .then_with(|| b.total_commits.cmp(&a.total_commits))
            .then_with(|| b.latest_commit_date.cmp(&a.latest_commit_date))
    });

    list
}

fn op_done(result: Result<String, String>, repo_index: Option<usize>) -> Msg {
    match result {
        Ok(text) => Msg::OpDone {
            ok: true,
            text,
            repo_index,
        },
        Err(text) => Msg::OpDone {
            ok: false,
            text,
            repo_index,
        },
    }
}

/// Build one Home-list row.
///
/// Uses [`git::get_home_summary`], not `git::get_summary`: a Home row shows
/// none of the whole-history/whole-tree statistics the full summary collects
/// (commit count, contributors, branch count, tree size), and Home recomputes
/// every repository on every refresh. The member list is therefore not needed
/// here — it only ever fed contributor counting.
pub fn load_home_row(path: &std::path::Path) -> Result<HomeRow, String> {
    let summary = git::get_home_summary(path)?;
    let last_commit = git::get_recent_commits(path)
        .ok()
        .and_then(|c| c.into_iter().next())
        .map(|c| c.date)
        .unwrap_or_default();
    let (open_prs, ci_status) = if let Some(ref ci_pr) = summary.remote_ci_pr {
        (ci_pr.open_prs, ci_pr.ci_status.clone())
    } else {
        (None, None)
    };
    Ok(HomeRow {
        fetched_at: chrono::Utc::now().timestamp(),
        github: summary.remote_ci_pr.clone(),
        branch: summary.current_branch,
        upstream: summary.upstream,
        ahead: summary.ahead,
        behind: summary.behind,
        dirty: summary.uncommitted_changes,
        tracked_changes: summary.tracked_changes,
        untracked: summary.untracked,
        last_fetch: summary.last_fetch,
        last_commit,
        open_prs,
        ci_status,
        op_state: summary.op_state,
        conflicts: summary.conflicts,
        error: None,
    })
}

pub fn load_repo(
    path: &std::path::Path,
    members: &[Member],
    time_span: TimeSpan,
) -> Result<RepoSnapshot, String> {
    let summary = git::get_summary(path, members)?;
    let (commits, commits_err) = split_list(git::get_commits_for_diff(path));
    let (branches, branches_err) = split_list(git::get_branches(path));
    let (tags, tags_err) = split_list(git::get_tags(path));
    let (stashes, stashes_err) = split_list(git::get_stash_list(path));
    let (working_files, working_err) =
        split_list(git::get_changed_files(path, None, "WORKING_TREE", false));
    let (contributors, contributors_err) =
        split_list(git::get_contributors(path, members, time_span));
    let (worktrees, worktrees_err) = split_list(git::get_worktrees(path));
    Ok(RepoSnapshot {
        summary,
        commits,
        commits_err,
        branches,
        branches_err,
        tags,
        tags_err,
        stashes,
        stashes_err,
        working_files,
        working_err,
        contributors,
        contributors_err,
        worktrees,
        worktrees_err,
    })
}

#[derive(serde::Serialize, serde::Deserialize)]
struct TuiCache {
    version: u32,
    summary: Summary,
    commits: Vec<CommitSummary>,
    branches: Vec<BranchInfo>,
    tags: Vec<TagInfo>,
    stashes: Vec<StashEntry>,
    contributors: Vec<Contributor>,
    #[serde(default)]
    worktrees: Vec<WorktreeInfo>,
}

pub fn load_tui_cache(path: &std::path::Path) -> Option<RepoSnapshot> {
    let json = std::fs::read_to_string(config::tui_cache_path(path)).ok()?;
    let cache: TuiCache = serde_json::from_str(&json).ok()?;
    if cache.version != 1 {
        return None;
    }
    Some(RepoSnapshot {
        summary: cache.summary,
        commits: cache.commits,
        commits_err: None,
        branches: cache.branches,
        branches_err: None,
        tags: cache.tags,
        tags_err: None,
        stashes: cache.stashes,
        stashes_err: None,
        working_files: Vec::new(),
        working_err: None,
        contributors: cache.contributors,
        contributors_err: None,
        worktrees: cache.worktrees,
        worktrees_err: None,
    })
}

pub fn save_tui_cache(path: &std::path::Path, snap: &RepoSnapshot) {
    let _ = std::fs::create_dir_all(config::cache_dir());
    let cache = TuiCache {
        version: 1,
        summary: snap.summary.clone(),
        commits: snap.commits.clone(),
        branches: snap.branches.clone(),
        tags: snap.tags.clone(),
        stashes: snap.stashes.clone(),
        contributors: snap.contributors.clone(),
        worktrees: snap.worktrees.clone(),
    };
    if let Ok(json) = serde_json::to_string(&cache) {
        let _ = config::write_atomic(&config::tui_cache_path(path), &json);
    }
}

/// Home rows are cached per repository so the dashboard renders populated on
/// the next launch instead of a screen of `…` placeholders.
///
/// This matters because building one row is not cheap: a batch of git commands
/// plus, for GitHub repositories, two `gh` calls. Measured on a real
/// repository, that is ~1.7 s with GitHub integration and ~0.45 s without —
/// and the rows are built one at a time on a worker, so the wait scales with
/// the number of repositories. The cached values are replaced as each refresh
/// lands, so the cost is a few seconds of possibly-stale numbers rather than a
/// few seconds of nothing.
pub fn load_home_cache(path: &std::path::Path) -> Option<HomeRow> {
    let json = std::fs::read_to_string(config::home_cache_path(path)).ok()?;
    serde_json::from_str(&json).ok()
}

pub fn save_home_cache(path: &std::path::Path, row: &HomeRow) {
    let _ = std::fs::create_dir_all(config::cache_dir());
    if let Ok(json) = serde_json::to_string(row) {
        let _ = config::write_atomic(&config::home_cache_path(path), &json);
    }
}

pub fn resolve_diff_command(
    template: &str,
    repo: &std::path::Path,
    base: Option<&str>,
    target: &str,
) -> Result<ExternalDiff, String> {
    let t = template.trim();
    if t.is_empty() {
        return Err("diff command is empty".into());
    }
    let first = t.split_whitespace().next().unwrap_or(t);
    let is_hunk = first == "hunk" || first == "hunkdiff";
    if is_hunk && !t.contains('{') {
        let wt = target == "WORKING_TREE";
        if wt {
            return Ok(ExternalDiff {
                program: first.to_string(),
                args: vec!["diff".into()],
                cwd: repo.to_path_buf(),
            });
        }
        if let Some(b) = base {
            return Ok(ExternalDiff {
                program: first.to_string(),
                args: vec!["diff".into(), format!("{b}..{target}")],
                cwd: repo.to_path_buf(),
            });
        }
        return Ok(ExternalDiff {
            program: first.to_string(),
            args: vec!["show".into(), target.to_string()],
            cwd: repo.to_path_buf(),
        });
    }
    let spec = if target == "WORKING_TREE" {
        String::new()
    } else {
        target.to_string()
    };
    let range = match base {
        Some(b) if !spec.is_empty() => format!("{b}..{spec}"),
        _ => spec.clone(),
    };
    let expanded = t
        .replace("{path}", &repo.display().to_string())
        .replace("{repo}", &repo.display().to_string())
        .replace("{range}", &range)
        .replace("{base} {target}", &range)
        .replace("{base}", base.unwrap_or(""))
        .replace("{target}", &spec)
        .replace("{file}", "");
    let mut parts = expanded.split_whitespace();
    let program = parts
        .next()
        .ok_or_else(|| "diff command has no program".to_string())?
        .to_string();
    let mut args: Vec<String> = parts.map(str::to_string).collect();
    let is_hunk_bin = program == "hunk" || program == "hunkdiff" || program.ends_with("/hunk");
    if is_hunk_bin
        && args.first().is_some_and(|a| a == "show")
        && args.get(1).is_some_and(|a| a.contains(".."))
    {
        args[0] = "diff".into();
    }
    Ok(ExternalDiff {
        program,
        args,
        cwd: repo.to_path_buf(),
    })
}

pub fn split_list<T>(result: Result<Vec<T>, String>) -> (Vec<T>, Option<String>) {
    match result {
        Ok(v) => (v, None),
        Err(e) => (Vec::new(), Some(e)),
    }
}

#[cfg(test)]
mod pool_tests {
    use super::*;
    use std::sync::Barrier;
    use std::sync::mpsc;
    use std::time::Duration;

    /// The clamp is the whole point of [`home_pool_size`]: a 1-core box still
    /// gets two threads (these jobs wait on `git`/`gh`, not on the CPU), a
    /// 64-core box does not get 64 concurrent `gh` calls, and a machine that
    /// cannot report its parallelism falls back to the low end.
    #[test]
    fn the_home_pool_size_is_clamped_to_a_sane_range() {
        assert_eq!(home_pool_size(None), HOME_POOL_MIN);
        assert_eq!(home_pool_size(Some(0)), HOME_POOL_MIN);
        assert_eq!(home_pool_size(Some(1)), HOME_POOL_MIN);
        assert_eq!(home_pool_size(Some(4)), 4);
        assert_eq!(home_pool_size(Some(HOME_POOL_MAX)), HOME_POOL_MAX);
        assert_eq!(home_pool_size(Some(64)), HOME_POOL_MAX);
    }

    fn dummy_job(index: usize) -> Job {
        Job::LoadHome {
            generation: 0,
            index,
            path: PathBuf::from("/nonexistent"),
        }
    }

    /// Two jobs must actually be in flight at once. The handler waits on a
    /// two-party barrier, which can only be cleared if a second pool thread
    /// picked up the second job while the first was still running — the exact
    /// thing that breaks if the queue mutex is held across the work.
    #[test]
    fn two_pool_threads_run_two_jobs_at_the_same_time() {
        let (job_tx, job_rx) = mpsc::channel::<Job>();
        let (done_tx, done_rx) = mpsc::channel::<usize>();
        let barrier = Arc::new(Barrier::new(2));
        spawn_pool(job_rx, 2, move |job| {
            let Job::LoadHome { index, .. } = job else {
                unreachable!("test only queues LoadHome");
            };
            barrier.wait();
            let _ = done_tx.send(index);
            ControlFlow::Continue(())
        });
        job_tx.send(dummy_job(1)).unwrap();
        job_tx.send(dummy_job(2)).unwrap();

        let mut seen = vec![
            done_rx
                .recv_timeout(Duration::from_secs(10))
                .expect("both jobs must run concurrently, not one after the other"),
            done_rx
                .recv_timeout(Duration::from_secs(10))
                .expect("both jobs must run concurrently, not one after the other"),
        ];
        seen.sort_unstable();
        assert_eq!(seen, vec![1, 2]);
    }

    /// One thread giving up (its `msg_tx` receiver is gone) must not take the
    /// pool down with it — the single worker breaks out of its own loop only,
    /// and the pool keeps that semantic.
    #[test]
    fn a_thread_that_breaks_does_not_stop_the_rest_of_the_pool() {
        let (job_tx, job_rx) = mpsc::channel::<Job>();
        let (done_tx, done_rx) = mpsc::channel::<usize>();
        spawn_pool(job_rx, 2, move |job| {
            let Job::LoadHome { index, .. } = job else {
                unreachable!("test only queues LoadHome");
            };
            let _ = done_tx.send(index);
            // Index 0 stands in for "the UI receiver went away".
            if index == 0 {
                ControlFlow::Break(())
            } else {
                ControlFlow::Continue(())
            }
        });
        for index in 0..6 {
            job_tx.send(dummy_job(index)).unwrap();
        }
        let mut seen = Vec::new();
        while seen.len() < 6 {
            match done_rx.recv_timeout(Duration::from_secs(10)) {
                Ok(index) => seen.push(index),
                Err(_) => break,
            }
        }
        seen.sort_unstable();
        assert_eq!(seen, vec![0, 1, 2, 3, 4, 5]);
    }
}

#[cfg(test)]
mod home_cache_tests {
    use super::*;
    use crate::git::GitOpState;

    fn row() -> HomeRow {
        HomeRow {
            branch: "main".into(),
            ahead: 2,
            behind: 1,
            dirty: 3,
            last_commit: "2026-08-25".into(),
            open_prs: Some(4),
            ci_status: Some("success".into()),
            op_state: GitOpState::None,
            conflicts: 0,
            ..Default::default()
        }
    }

    #[test]
    fn a_saved_row_round_trips() {
        let path = std::path::Path::new("/tmp/round-trips-repo");
        save_home_cache(path, &row());
        let got = load_home_cache(path).expect("cache should load back");
        assert_eq!(got.branch, "main");
        assert_eq!((got.ahead, got.behind, got.dirty), (2, 1, 3));
        assert_eq!(got.open_prs, Some(4));
        assert_eq!(got.ci_status.as_deref(), Some("success"));
    }

    #[test]
    fn each_repository_gets_its_own_cache_file() {
        let a = std::path::Path::new("/tmp/distinct-a");
        let b = std::path::Path::new("/tmp/distinct-b");
        assert_ne!(
            crate::config::home_cache_path(a),
            crate::config::home_cache_path(b)
        );
        let mut other = row();
        other.branch = "develop".into();
        save_home_cache(a, &row());
        save_home_cache(b, &other);
        assert_eq!(load_home_cache(a).unwrap().branch, "main");
        assert_eq!(load_home_cache(b).unwrap().branch, "develop");
    }

    #[test]
    fn a_never_cached_repository_simply_has_no_row() {
        assert!(load_home_cache(std::path::Path::new("/tmp/never-cached-repo")).is_none());
    }

    /// A truncated or hand-corrupted cache file must not take the app down —
    /// a stale dashboard row is a cache, not a source of truth.
    #[test]
    fn a_corrupt_cache_file_is_ignored_rather_than_fatal() {
        let path = std::path::Path::new("/tmp/corrupt-cache-repo");
        let _ = std::fs::create_dir_all(crate::config::cache_dir());
        std::fs::write(crate::config::home_cache_path(path), "{not json").unwrap();
        assert!(load_home_cache(path).is_none());
    }

    /// The cache is written by whatever build the user last ran. A file from
    /// an older build lacks fields added since; `#[serde(default)]` has to
    /// fill them in rather than the whole row being thrown away.
    #[test]
    fn a_row_from_an_older_build_still_loads() {
        let path = std::path::Path::new("/tmp/older-build-repo");
        let _ = std::fs::create_dir_all(crate::config::cache_dir());
        std::fs::write(
            crate::config::home_cache_path(path),
            r#"{"branch":"main","dirty":2}"#,
        )
        .unwrap();
        let got = load_home_cache(path).expect("partial row should still load");
        assert_eq!(got.branch, "main");
        assert_eq!(got.dirty, 2);
        assert_eq!(got.open_prs, None);
    }
}
