use super::types::*;
use crate::config::{self, Member};
use crate::git::{
    self, BranchInfo, CommitSummary, Contributor, StashEntry, Summary, TagInfo, TimeSpan,
    WorktreeInfo,
};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::thread;

/// `home_gen` is the newest home-refresh generation the app has issued. The
/// queue is unbounded and holding `r` (or a bulk pull) enqueues one `LoadHome`
/// per repository per refresh, so superseded ones are dropped here rather than
/// each running a full repository analysis whose result is then discarded.
pub fn spawn_worker(job_rx: Receiver<Job>, msg_tx: Sender<Msg>, home_gen: Arc<AtomicU64>) {
    thread::spawn(move || {
        while let Ok(job) = job_rx.recv() {
            let msg = match job {
                Job::LoadHome {
                    generation,
                    index,
                    path,
                    members,
                } => {
                    if generation < home_gen.load(Ordering::Relaxed) {
                        continue;
                    }
                    let row = load_home_row(&path, &members);
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
                Job::Pull { index, path, lang } => {
                    op_done(git::pull_repository(&path, lang), Some(index))
                }
                Job::Fetch { index, path, lang } => {
                    op_done(git::fetch_repository(&path, lang), Some(index))
                }
                Job::StashApply { path, stash_ref } => {
                    op_done(git::apply_stash(&path, &stash_ref), None)
                }
                Job::StashDrop { path, stash_ref } => {
                    op_done(git::drop_stash(&path, &stash_ref), None)
                }
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
            if msg_tx.send(msg).is_err() {
                break;
            }
        }
    });
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

pub fn load_home_row(path: &std::path::Path, members: &[Member]) -> Result<HomeRow, String> {
    let summary = git::get_summary(path, members)?;
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
        branch: summary.current_branch,
        ahead: summary.ahead,
        behind: summary.behind,
        dirty: summary.uncommitted_changes,
        last_commit,
        open_prs,
        ci_status,
        op_state: summary.op_state,
        conflicts: summary.conflicts,
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
