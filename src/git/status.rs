use super::contributors::process_contributor_log;
use super::diff::unquote_path;
use super::exec::{check_safe_ref, parse_ssh_repo, run_git_batch, run_git_cmd};
use super::types::{
    BranchInfo, FileExtInfo, FileInfo, FilesReport, GitOpState, LastFetch, RemoteCiPrInfo, Summary,
    SyncStatus, TagInfo, TechInfo, TechRule, UpstreamState, VersionFileRule, WorktreeInfo,
};
use crate::config::Member;
use chrono::{Local, TimeZone, Utc};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Read at most `cap` bytes of a repository file. `fs::read_to_string` would
/// pull the whole file into memory first, so a repository shipping a 4 GiB
/// README could exhaust it before any cap was applied.
pub fn read_file_capped(path: &Path, cap: usize) -> Option<String> {
    use std::io::Read;
    let mut buf = Vec::new();
    std::fs::File::open(path)
        .ok()?
        .take(cap as u64)
        .read_to_end(&mut buf)
        .ok()?;
    Some(String::from_utf8_lossy(&buf).into_owned())
}

/// Byte budget for a README before [`cap_readme`] trims it to a char count.
const MAX_README_BYTES: usize = 4 * 300_000;
/// Byte budget for the small manifests scanned by tech detection.
const MAX_MANIFEST_BYTES: usize = 1024 * 1024;
/// Commits walked to count contributors. A full-history walk on a repo with
/// a million commits returns tens of MB of text on *every* home refresh.
pub const CONTRIBUTOR_LOG_LIMIT: &str = "50000";

/// Split a `|||`-delimited git format line into exactly `N` fields.
/// Trailing fields are allowed to be missing (an empty `%(subject)` produces a
/// short line), so absent ones come back as `""`.
pub fn split_fields<const N: usize>(line: &str) -> [&str; N] {
    let mut out = [""; N];
    for (slot, part) in out.iter_mut().zip(line.split("|||")) {
        *slot = part.trim();
    }
    out
}

/// Parse `git ls-tree -r --long HEAD` into (tracked file count, total bytes).
/// Git reports the blob sizes itself, which replaces one `fs::metadata` syscall
/// per tracked file — 300k stats per refresh on a monorepo, and syscalls that
/// cannot succeed at all for `ssh://` repositories.
pub fn parse_ls_tree_long(output: &str) -> (usize, u64) {
    let mut count = 0usize;
    let mut bytes = 0u64;
    for line in output.lines() {
        let Some((meta, _path)) = line.split_once('\t') else {
            continue;
        };
        let mut fields = meta.split_whitespace();
        let (_mode, kind, _oid, size) =
            (fields.next(), fields.next(), fields.next(), fields.next());
        // Submodules are `commit` entries whose size column is "-"
        if kind != Some("blob") {
            continue;
        }
        count += 1;
        bytes += size.and_then(|s| s.parse::<u64>().ok()).unwrap_or(0);
    }
    (count, bytes)
}

pub fn parse_sync_status(has_upstream: bool, counts_out: &str) -> SyncStatus {
    if !has_upstream {
        return SyncStatus {
            has_upstream: false,
            ahead: 0,
            behind: 0,
            has_remote: false,
        };
    }
    let parts: Vec<&str> = counts_out.trim().split('\t').collect();
    let (ahead, behind) = if parts.len() == 2 {
        (
            parts[0].parse::<usize>().unwrap_or(0),
            parts[1].parse::<usize>().unwrap_or(0),
        )
    } else {
        (0, 0)
    };
    SyncStatus {
        has_upstream: true,
        ahead,
        behind,
        has_remote: true,
    }
}

// Check if a path is a git repository
pub fn is_git_repo(repo_path: &Path) -> bool {
    if repo_path.join(".git").exists() {
        return true;
    }
    run_git_cmd(repo_path, &["rev-parse", "--is-inside-work-tree"]).is_ok()
}

/// Scan a directory for valid Git repositories up to `max_depth`.
/// If `root` itself is a Git repository, returns `vec![root]`.
pub fn find_git_repos(root: &Path, max_depth: usize) -> Vec<std::path::PathBuf> {
    let mut results = Vec::new();
    if !root.is_dir() {
        return results;
    }
    if is_git_repo(root) {
        results.push(root.to_path_buf());
        return results;
    }
    scan_git_repos_recursive(root, 1, max_depth, &mut results);
    results.sort();
    results.dedup();
    results
}

pub fn scan_git_repos_recursive(
    dir: &Path,
    current_depth: usize,
    max_depth: usize,
    results: &mut Vec<std::path::PathBuf>,
) {
    if current_depth > max_depth {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if name_str.starts_with('.')
            || name_str == "node_modules"
            || name_str == "target"
            || name_str == "vendor"
        {
            continue;
        }
        if is_git_repo(&path) {
            results.push(path);
        } else if current_depth < max_depth {
            scan_git_repos_recursive(&path, current_depth + 1, max_depth, results);
        }
    }
}

// Git pull repository

pub fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * KB;
    const GB: u64 = 1024 * MB;

    if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

// Get repository name
pub fn get_repo_name(repo_path: &Path) -> String {
    if let Ok(url) = run_git_cmd(repo_path, &["config", "--get", "remote.origin.url"]) {
        let url = url.trim();
        if !url.is_empty()
            && let Some(pos) = url.rfind('/')
        {
            let name = &url[pos + 1..];
            let name = name.trim_end_matches(".git");
            if !name.is_empty() {
                return name.to_string();
            }
        }
    }

    // SSH locators (ssh://…) are not local filesystem paths, so canonicalize
    // would fail; derive the name from the remote path's last segment instead.
    if let Some((_, remote_path)) = parse_ssh_repo(repo_path) {
        return repo_name_from_ssh_path(&remote_path)
            .unwrap_or_else(|| "不明なリポジトリ".to_string());
    }

    repo_path
        .canonicalize()
        .ok()
        .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
        .unwrap_or_else(|| "不明なリポジトリ".to_string())
}

/// Last path segment of an SSH remote path, used as the repository name
/// (`/home/dev/myrepo` → `myrepo`, `/srv/app.git` → `app`).
pub fn repo_name_from_ssh_path(remote_path: &str) -> Option<String> {
    let name = remote_path
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or("")
        .trim_end_matches(".git");
    if name.is_empty() {
        None
    } else {
        Some(name.to_string())
    }
}

// Format Unix timestamp to local time string
pub fn format_timestamp(ts: i64) -> String {
    if ts == 0 {
        return "不明".to_string();
    }
    if let Some(dt) = Utc.timestamp_opt(ts, 0).single() {
        dt.with_timezone(&Local)
            .format("%Y-%m-%d %H:%M:%S")
            .to_string()
    } else {
        "不明".to_string()
    }
}

pub fn get_summary(repo_path: &Path, members: &[Member]) -> Result<Summary, String> {
    let repo_name = get_repo_name(repo_path);
    // SSH locators (ssh://…) have no local path to canonicalize — doing so
    // failed with "os error 123" on Windows and, since get_summary is the one
    // mandatory analysis, turned the whole dashboard into an error. Use the
    // locator string as-is for those.
    let repo_path_str = if parse_ssh_repo(repo_path).is_some() {
        repo_path.to_string_lossy().to_string()
    } else {
        repo_path
            .canonicalize()
            .map_err(|e| e.to_string())?
            .to_string_lossy()
            .to_string()
    };

    // All summary metadata in one batch: over SSH this is a single connection
    // instead of ~10 separate handshakes (the dominant cost on remote repos).
    // The last four are existence checks for the pseudo-refs git creates for
    // an in-progress merge/rebase/cherry-pick/revert (present since 2.35 for
    // rebase, long-standing for the others) — a non-zero exit just means "not
    // in that state", not a failure.
    let cmds: [&[&str]; 13] = [
        &["branch", "--show-current"],
        &["rev-list", "--count", "HEAD"],
        &["branch", "-a"],
        &["ls-tree", "-r", "--long", "HEAD"],
        &["remote"],
        &["status", "--porcelain=v1", "--untracked-files=all"],
        &["rev-parse", "--abbrev-ref", "@{u}"],
        &["rev-list", "--left-right", "--count", "HEAD...@{u}"],
        &[
            "log",
            "-n",
            CONTRIBUTOR_LOG_LIMIT,
            "--format=%an|||%ae|||%at",
        ],
        &["rev-parse", "-q", "--verify", "MERGE_HEAD"],
        &["rev-parse", "-q", "--verify", "REBASE_HEAD"],
        &["rev-parse", "-q", "--verify", "CHERRY_PICK_HEAD"],
        &["rev-parse", "-q", "--verify", "REVERT_HEAD"],
    ];
    let r = run_git_batch(repo_path, &cmds);
    // "" on a failed command mirrors the previous per-call unwrap_or_default
    let out = |i: usize| -> &str {
        r.get(i)
            .and_then(|x| x.as_ref().ok())
            .map(String::as_str)
            .unwrap_or("")
    };

    let current_branch = out(0).trim().to_string();
    let total_commits: usize = out(1).trim().parse().unwrap_or(0);
    let total_branches = out(2)
        .lines()
        .filter(|l| !l.trim().is_empty() && !l.contains("->"))
        .count();

    let (total_files, total_size_bytes) = parse_ls_tree_long(out(3));

    let has_remote = !out(4).trim().is_empty();
    let uncommitted_changes = out(5).lines().filter(|l| !l.trim().is_empty()).count();
    let has_upstream = !out(6).trim().is_empty();
    let sync_status = parse_sync_status(has_upstream, out(7));
    let total_contributors = process_contributor_log(out(8), members).len();
    let conflicts = count_conflicts(out(5));
    let op_state = if r.get(9).is_some_and(|x| x.is_ok()) {
        GitOpState::Merge
    } else if rebase_in_progress(repo_path, r.get(10).is_some_and(|x| x.is_ok())) {
        GitOpState::Rebase
    } else if r.get(11).is_some_and(|x| x.is_ok()) {
        GitOpState::CherryPick
    } else if r.get(12).is_some_and(|x| x.is_ok()) {
        GitOpState::Revert
    } else {
        GitOpState::None
    };

    let remote_ci_pr = get_github_status(repo_path);

    Ok(Summary {
        repo_name,
        repo_path: repo_path_str,
        current_branch,
        total_commits,
        total_contributors,
        total_branches,
        total_files,
        total_size_bytes,
        total_size_formatted: format_size(total_size_bytes),
        has_upstream: sync_status.has_upstream,
        ahead: sync_status.ahead,
        behind: sync_status.behind,
        has_remote,
        uncommitted_changes,
        remote_ci_pr,
        op_state,
        conflicts,
    })
}

/// The repository state a Home-list row actually renders.
///
/// Home refreshes *every* configured repository on each reload, which makes it
/// the hottest git path in the app, yet a row only shows the branch, the
/// ahead/behind counts, the dirty/conflict counts, the in-progress operation
/// badge and the GitHub CI/PR badge. [`Summary`] additionally walks the whole
/// history (`rev-list --count HEAD` plus a 50k-commit contributor log), the
/// whole tree (`ls-tree -r --long HEAD`) and every ref (`branch -a`) for the
/// repository-detail screen. Measured on a synthetic 20k-commit / 3k-file
/// repository (git 2.53), the full set costs ~105 ms against ~10 ms for the
/// commands below — per repository, per refresh.
///
/// This is a purpose-built struct rather than a partially filled [`Summary`]
/// on purpose: a `Summary` whose `total_commits` is `0` merely because nobody
/// asked for it is indistinguishable from an empty repository, and the next
/// reader would have no way to tell the difference.
#[derive(Debug, Clone, Default)]
pub struct HomeSummary {
    pub current_branch: String,
    /// What the current branch tracks. Without it `ahead`/`behind` being zero
    /// is ambiguous between "in sync" and "never compared to anything".
    pub upstream: UpstreamState,
    pub ahead: usize,
    pub behind: usize,
    pub uncommitted_changes: usize,
    /// `uncommitted_changes` split into the two halves a reader acts on
    /// differently: edits to tracked files versus untracked files (which are
    /// often build output). Their sum is `uncommitted_changes`.
    pub tracked_changes: usize,
    pub untracked: usize,
    pub conflicts: usize,
    pub op_state: GitOpState,
    /// Age of the *remote* half of this row — see [`LastFetch`].
    pub last_fetch: LastFetch,
    pub remote_ci_pr: Option<RemoteCiPrInfo>,
}

/// Collect exactly the state a Home row displays — see [`HomeSummary`] for why
/// this exists next to [`get_summary`] instead of reusing it.
///
/// Like `get_summary` this issues a *single* [`run_git_batch`]: over SSH that
/// is one connection handshake for all eight commands, so splitting it into
/// separate `run_git_cmd` calls would cost far more than the statistics this
/// skips ever saved.
pub fn get_home_summary(repo_path: &Path) -> Result<HomeSummary, String> {
    // Same early error as `get_summary`: a repository that was moved or
    // deleted must surface as an error row rather than a silently empty one.
    // SSH locators (ssh://…) have no local path to canonicalize, so they are
    // exempt here exactly as they are there.
    if parse_ssh_repo(repo_path).is_none() {
        repo_path.canonicalize().map_err(|e| e.to_string())?;
    }

    // Commands 4-7 are existence checks for the pseudo-refs git creates for
    // an in-progress merge/rebase/cherry-pick/revert; a non-zero exit just
    // means "not in that state", not a failure.
    //
    // The last four are appended to this batch rather than run separately so
    // an `ssh://` repository still costs exactly one connection:
    //   `--git-dir`        — the honest "is this a git repository at all"
    //                        check. Without it a registered directory that is
    //                        not a repository renders as a pristine one,
    //                        because every other command fails into "".
    //   `--git-path FETCH_HEAD` and `--git-common-dir` — the two places a
    //                        FETCH_HEAD can live; see [`last_fetch_time`].
    //   `remote`           — tells "no remote at all" apart from "remote, but
    //                        this branch was never pushed".
    let cmds: [&[&str]; 12] = [
        &["branch", "--show-current"],
        &["status", "--porcelain=v1", "--untracked-files=all"],
        &["rev-parse", "--abbrev-ref", "@{u}"],
        &["rev-list", "--left-right", "--count", "HEAD...@{u}"],
        &["rev-parse", "-q", "--verify", "MERGE_HEAD"],
        &["rev-parse", "-q", "--verify", "REBASE_HEAD"],
        &["rev-parse", "-q", "--verify", "CHERRY_PICK_HEAD"],
        &["rev-parse", "-q", "--verify", "REVERT_HEAD"],
        &["rev-parse", "--git-dir"],
        &["rev-parse", "--git-path", "FETCH_HEAD"],
        &["remote"],
        &["rev-parse", "--git-common-dir"],
    ];
    let r = run_git_batch(repo_path, &cmds);
    // "" on a failed command, mirroring `get_summary`
    let out = |i: usize| -> &str {
        r.get(i)
            .and_then(|x| x.as_ref().ok())
            .map(String::as_str)
            .unwrap_or("")
    };

    // `rev-parse --git-dir` is the one command here that cannot legitimately
    // fail for a real repository, so its failure is the failure of the row.
    if let Some(Err(e)) = r.get(8) {
        return Err(format!(
            "{NOT_A_REPOSITORY}: {} ({})",
            repo_path.display(),
            e.trim()
        ));
    }

    let status_porcelain = out(1);
    let has_upstream = !out(2).trim().is_empty();
    let sync_status = parse_sync_status(has_upstream, out(3));
    let op_state = if r.get(4).is_some_and(|x| x.is_ok()) {
        GitOpState::Merge
    } else if rebase_in_progress(repo_path, r.get(5).is_some_and(|x| x.is_ok())) {
        GitOpState::Rebase
    } else if r.get(6).is_some_and(|x| x.is_ok()) {
        GitOpState::CherryPick
    } else if r.get(7).is_some_and(|x| x.is_ok()) {
        GitOpState::Revert
    } else {
        GitOpState::None
    };

    let (tracked_changes, untracked) = parse_status_counts(status_porcelain);
    let upstream = if has_upstream {
        UpstreamState::Tracking
    } else if out(10).trim().is_empty() {
        UpstreamState::NoRemote
    } else {
        UpstreamState::NoUpstream
    };

    Ok(HomeSummary {
        current_branch: out(0).trim().to_string(),
        upstream,
        ahead: sync_status.ahead,
        behind: sync_status.behind,
        uncommitted_changes: tracked_changes + untracked,
        tracked_changes,
        untracked,
        conflicts: count_conflicts(status_porcelain),
        op_state,
        last_fetch: last_fetch_time(
            repo_path,
            r.get(9).and_then(|x| x.as_ref().ok()),
            r.get(11).and_then(|x| x.as_ref().ok()),
        ),
        remote_ci_pr: get_github_status(repo_path),
    })
}

/// Marker opening the error a Home row carries when its registered path is a
/// directory but not a git repository. Matched (not parsed) by the UI to pick
/// a short localized reason, so it must stay stable.
pub const NOT_A_REPOSITORY: &str = "not a git repository";

/// Split `git status --porcelain=v1` into (tracked changes, untracked files).
///
/// `??` is the porcelain code for an untracked path; with
/// `--untracked-files=all` each file inside an untracked directory gets its
/// own line, which is the count we want — "5 untracked files", not
/// "1 untracked directory". Anything else non-empty is a change to a tracked
/// file. (Over SSH a stderr line can end up in this stream — see
/// [`count_conflicts`] — and counts as a tracked change, exactly as it did
/// when this was one undifferentiated total.)
pub fn parse_status_counts(status_porcelain: &str) -> (usize, usize) {
    let mut tracked = 0usize;
    let mut untracked = 0usize;
    for line in status_porcelain.lines() {
        if line.trim().is_empty() {
            continue;
        }
        if line.starts_with("??") {
            untracked += 1;
        } else {
            tracked += 1;
        }
    }
    (tracked, untracked)
}

/// Last fetch time from the mtime of `FETCH_HEAD`. git rewrites that file on
/// every fetch and pull and nothing else touches it, so its mtime is the age
/// of the ahead/behind counts.
///
/// Two candidates, both resolved by git in the Home batch rather than
/// assembled here as `<repo>/.git/FETCH_HEAD` (which is wrong for linked
/// worktrees, submodules and separate git dirs alike):
/// * `git_path` — `rev-parse --git-path FETCH_HEAD`, the *per-worktree* file,
///   written when the fetch was run from this worktree;
/// * `common_dir` — `rev-parse --git-common-dir`, whose `FETCH_HEAD` is the
///   one a fetch from the main worktree wrote.
///
/// The remote-tracking refs those counts compare against are shared by every
/// worktree, so the newer of the two files is the honest answer: reporting
/// "never fetched" in a linked worktree whose numbers were refreshed from
/// next door is exactly the kind of stale-looking lie this reports against.
fn last_fetch_time(
    repo_path: &Path,
    git_path: Option<&String>,
    common_dir: Option<&String>,
) -> LastFetch {
    // An `ssh://` repository has no local file to stat; reporting "never
    // fetched" there would be a guess, and the wrong one.
    if parse_ssh_repo(repo_path).is_some() {
        return LastFetch::Unavailable;
    }
    let absolute = |resolved: &str| -> PathBuf {
        if Path::new(resolved).is_absolute() {
            PathBuf::from(resolved)
        } else {
            repo_path.join(resolved)
        }
    };
    // An empty output would resolve to the repository directory itself,
    // whose mtime has nothing to do with fetching.
    let non_empty = |s: &&String| !s.trim().is_empty();
    let candidates: Vec<PathBuf> = [
        git_path.filter(non_empty).map(|s| absolute(s.trim())),
        common_dir
            .filter(non_empty)
            .map(|s| absolute(s.trim()).join("FETCH_HEAD")),
    ]
    .into_iter()
    .flatten()
    .collect();
    if candidates.is_empty() {
        return LastFetch::Unknown;
    }
    candidates
        .iter()
        .filter_map(|p| std::fs::metadata(p).ok())
        .filter_map(|m| m.modified().ok())
        .filter_map(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .max()
        .map_or(LastFetch::Never, LastFetch::At)
}

/// Whether a rebase is genuinely in progress.
///
/// `REBASE_HEAD` (the batched `rev-parse -q --verify` result passed in as
/// `rebase_head_exists`) is **not** reliable on its own: git leaves that
/// pseudo-ref file behind after `rebase --continue` completes successfully,
/// while it removes `.git/rebase-merge`/`.git/rebase-apply` correctly
/// (verified against real git 2.53). For local repositories this resolves
/// the actual git-dir (via `--git-path`, which handles worktrees correctly —
/// rebase state is per-worktree) and checks those directories directly.
///
/// SSH repositories have no local filesystem to check, so they fall back to
/// the `REBASE_HEAD` heuristic and can still show a stale ⚠REBASE badge
/// after `rebase --continue`; this is a known gap, not a regression (it is
/// the same signal used everywhere before this fix).
fn rebase_in_progress(repo_path: &Path, rebase_head_exists: bool) -> bool {
    if parse_ssh_repo(repo_path).is_some() {
        return rebase_head_exists;
    }
    for git_path in ["rebase-merge", "rebase-apply"] {
        let Ok(resolved) = run_git_cmd(repo_path, &["rev-parse", "--git-path", git_path]) else {
            return rebase_head_exists;
        };
        let resolved = resolved.trim();
        let dir = if Path::new(resolved).is_absolute() {
            PathBuf::from(resolved)
        } else {
            repo_path.join(resolved)
        };
        if dir.is_dir() {
            return true;
        }
    }
    false
}

/// Count entries `git status --porcelain=v1` marks unmerged — i.e. files with
/// an unresolved merge/rebase/cherry-pick conflict. Git documents exactly
/// these seven XY combinations for unmerged paths; the code is always two
/// ASCII letters at the start of the line, so byte-slicing it is safe
/// regardless of what non-ASCII bytes the path itself might contain.
pub fn count_conflicts(status_porcelain: &str) -> usize {
    const UNMERGED: [&str; 7] = ["DD", "AU", "UD", "UA", "DU", "AA", "UU"];
    // `starts_with` rather than a `&l[..2]` byte slice: over SSH, stderr is
    // merged into the same stream (`run_git_batch`'s `2>&1`) while the batch
    // still reports success, so a line here isn't guaranteed to be genuine
    // porcelain output — it could start with a multi-byte character, which a
    // fixed byte-index slice would panic on.
    status_porcelain
        .lines()
        .filter(|l| UNMERGED.iter().any(|code| l.starts_with(code)))
        .count()
}

pub fn get_branches(repo_path: &Path) -> Result<Vec<BranchInfo>, String> {
    let output = match run_git_cmd(
        repo_path,
        &[
            "branch",
            "-a",
            "--format=%(refname:short)|||%(committerdate:unix)|||%(authorname)|||%(subject)",
        ],
    ) {
        Ok(data) => data,
        Err(_) => return Ok(Vec::new()),
    };

    let mut branches = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for line in output.lines() {
        let [refname, date_raw, author, message] = split_fields::<4>(line);
        let refname = refname.to_string();
        if refname.is_empty() || refname == "HEAD" || refname.contains("->") {
            continue;
        }

        if seen.contains(&refname) {
            continue;
        }
        seen.insert(refname.clone());

        let is_remote = refname.starts_with("remotes/") || refname.starts_with("origin/");
        let clean_name = refname.trim_start_matches("remotes/").to_string();

        let date_unix: i64 = date_raw.parse().unwrap_or(0);
        let author = author.to_string();
        let message = message.to_string();

        let date_str = format_timestamp(date_unix);

        branches.push(BranchInfo {
            name: clean_name,
            is_remote,
            author,
            date: date_str,
            date_unix,
            message,
        });
    }

    branches.sort_by_key(|b| std::cmp::Reverse(b.date_unix));

    Ok(branches)
}

pub fn get_tags(repo_path: &Path) -> Result<Vec<TagInfo>, String> {
    let output = match run_git_cmd(
        repo_path,
        &[
            "tag",
            "-l",
            "--sort=-creatordate",
            "--format=%(refname:short)|||%(creatordate:short)|||%(subject)|||%(committerdate:short)|||%(objectname:short)|||%(*objectname:short)",
        ],
    ) {
        Ok(data) => data,
        Err(_) => return Ok(Vec::new()),
    };

    let mut tags = Vec::new();
    for line in output.lines() {
        let [
            name,
            creatordate,
            subject,
            committerdate,
            objectname,
            deref_objectname,
        ] = split_fields::<6>(line);
        let name = name.to_string();
        if name.is_empty() {
            continue;
        }

        // %(*objectname) is the dereferenced commit of an annotated tag and is
        // empty for lightweight tags, where %(objectname) already is the commit
        let hash = if deref_objectname.is_empty() {
            objectname.to_string()
        } else {
            deref_objectname.to_string()
        };

        let date = if !creatordate.is_empty() {
            creatordate.to_string()
        } else if !committerdate.is_empty() {
            committerdate.to_string()
        } else {
            "不明".to_string()
        };

        let message = if !subject.is_empty() {
            subject.to_string()
        } else {
            // The tag name comes from git, but it reaches git again as an
            // argument here — `--` and the ref check keep a name like
            // `--output=...` from being read as an option.
            let msg = check_safe_ref(&name).ok().and_then(|()| {
                run_git_cmd(repo_path, &["log", "-1", "--format=%s", &name, "--"]).ok()
            });
            match msg.as_deref().map(str::trim) {
                Some("") => "メッセージなし".to_string(),
                Some(m) => m.to_string(),
                None => "No message".to_string(),
            }
        };

        tags.push(TagInfo {
            name,
            date,
            message,
            hash,
        });
    }

    Ok(tags)
}

pub fn get_files_report(repo_path: &Path) -> Result<FilesReport, String> {
    let output = match run_git_cmd(repo_path, &["ls-tree", "-r", "--long", "HEAD"]) {
        Ok(data) => data,
        Err(_) => {
            return Ok(FilesReport {
                extensions: vec![],
                largest_files: vec![],
            });
        }
    };

    let mut ext_map = HashMap::new();
    let mut all_files = Vec::new();
    let mut total_files_count = 0;

    for line in output.lines() {
        let Some((meta, raw_path)) = line.split_once('\t') else {
            continue;
        };
        let mut fields = meta.split_whitespace();
        let (_mode, kind, _oid, size) =
            (fields.next(), fields.next(), fields.next(), fields.next());
        if kind != Some("blob") {
            continue;
        }
        let size_bytes = size.and_then(|s| s.parse::<u64>().ok()).unwrap_or(0);
        // Paths with spaces or non-ASCII bytes come back C-quoted
        let file_path = unquote_path(raw_path);
        if file_path.is_empty() {
            continue;
        }

        total_files_count += 1;

        let path_obj = Path::new(&file_path);
        let ext = path_obj
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("other")
            .to_lowercase();

        *ext_map.entry(ext).or_insert(0) += 1;

        all_files.push(FileInfo {
            path: file_path,
            size_bytes,
            size_formatted: format_size(size_bytes),
        });
    }

    let mut extensions: Vec<FileExtInfo> = ext_map
        .into_iter()
        .map(|(ext, count)| {
            let percentage = if total_files_count > 0 {
                (count as f64 / total_files_count as f64) * 100.0
            } else {
                0.0
            };
            FileExtInfo {
                ext,
                count,
                percentage,
            }
        })
        .collect();
    extensions.sort_by_key(|b| std::cmp::Reverse(b.count));

    all_files.sort_by_key(|b| std::cmp::Reverse(b.size_bytes));
    let largest_files = all_files.into_iter().take(10).collect();

    Ok(FilesReport {
        extensions,
        largest_files,
    })
}

// Find project README file and read its content
/// Guard the Markdown renderer (and the disk cache) against pathological
/// README sizes; 300k chars is far beyond any README meant for humans.
pub fn cap_readme(mut content: String) -> String {
    const MAX_CHARS: usize = 300_000;
    if content.chars().count() > MAX_CHARS {
        content = content.chars().take(MAX_CHARS).collect();
        content.push_str("\n\n... (truncated due to large file size)");
    }
    content
}

pub fn find_readme(repo_path: &Path) -> Option<(String, String)> {
    const CANDIDATES: [&str; 5] = [
        "README.md",
        "README.txt",
        "README.rst",
        "readme.md",
        "Readme.md",
    ];

    // Remote (ssh://) repositories have no local files: read via `git show`
    if parse_ssh_repo(repo_path).is_some() {
        for name in CANDIDATES {
            if let Ok(content) = run_git_cmd(repo_path, &["show", &format!("HEAD:{name}")]) {
                return Some((name.to_string(), cap_readme(content)));
            }
        }
        return None;
    }

    for name in CANDIDATES {
        let p = repo_path.join(name);
        if let Some(content) = read_file_capped(&p, MAX_README_BYTES) {
            return Some((name.to_string(), cap_readme(content)));
        }
    }
    None
}

pub fn get_default_rules() -> Vec<TechRule> {
    vec![
        TechRule {
            name: "Ruby on Rails".to_string(),
            language: "Ruby".to_string(),
            lang_version_files: vec![
                VersionFileRule {
                    file: ".ruby-version".to_string(),
                    regex: r"(?:ruby-)?([0-9.]+)".to_string(),
                },
                VersionFileRule {
                    file: "Gemfile.lock".to_string(),
                    regex: r"RUBY VERSION\s+ruby\s+([0-9.]+)".to_string(),
                },
            ],
            framework: "Rails".to_string(),
            framework_version_files: vec![VersionFileRule {
                file: "Gemfile.lock".to_string(),
                regex: r"\srails\s+\(([0-9.]+)\)".to_string(),
            }],
        },
        TechRule {
            name: "Node.js".to_string(),
            language: "JavaScript/TypeScript".to_string(),
            lang_version_files: vec![
                VersionFileRule {
                    file: ".nvmrc".to_string(),
                    regex: r"^v?([0-9.]+)".to_string(),
                },
                VersionFileRule {
                    file: "package.json".to_string(),
                    regex: r#""node"\s*:\s*"[^"0-9]*([0-9.]+)"#.to_string(),
                },
            ],
            framework: "Next/React/Express".to_string(),
            framework_version_files: vec![
                VersionFileRule {
                    file: "package.json".to_string(),
                    regex: r#""next"\s*:\s*"[^"0-9]*([0-9.]+)"#.to_string(),
                },
                VersionFileRule {
                    file: "package.json".to_string(),
                    regex: r#""express"\s*:\s*"[^"0-9]*([0-9.]+)"#.to_string(),
                },
                VersionFileRule {
                    file: "package.json".to_string(),
                    regex: r#""react"\s*:\s*"[^"0-9]*([0-9.]+)"#.to_string(),
                },
            ],
        },
        TechRule {
            name: "Django".to_string(),
            language: "Python".to_string(),
            lang_version_files: vec![
                VersionFileRule {
                    file: ".python-version".to_string(),
                    regex: r"^([0-9.]+)".to_string(),
                },
                VersionFileRule {
                    file: "runtime.txt".to_string(),
                    regex: r"python-([0-9.]+)".to_string(),
                },
            ],
            framework: "Django".to_string(),
            framework_version_files: vec![
                VersionFileRule {
                    file: "requirements.txt".to_string(),
                    regex: r"(?:[Dd]jango|Django)==([0-9.]+)".to_string(),
                },
                VersionFileRule {
                    file: "Pipfile".to_string(),
                    regex: r#"(?:django|Django)\s*=\s*"[^"0-9]*([0-9.]+)"#.to_string(),
                },
            ],
        },
    ]
}

pub fn detect_tech_info(repo_path: &Path) -> TechInfo {
    let config_dir = crate::config::get_config_dir();
    let rules_path = config_dir.join("tech_rules.json");
    let rules: Vec<TechRule> = if rules_path.exists() {
        if let Ok(content) = std::fs::read_to_string(&rules_path) {
            serde_json::from_str(&content).unwrap_or_else(|_| get_default_rules())
        } else {
            get_default_rules()
        }
    } else {
        let r = get_default_rules();
        if let Ok(content) = serde_json::to_string_pretty(&r) {
            let _ = crate::config::write_atomic(&rules_path, &content);
        }
        r
    };

    detect_tech_info_with_rules(repo_path, &rules)
}

pub fn detect_tech_info_with_rules(repo_path: &Path, rules: &[TechRule]) -> TechInfo {
    let mut tech_info = TechInfo {
        language: "-".to_string(),
        lang_version: "-".to_string(),
        framework: "-".to_string(),
        framework_version: "-".to_string(),
    };

    for rule in rules {
        let mut lang_version = None;
        let mut framework_version = None;
        let mut matched_lang = false;
        let mut matched_fw = false;

        // Check language version files
        for vf in &rule.lang_version_files {
            let file_path = repo_path.join(&vf.file);
            if file_path.exists() {
                matched_lang = true;
                if let Some(content) = read_file_capped(&file_path, MAX_MANIFEST_BYTES)
                    && let Ok(re) = regex::Regex::new(&vf.regex)
                    && let Some(caps) = re.captures(&content)
                    && let Some(m) = caps.get(1)
                {
                    lang_version = Some(m.as_str().to_string());
                    break;
                }
            }
        }

        // Check framework version files
        for vf in &rule.framework_version_files {
            let file_path = repo_path.join(&vf.file);
            if file_path.exists() {
                matched_fw = true;
                if let Some(content) = read_file_capped(&file_path, MAX_MANIFEST_BYTES)
                    && let Ok(re) = regex::Regex::new(&vf.regex)
                    && let Some(caps) = re.captures(&content)
                    && let Some(m) = caps.get(1)
                {
                    framework_version = Some(m.as_str().to_string());
                    break;
                }
            }
        }

        if matched_lang || matched_fw {
            tech_info.language = rule.language.clone();
            tech_info.lang_version = lang_version.unwrap_or_else(|| "-".to_string());
            tech_info.framework = rule.framework.clone();
            tech_info.framework_version = framework_version.unwrap_or_else(|| "-".to_string());
            break;
        }
    }

    tech_info
}

pub fn parse_worktrees(output: &str) -> Vec<WorktreeInfo> {
    let mut list = Vec::new();
    let mut cur_path = String::new();
    let mut cur_head = String::new();
    let mut cur_branch = None;
    let mut is_bare = false;
    let mut is_detached = false;
    let mut is_locked = false;
    let mut is_prunable = false;

    let flush = |list: &mut Vec<WorktreeInfo>,
                 path: &mut String,
                 head: &mut String,
                 branch: &mut Option<String>,
                 bare: &mut bool,
                 detached: &mut bool,
                 locked: &mut bool,
                 prunable: &mut bool| {
        if !path.is_empty() {
            list.push(WorktreeInfo {
                path: std::mem::take(path),
                head: std::mem::take(head),
                branch: branch.take(),
                is_bare: *bare,
                is_detached: *detached,
                is_locked: *locked,
                is_prunable: *prunable,
            });
            *bare = false;
            *detached = false;
            *locked = false;
            *prunable = false;
        }
    };

    for line in output.lines() {
        let line = line.trim();
        if line.is_empty() {
            flush(
                &mut list,
                &mut cur_path,
                &mut cur_head,
                &mut cur_branch,
                &mut is_bare,
                &mut is_detached,
                &mut is_locked,
                &mut is_prunable,
            );
        } else if let Some(p) = line.strip_prefix("worktree ").map(unquote_path).as_deref() {
            flush(
                &mut list,
                &mut cur_path,
                &mut cur_head,
                &mut cur_branch,
                &mut is_bare,
                &mut is_detached,
                &mut is_locked,
                &mut is_prunable,
            );
            cur_path = p.to_string();
        } else if let Some(h) = line.strip_prefix("HEAD ") {
            cur_head = h.to_string();
        } else if let Some(b) = line.strip_prefix("branch ") {
            // Non-ASCII refs and paths come back C-quoted from --porcelain
            let b = unquote_path(b);
            let branch_name = b.strip_prefix("refs/heads/").unwrap_or(&b);
            cur_branch = Some(branch_name.to_string());
        } else if line == "bare" {
            is_bare = true;
        } else if line == "detached" {
            is_detached = true;
        } else if line.starts_with("locked") {
            is_locked = true;
        } else if line.starts_with("prunable") {
            is_prunable = true;
        }
    }

    flush(
        &mut list,
        &mut cur_path,
        &mut cur_head,
        &mut cur_branch,
        &mut is_bare,
        &mut is_detached,
        &mut is_locked,
        &mut is_prunable,
    );

    list
}

pub fn get_worktrees(repo_path: &Path) -> Result<Vec<WorktreeInfo>, String> {
    let output = run_git_cmd(repo_path, &["worktree", "list", "--porcelain"])?;
    Ok(parse_worktrees(&output))
}

pub fn parse_gh_prs(json: &str) -> Option<usize> {
    let val: serde_json::Value = serde_json::from_str(json).ok()?;
    let arr = val.as_array()?;
    Some(arr.len())
}

pub fn parse_gh_runs(json: &str) -> (Option<String>, Option<String>) {
    let val: serde_json::Value = serde_json::from_str(json)
        .ok()
        .unwrap_or(serde_json::Value::Null);
    let arr = val.as_array();
    let first = arr.and_then(|a| a.first());
    let conclusion = first.and_then(|r| {
        r.get("conclusion")
            .and_then(|c| c.as_str())
            .filter(|s| !s.is_empty())
            .or_else(|| r.get("status").and_then(|s| s.as_str()))
            .map(|s| s.to_string())
    });
    let url = first.and_then(|r| r.get("url").and_then(|u| u.as_str()).map(|s| s.to_string()));
    (conclusion, url)
}

pub fn get_github_status(repo_path: &Path) -> Option<RemoteCiPrInfo> {
    super::github::get_status(repo_path)
}
