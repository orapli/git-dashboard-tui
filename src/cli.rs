//! Non-interactive surface: command-line parsing, the `--json` status
//! snapshot, and the ephemeral "focus this repository" start-up.
//!
//! Everything here has to work with **no terminal at all** — `--json` is meant
//! to be shelled out to from a status line, a prompt or a script, so nothing
//! in this module may initialise ratatui, write to the alternate screen, or
//! block on input. The only exception is [`focus_repository`], which mutates
//! an already-constructed [`App`] and is called from `crate::run_with_focus`.
//!
//! Output here is English-only on purpose. The TUI is bilingual because it is
//! read by a person; `--json` and the usage text are read by `jq`, by a shell
//! script and by whoever wrote it, so they match the `HELP` constant in
//! `src/main.rs` rather than going through `App::tt`.

use crate::app::App;
use crate::config::{self, Repository};
use crate::git::{self, GitOpState, GithubState, RemoteCiPrInfo};
use serde::Serialize;
use std::ffi::OsString;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Version of the `--json` document. Bump it only for a **breaking** change:
/// adding a field is not one, because the top level is an object rather than
/// a bare array precisely so consumers can ignore what they don't know.
pub const SCHEMA_VERSION: u32 = 1;

/// Stable identifier so a consumer can tell this document apart from any
/// other JSON it might be handed.
pub const SCHEMA_ID: &str = "git-dashboard-tui.status-snapshot";

/// Upper bound on concurrent repository analyses. One repository is ~10 git
/// invocations; a serial loop over 30 repositories is several seconds of
/// nothing, and an unbounded fan-out would spawn 30 × 10 processes at once on
/// a laptop. Eight keeps the wall time near the slowest repository without
/// turning a status-line call into a fork bomb.
const MAX_SNAPSHOT_THREADS: usize = 8;

/// Exit code for a command line that could not be parsed, matching what the
/// binary has always returned for an unrecognised argument.
pub const EXIT_USAGE: u8 = 2;

// ---------------------------------------------------------------------------
// Argument parsing
// ---------------------------------------------------------------------------

/// What the command line asked for. Parsing is deliberately a pure function
/// over the arguments so it can be tested without spawning the binary and
/// without a terminal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Invocation {
    /// `-h` / `--help`
    Help,
    /// `-V` / `--version`
    Version,
    /// Start the TUI, optionally focused on `path`.
    Dashboard { path: Option<PathBuf> },
    /// Print the JSON snapshot and exit; `path` limits it to one repository.
    Snapshot { path: Option<PathBuf> },
}

/// Parse the arguments *after* `argv[0]`.
///
/// Hand-rolled on purpose (no `clap`): the whole surface is three flags and
/// one operand, and a dependency would cost more than it saves.
///
/// Takes `OsString` rather than `String` because a repository path need not be
/// valid UTF-8 on Unix — `std::env::args()` would panic on one, so callers
/// should feed this `std::env::args_os().skip(1)`.
///
/// Precedence: an unrecognised flag is an error even if `--help` appears later
/// (a typo should not be silently answered with the help text), `--help` beats
/// `--version`, and both beat everything else. `--` ends flag parsing, so a
/// directory genuinely named `--json` can still be opened.
pub fn parse_args<I: IntoIterator<Item = OsString>>(args: I) -> Result<Invocation, String> {
    let mut path: Option<PathBuf> = None;
    let mut json = false;
    let mut help = false;
    let mut version = false;
    let mut operands_only = false;

    for arg in args {
        let text = arg.to_str();
        if !operands_only && text.is_some_and(|t| t.starts_with('-')) {
            match text.unwrap_or_default() {
                "--" => operands_only = true,
                "-h" | "--help" => help = true,
                "-V" | "--version" => version = true,
                // Repeating it is harmless rather than an error: a wrapper
                // script that always appends `--json` should not break when
                // the user also typed it.
                "--json" => json = true,
                other => return Err(format!("unrecognised argument '{other}'")),
            }
            continue;
        }
        if arg.is_empty() {
            return Err("PATH must not be empty".to_string());
        }
        if let Some(first) = path.as_ref() {
            return Err(format!(
                "only one PATH may be given (got '{}' and '{}')",
                first.display(),
                Path::new(&arg).display()
            ));
        }
        path = Some(PathBuf::from(arg));
    }

    if help {
        return Ok(Invocation::Help);
    }
    if version {
        return Ok(Invocation::Version);
    }
    if json {
        Ok(Invocation::Snapshot { path })
    } else {
        Ok(Invocation::Dashboard { path })
    }
}

/// Turn a `PATH` operand into an absolute repository path, or explain why it
/// is not one.
///
/// Called *before* the terminal is touched, for the same reason `--version` is
/// handled before `ratatui::init()`: a message printed after the alternate
/// screen is entered and left is easy to miss, and a panic there leaves the
/// terminal in raw mode.
pub fn resolve_repository_path(path: &Path) -> Result<PathBuf, String> {
    // An `ssh://` locator is not a local filesystem path — canonicalize would
    // fail on it — so it is taken at face value, as the git layer does.
    if git::parse_ssh_repo(path).is_some() {
        return Ok(path.to_path_buf());
    }
    let resolved = path
        .canonicalize()
        .map_err(|e| format!("cannot open '{}': {e}", path.display()))?;
    if !resolved.is_dir() {
        return Err(format!("not a directory: {}", resolved.display()));
    }
    if !git::is_git_repo(&resolved) {
        return Err(format!("not a git repository: {}", resolved.display()));
    }
    Ok(resolved)
}

// ---------------------------------------------------------------------------
// Focused start-up
// ---------------------------------------------------------------------------

/// Show `path` in the dashboard and select it, **without registering it**.
///
/// The repository is appended to the in-memory list only. Writing it to
/// `config.json` from a command-line argument would silently grow the user's
/// configuration every time they ran `git-dashboard-tui .` in a scratch clone;
/// the argument asks for a view, not a subscription.
///
/// If `path` is already registered, the existing entry is selected instead of
/// a duplicate row being added — that is the common case (`cd` into a project
/// you already track) and the one where a second row would look like a bug.
pub fn focus_repository(app: &mut App, path: PathBuf) {
    let existing = app
        .repos
        .iter()
        .position(|repo| same_repository(&repo.path, &path));

    let index = match existing {
        Some(index) => index,
        None => {
            // Appended rather than inserted, so the indices `App::new`'s
            // in-flight refresh jobs were built with still point at the same
            // repositories.
            app.repos.push(Repository {
                name: format!(
                    "{} ({})",
                    git::get_repo_name(&path),
                    app.tt("not registered", "未登録")
                ),
                path,
                group: None,
            });
            app.repos.len() - 1
        }
    };

    // `App` exposes no public "queue a refresh" call, and the jobs queued by
    // `App::new()` predate the row we just added. `r` on Home is the gesture
    // that means exactly this, and `refresh_home` bumps the generation
    // counter, so the initial batch is discarded by the worker rather than
    // run twice.
    app.handle_key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char('r'),
        crossterm::event::KeyModifiers::NONE,
    ));

    app.home_selected = app
        .filtered_home()
        .iter()
        .position(|&i| i == index)
        .unwrap_or(0);

    if existing.is_none() {
        app.status = app.tt(
            "Showing the repository given on the command line — not saved to config.json",
            "コマンドラインで指定したリポジトリを表示中 — config.json には保存しません",
        );
    }
}

/// Whether two configured paths denote the same repository. Compared after
/// canonicalisation where possible so `.`, a trailing slash and a symlinked
/// home directory all match a registered entry instead of duplicating it.
fn same_repository(a: &Path, b: &Path) -> bool {
    if a == b {
        return true;
    }
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

// ---------------------------------------------------------------------------
// --json document
// ---------------------------------------------------------------------------

/// Top level of the `--json` document.
///
/// An object rather than a bare array: an array has nowhere to put a schema
/// version or a generation time, and cannot gain one later without breaking
/// every consumer that indexes into it.
#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
pub struct Snapshot {
    pub schema: &'static str,
    pub schema_version: u32,
    pub tool_version: &'static str,
    /// RFC 3339, UTC.
    pub generated_at: String,
    pub generated_at_unix: i64,
    pub repository_count: usize,
    pub repositories: Vec<RepositoryStatus>,
}

/// One repository's line in the snapshot.
///
/// Every measured field is an `Option` and is always serialised, `null`
/// included: a repository whose path has vanished must be distinguishable
/// from a clean one, and `"ahead": 0` on a repository we could not read would
/// be a lie that a status line has no way to detect.
#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
pub struct RepositoryStatus {
    /// The name as configured (or derived from the remote, for a path given
    /// on the command line).
    pub name: String,
    pub path: String,
    pub group: Option<String>,
    /// `false` for a repository named on the command line that is not in
    /// `config.json`.
    pub registered: bool,
    /// `false` when the analysis failed; every field below is then `null`.
    pub ok: bool,
    pub error: Option<String>,
    pub branch: Option<String>,
    pub has_upstream: Option<bool>,
    /// `null` when there is no upstream — a branch with nothing to compare
    /// against is not "0 ahead, 0 behind".
    pub ahead: Option<usize>,
    pub behind: Option<usize>,
    pub uncommitted: Option<usize>,
    pub conflicts: Option<usize>,
    /// `none`, `merge`, `rebase`, `cherry_pick` or `revert` — a git operation
    /// left unfinished by work done outside this dashboard.
    pub operation: Option<&'static str>,
    pub last_commit_unix: Option<i64>,
    /// RFC 3339, UTC.
    pub last_commit: Option<String>,
    pub github: GithubStatus,
}

/// CI / pull-request information, **read from the dashboard's cache only**.
///
/// `--json` never calls `gh` and never touches the network: a prompt that
/// shells out every few seconds would otherwise hammer the GitHub API and
/// stall on every timeout. When there is nothing cached, `cached` is `false`
/// and `note` says why, rather than reporting a plausible-looking zero.
#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
pub struct GithubStatus {
    pub cached: bool,
    /// Present only when `cached` is `false`, explaining the absence.
    pub note: Option<String>,
    /// `true` when the cached values are older than the dashboard's own
    /// refresh period, i.e. the dashboard itself would re-fetch them.
    pub stale: Option<bool>,
    pub age_seconds: Option<i64>,
    pub ci_state: Option<&'static str>,
    pub ci_status: Option<String>,
    pub ci_branch: Option<String>,
    pub ci_fetched_at: Option<String>,
    pub ci_fetched_at_unix: Option<i64>,
    pub last_run_url: Option<String>,
    pub pr_state: Option<&'static str>,
    pub open_prs: Option<usize>,
    pub pr_fetched_at: Option<String>,
    pub pr_fetched_at_unix: Option<i64>,
}

impl GithubStatus {
    fn absent(note: &str) -> Self {
        Self {
            cached: false,
            note: Some(note.to_string()),
            stale: None,
            age_seconds: None,
            ci_state: None,
            ci_status: None,
            ci_branch: None,
            ci_fetched_at: None,
            ci_fetched_at_unix: None,
            last_run_url: None,
            pr_state: None,
            open_prs: None,
            pr_fetched_at: None,
            pr_fetched_at_unix: None,
        }
    }
}

/// Local git state for one repository, as read by [`local_status`].
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LocalStatus {
    pub branch: String,
    pub has_upstream: bool,
    pub ahead: usize,
    pub behind: usize,
    pub uncommitted: usize,
    pub conflicts: usize,
    pub operation: GitOpState,
    pub last_commit_unix: Option<i64>,
}

fn operation_name(state: GitOpState) -> &'static str {
    match state {
        GitOpState::None => "none",
        GitOpState::Merge => "merge",
        GitOpState::Rebase => "rebase",
        GitOpState::CherryPick => "cherry_pick",
        GitOpState::Revert => "revert",
    }
}

fn github_state_name(state: GithubState) -> &'static str {
    match state {
        GithubState::Unknown => "not_fetched",
        GithubState::Ready => "ready",
        GithubState::NoRuns => "no_runs",
        GithubState::Unauthenticated => "unauthenticated",
        GithubState::Failed => "fetch_failed",
        GithubState::Unavailable => "gh_not_installed",
        GithubState::Unsupported => "unsupported",
    }
}

/// RFC 3339 in UTC, or `None` for the "unknown" sentinel the git layer uses.
fn rfc3339(ts: i64) -> Option<String> {
    use chrono::{TimeZone, Utc};
    if ts <= 0 {
        return None;
    }
    Utc.timestamp_opt(ts, 0)
        .single()
        .map(|dt| dt.to_rfc3339_opts(chrono::SecondsFormat::Secs, true))
}

/// Collapse a git error into one short line. Git writes multi-line,
/// repository-influenced text to stderr; a JSON field carrying an unbounded
/// blob of it is unusable in a status line and unpleasant in a log.
fn short_error(text: &str) -> String {
    const LIMIT: usize = 200;
    let joined = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let trimmed = if joined.is_empty() {
        "git reported an unspecified failure".to_string()
    } else {
        joined
    };
    match trimmed.char_indices().nth(LIMIT) {
        Some((cut, _)) => format!("{}…", &trimmed[..cut]),
        None => trimmed,
    }
}

/// Indices into the command batch issued by [`local_status`].
const CMD_BRANCH: usize = 0;
const CMD_STATUS: usize = 1;
const CMD_UPSTREAM: usize = 2;
const CMD_COUNTS: usize = 3;
const CMD_LAST_COMMIT: usize = 4;
const CMD_MERGE_HEAD: usize = 5;
const CMD_REBASE_HEAD: usize = 6;
const CMD_CHERRY_PICK_HEAD: usize = 7;
const CMD_REVERT_HEAD: usize = 8;
const CMD_REBASE_MERGE_DIR: usize = 9;
const CMD_REBASE_APPLY_DIR: usize = 10;

/// Read one repository's local state.
///
/// This deliberately does **not** call [`git::get_summary`]: that function
/// also calls `gh` for GitHub remotes, which is exactly the network access
/// `--json` promises not to do. It reuses the same execution and parsing
/// helpers (`run_git_batch`, `parse_sync_status`, `count_conflicts`) and the
/// same command list, so the numbers match what the dashboard shows — and,
/// like the dashboard, every invocation carries `git::GIT_TIMEOUT`.
fn local_status(path: &Path) -> Result<LocalStatus, String> {
    let ssh = git::parse_ssh_repo(path).is_some();
    if !ssh {
        if !path.exists() {
            return Err(format!("path does not exist: {}", path.display()));
        }
        if !git::is_git_repo(path) {
            return Err(format!("not a git repository: {}", path.display()));
        }
    }

    let cmds: [&[&str]; 11] = [
        &["branch", "--show-current"],
        &["status", "--porcelain=v1", "--untracked-files=all"],
        &["rev-parse", "--abbrev-ref", "@{u}"],
        &["rev-list", "--left-right", "--count", "HEAD...@{u}"],
        &["log", "-1", "--format=%ct"],
        &["rev-parse", "-q", "--verify", "MERGE_HEAD"],
        &["rev-parse", "-q", "--verify", "REBASE_HEAD"],
        &["rev-parse", "-q", "--verify", "CHERRY_PICK_HEAD"],
        &["rev-parse", "-q", "--verify", "REVERT_HEAD"],
        &["rev-parse", "--git-path", "rebase-merge"],
        &["rev-parse", "--git-path", "rebase-apply"],
    ];
    let results = git::run_git_batch(path, &cmds);

    // `REBASE_HEAD` alone is unreliable: git leaves the pseudo-ref behind
    // after a successful `rebase --continue`, but removes the state
    // directory. Same reasoning (and the same fallback for SSH repositories,
    // which have no local directory to look at) as `git::get_summary`.
    let rebase = if ssh {
        results
            .get(CMD_REBASE_HEAD)
            .is_some_and(|r: &Result<String, String>| r.is_ok())
    } else {
        [CMD_REBASE_MERGE_DIR, CMD_REBASE_APPLY_DIR]
            .iter()
            .any(|&i| match results.get(i).and_then(|r| r.as_ref().ok()) {
                Some(resolved) => {
                    let resolved = resolved.trim();
                    let dir = if Path::new(resolved).is_absolute() {
                        PathBuf::from(resolved)
                    } else {
                        path.join(resolved)
                    };
                    dir.is_dir()
                }
                None => false,
            })
    };

    local_status_from_batch(&results, rebase)
}

/// The pure half of [`local_status`]: turn the batch results into a
/// [`LocalStatus`]. Split out so the mapping is testable without a repository
/// on disk.
fn local_status_from_batch(
    results: &[Result<String, String>],
    rebase_in_progress: bool,
) -> Result<LocalStatus, String> {
    // `git status` is the one command that must succeed: if it failed, the
    // path is not a readable working tree and every other number would be
    // guesswork. Reporting the failure beats reporting zeros.
    let status = match results.get(CMD_STATUS) {
        Some(Ok(text)) => text.as_str(),
        Some(Err(e)) => return Err(short_error(e)),
        None => return Err("git produced no output".to_string()),
    };

    let out = |i: usize| -> &str {
        results
            .get(i)
            .and_then(|r| r.as_ref().ok())
            .map(String::as_str)
            .unwrap_or("")
    };
    let ok = |i: usize| -> bool { results.get(i).is_some_and(|r| r.is_ok()) };

    let has_upstream = !out(CMD_UPSTREAM).trim().is_empty();
    let sync = git::parse_sync_status(has_upstream, out(CMD_COUNTS));

    let operation = if ok(CMD_MERGE_HEAD) {
        GitOpState::Merge
    } else if rebase_in_progress {
        GitOpState::Rebase
    } else if ok(CMD_CHERRY_PICK_HEAD) {
        GitOpState::CherryPick
    } else if ok(CMD_REVERT_HEAD) {
        GitOpState::Revert
    } else {
        GitOpState::None
    };

    Ok(LocalStatus {
        branch: out(CMD_BRANCH).trim().to_string(),
        has_upstream: sync.has_upstream,
        ahead: sync.ahead,
        behind: sync.behind,
        uncommitted: status.lines().filter(|l| !l.trim().is_empty()).count(),
        conflicts: git::count_conflicts(status),
        operation,
        last_commit_unix: out(CMD_LAST_COMMIT).trim().parse::<i64>().ok(),
    })
}

/// Build the `github` object from whatever the dashboard last wrote to its
/// on-disk cache for this repository.
fn cached_github(path: &Path, now: i64) -> GithubStatus {
    let Some(row) = crate::app::load_home_cache(path) else {
        return GithubStatus::absent(
            "no cached dashboard data for this repository; --json never contacts the network",
        );
    };
    let Some(info) = row.github else {
        return GithubStatus::absent(
            "the cached dashboard data has no GitHub information (no GitHub remote, or never fetched)",
        );
    };
    github_status(&info, now)
}

/// The pure half of [`cached_github`]. `now` is a Unix timestamp.
fn github_status(info: &RemoteCiPrInfo, now: i64) -> GithubStatus {
    let newest = info.ci_fetched_at.max(info.pr_fetched_at);
    let age = (newest > 0).then(|| now - newest);
    // "Stale" means the dashboard itself would re-fetch: either the values
    // are older than its refresh period, or they were never fetched at all.
    let stale = match age {
        Some(age) => age >= git::github::REFRESH_SECS,
        None => true,
    };
    GithubStatus {
        cached: true,
        note: None,
        stale: Some(stale),
        age_seconds: age,
        ci_state: Some(github_state_name(info.ci_state)),
        ci_status: info.ci_status.clone(),
        ci_branch: info.ci_branch.clone(),
        ci_fetched_at: rfc3339(info.ci_fetched_at),
        ci_fetched_at_unix: (info.ci_fetched_at > 0).then_some(info.ci_fetched_at),
        last_run_url: info.last_run_url.clone(),
        pr_state: Some(github_state_name(info.pr_state)),
        open_prs: info.open_prs,
        pr_fetched_at: rfc3339(info.pr_fetched_at),
        pr_fetched_at_unix: (info.pr_fetched_at > 0).then_some(info.pr_fetched_at),
    }
}

/// Assemble one repository entry from its already-collected parts. Pure, so
/// the "failed repositories are visibly failed" contract can be tested
/// without a repository.
fn repository_status(
    repo: &Repository,
    registered: bool,
    local: Result<LocalStatus, String>,
    github: GithubStatus,
) -> RepositoryStatus {
    let base = RepositoryStatus {
        name: repo.name.clone(),
        path: repo.path.to_string_lossy().into_owned(),
        group: repo.group.clone().filter(|g| !g.trim().is_empty()),
        registered,
        ok: false,
        error: None,
        branch: None,
        has_upstream: None,
        ahead: None,
        behind: None,
        uncommitted: None,
        conflicts: None,
        operation: None,
        last_commit_unix: None,
        last_commit: None,
        github,
    };
    match local {
        Err(error) => RepositoryStatus {
            error: Some(error),
            ..base
        },
        Ok(local) => RepositoryStatus {
            ok: true,
            branch: Some(local.branch),
            has_upstream: Some(local.has_upstream),
            // Without an upstream there is nothing to be ahead of; `null`
            // says so, where `0` would read as "in sync".
            ahead: local.has_upstream.then_some(local.ahead),
            behind: local.has_upstream.then_some(local.behind),
            uncommitted: Some(local.uncommitted),
            conflicts: Some(local.conflicts),
            operation: Some(operation_name(local.operation)),
            last_commit_unix: local.last_commit_unix,
            last_commit: local.last_commit_unix.and_then(rfc3339),
            ..base
        },
    }
}

/// Analyse every target, at most [`MAX_SNAPSHOT_THREADS`] at a time, and
/// return the results in input order.
fn collect_statuses(targets: &[(Repository, bool)], now: i64) -> Vec<RepositoryStatus> {
    if targets.is_empty() {
        return Vec::new();
    }
    let next = AtomicUsize::new(0);
    let (tx, rx) = std::sync::mpsc::channel::<(usize, RepositoryStatus)>();
    std::thread::scope(|scope| {
        for _ in 0..targets.len().min(MAX_SNAPSHOT_THREADS) {
            let tx = tx.clone();
            let next = &next;
            scope.spawn(move || {
                loop {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    let Some((repo, registered)) = targets.get(i) else {
                        break;
                    };
                    let status = repository_status(
                        repo,
                        *registered,
                        local_status(&repo.path),
                        cached_github(&repo.path, now),
                    );
                    // The receiver outlives the scope, so a send can only
                    // fail if the channel is gone — nothing to recover.
                    let _ = tx.send((i, status));
                }
            });
        }
    });
    drop(tx);
    let mut collected: Vec<(usize, RepositoryStatus)> = rx.into_iter().collect();
    collected.sort_by_key(|(i, _)| *i);
    collected.into_iter().map(|(_, status)| status).collect()
}

/// Build the snapshot document. `path`, when given, limits it to that one
/// repository (which need not be registered).
pub fn build_snapshot(path: Option<PathBuf>) -> Result<Snapshot, String> {
    // Loaded even for the single-path form: `registered` is a claim about the
    // configuration, and reporting `false` from a config we could not read
    // would be a guess dressed up as a fact.
    let configured = config::load_repositories()?;

    let targets: Vec<(Repository, bool)> = match path {
        None => configured.into_iter().map(|repo| (repo, true)).collect(),
        Some(path) => {
            let path = resolve_repository_path(&path)?;
            match configured
                .into_iter()
                .find(|repo| same_repository(&repo.path, &path))
            {
                Some(repo) => vec![(repo, true)],
                None => vec![(
                    Repository {
                        name: git::get_repo_name(&path),
                        path,
                        group: None,
                    },
                    false,
                )],
            }
        }
    };

    let now = chrono::Utc::now().timestamp();
    let repositories = collect_statuses(&targets, now);
    Ok(Snapshot {
        schema: SCHEMA_ID,
        schema_version: SCHEMA_VERSION,
        tool_version: env!("CARGO_PKG_VERSION"),
        generated_at: rfc3339(now).unwrap_or_default(),
        generated_at_unix: now,
        repository_count: repositories.len(),
        repositories,
    })
}

/// `--json`: print the snapshot to stdout and return the process exit code.
/// No terminal is initialised at any point, so this works with stdout on a
/// pipe.
pub fn run_snapshot(path: Option<PathBuf>) -> ExitCode {
    let snapshot = match build_snapshot(path) {
        Ok(snapshot) => snapshot,
        Err(e) => {
            eprintln!("git-dashboard-tui: {e}");
            return ExitCode::FAILURE;
        }
    };
    let mut json = match serde_json::to_string_pretty(&snapshot) {
        Ok(json) => json,
        Err(e) => {
            eprintln!("git-dashboard-tui: cannot serialise the snapshot: {e}");
            return ExitCode::FAILURE;
        }
    };
    json.push('\n');

    // Written through `write_all` rather than `println!` so that a closed pipe
    // (`| head -1`) is a clean exit instead of a panic.
    let mut stdout = std::io::stdout().lock();
    match stdout
        .write_all(json.as_bytes())
        .and_then(|()| stdout.flush())
    {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) if e.kind() == std::io::ErrorKind::BrokenPipe => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("git-dashboard-tui: cannot write to stdout: {e}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<Invocation, String> {
        parse_args(args.iter().map(OsString::from))
    }

    fn dashboard(path: Option<&str>) -> Invocation {
        Invocation::Dashboard {
            path: path.map(PathBuf::from),
        }
    }

    fn snapshot(path: Option<&str>) -> Invocation {
        Invocation::Snapshot {
            path: path.map(PathBuf::from),
        }
    }

    #[test]
    fn no_arguments_starts_the_dashboard() {
        assert_eq!(parse(&[]).unwrap(), dashboard(None));
    }

    #[test]
    fn a_positional_path_focuses_the_dashboard() {
        assert_eq!(parse(&["."]).unwrap(), dashboard(Some(".")));
        assert_eq!(parse(&["/srv/app"]).unwrap(), dashboard(Some("/srv/app")));
        // A path may contain characters that look like flag syntax as long as
        // it does not *start* with `-`.
        assert_eq!(parse(&["a--b"]).unwrap(), dashboard(Some("a--b")));
    }

    #[test]
    fn json_alone_and_with_a_path() {
        assert_eq!(parse(&["--json"]).unwrap(), snapshot(None));
        assert_eq!(parse(&["--json", "."]).unwrap(), snapshot(Some(".")));
        // Order must not matter.
        assert_eq!(parse(&[".", "--json"]).unwrap(), snapshot(Some(".")));
        // Repeating a flag is not an error.
        assert_eq!(parse(&["--json", "--json"]).unwrap(), snapshot(None));
    }

    #[test]
    fn help_and_version_still_parse() {
        for arg in ["-h", "--help"] {
            assert_eq!(parse(&[arg]).unwrap(), Invocation::Help);
        }
        for arg in ["-V", "--version"] {
            assert_eq!(parse(&[arg]).unwrap(), Invocation::Version);
        }
    }

    #[test]
    fn help_wins_over_everything_else_it_is_combined_with() {
        assert_eq!(parse(&["--json", "--help"]).unwrap(), Invocation::Help);
        assert_eq!(parse(&["--help", "--json"]).unwrap(), Invocation::Help);
        assert_eq!(parse(&["--help", "--version"]).unwrap(), Invocation::Help);
        assert_eq!(parse(&["--version", "--help"]).unwrap(), Invocation::Help);
        assert_eq!(parse(&["/srv/app", "--help"]).unwrap(), Invocation::Help);
        // Version still beats the dashboard and the snapshot.
        assert_eq!(parse(&["--json", "-V"]).unwrap(), Invocation::Version);
    }

    #[test]
    fn unknown_flags_are_rejected_and_name_themselves() {
        for arg in ["--jsonn", "-x", "--", "-"] {
            // `--` is the exception: it is the operand terminator, not a flag.
            if arg == "--" {
                assert_eq!(parse(&[arg]).unwrap(), dashboard(None));
                continue;
            }
            let err = parse(&[arg]).unwrap_err();
            assert!(err.contains(arg), "error should name the argument: {err}");
        }
        // A typo is not silently answered with the help text.
        assert!(parse(&["--jsonn", "--help"]).is_err());
    }

    #[test]
    fn the_operand_terminator_allows_a_path_that_looks_like_a_flag() {
        assert_eq!(parse(&["--", "--json"]).unwrap(), dashboard(Some("--json")));
        assert_eq!(
            parse(&["--json", "--", "-weird-dir"]).unwrap(),
            snapshot(Some("-weird-dir"))
        );
    }

    #[test]
    fn two_paths_are_rejected_and_an_empty_path_is_not_a_path() {
        let err = parse(&["/a", "/b"]).unwrap_err();
        assert!(err.contains("/a") && err.contains("/b"), "{err}");
        assert!(parse(&[""]).unwrap_err().contains("empty"));
    }

    // -- JSON shape ---------------------------------------------------------

    fn repo() -> Repository {
        Repository {
            name: "app".into(),
            path: PathBuf::from("/srv/app"),
            group: Some("work".into()),
        }
    }

    fn value(status: &RepositoryStatus) -> serde_json::Value {
        serde_json::to_value(status).unwrap()
    }

    #[test]
    fn a_failed_repository_is_marked_failed_instead_of_reading_as_healthy() {
        let status = repository_status(
            &repo(),
            true,
            Err("path does not exist: /srv/app".into()),
            GithubStatus::absent("nothing cached"),
        );
        let v = value(&status);
        assert_eq!(v["ok"], serde_json::json!(false));
        assert_eq!(
            v["error"],
            serde_json::json!("path does not exist: /srv/app")
        );
        // The dangerous failure mode is a vanished repository that looks
        // clean, so every measured field must be null — not zero, not "".
        for field in [
            "branch",
            "has_upstream",
            "ahead",
            "behind",
            "uncommitted",
            "conflicts",
            "operation",
            "last_commit",
            "last_commit_unix",
        ] {
            assert!(v[field].is_null(), "{field} should be null: {v}");
        }
        // Identity is still reported, so a consumer can say *which* repo broke.
        assert_eq!(v["name"], serde_json::json!("app"));
        assert_eq!(v["path"], serde_json::json!("/srv/app"));
        assert_eq!(v["registered"], serde_json::json!(true));
    }

    #[test]
    fn a_repository_without_an_upstream_is_distinguishable_from_a_synced_one() {
        let synced = value(&repository_status(
            &repo(),
            true,
            Ok(LocalStatus {
                branch: "main".into(),
                has_upstream: true,
                ..LocalStatus::default()
            }),
            GithubStatus::absent("nothing cached"),
        ));
        let detached = value(&repository_status(
            &repo(),
            true,
            Ok(LocalStatus {
                branch: "wip".into(),
                has_upstream: false,
                ..LocalStatus::default()
            }),
            GithubStatus::absent("nothing cached"),
        ));

        assert_eq!(synced["ok"], serde_json::json!(true));
        assert_eq!(synced["has_upstream"], serde_json::json!(true));
        assert_eq!(synced["ahead"], serde_json::json!(0));
        assert_eq!(synced["behind"], serde_json::json!(0));

        assert_eq!(detached["ok"], serde_json::json!(true));
        assert_eq!(detached["has_upstream"], serde_json::json!(false));
        assert!(detached["ahead"].is_null(), "{detached}");
        assert!(detached["behind"].is_null(), "{detached}");
        assert_ne!(synced["ahead"], detached["ahead"]);
    }

    #[test]
    fn an_ok_repository_reports_counts_and_the_in_progress_operation() {
        let v = value(&repository_status(
            &repo(),
            false,
            Ok(LocalStatus {
                branch: "main".into(),
                has_upstream: true,
                ahead: 2,
                behind: 3,
                uncommitted: 4,
                conflicts: 1,
                operation: GitOpState::Rebase,
                last_commit_unix: Some(1_700_000_000),
            }),
            GithubStatus::absent("nothing cached"),
        ));
        assert_eq!(v["ahead"], serde_json::json!(2));
        assert_eq!(v["behind"], serde_json::json!(3));
        assert_eq!(v["uncommitted"], serde_json::json!(4));
        assert_eq!(v["conflicts"], serde_json::json!(1));
        assert_eq!(v["operation"], serde_json::json!("rebase"));
        assert_eq!(v["registered"], serde_json::json!(false));
        assert_eq!(v["last_commit_unix"], serde_json::json!(1_700_000_000));
        assert_eq!(v["last_commit"], serde_json::json!("2023-11-14T22:13:20Z"));
        assert_eq!(v["group"], serde_json::json!("work"));
    }

    #[test]
    fn absent_ci_data_is_explicit_rather_than_zero() {
        let v = serde_json::to_value(GithubStatus::absent(
            "no cached dashboard data for this repository",
        ))
        .unwrap();
        assert_eq!(v["cached"], serde_json::json!(false));
        assert!(v["note"].as_str().unwrap().contains("no cached"));
        // The whole point: a consumer must not be able to read "0 open PRs"
        // or "CI green" out of data we never had.
        for field in ["open_prs", "ci_status", "ci_state", "pr_state", "stale"] {
            assert!(v[field].is_null(), "{field} should be null: {v}");
        }
    }

    #[test]
    fn cached_ci_data_reports_its_age_and_staleness() {
        let info = RemoteCiPrInfo {
            ci_state: GithubState::Ready,
            pr_state: GithubState::Ready,
            ci_status: Some("failure".into()),
            ci_branch: Some("main".into()),
            ci_fetched_at: 1_700_000_000,
            pr_fetched_at: 1_700_000_000,
            checked_at: 1_700_000_000,
            open_prs: Some(3),
            last_run_url: Some("https://github.com/a/b/actions/runs/1".into()),
        };

        let fresh = serde_json::to_value(github_status(&info, 1_700_000_060)).unwrap();
        assert_eq!(fresh["cached"], serde_json::json!(true));
        assert_eq!(fresh["stale"], serde_json::json!(false));
        assert_eq!(fresh["age_seconds"], serde_json::json!(60));
        assert_eq!(fresh["open_prs"], serde_json::json!(3));
        assert_eq!(fresh["ci_status"], serde_json::json!("failure"));
        assert_eq!(fresh["ci_state"], serde_json::json!("ready"));
        assert_eq!(
            fresh["ci_fetched_at"],
            serde_json::json!("2023-11-14T22:13:20Z")
        );
        assert!(fresh["note"].is_null());

        let old = serde_json::to_value(github_status(&info, 1_700_099_999)).unwrap();
        assert_eq!(old["stale"], serde_json::json!(true));
        assert_eq!(old["open_prs"], serde_json::json!(3));

        // Never fetched: cached entry, but nothing in it may read as a result.
        let never = serde_json::to_value(github_status(
            &RemoteCiPrInfo {
                ci_state: GithubState::Unauthenticated,
                pr_state: GithubState::Unauthenticated,
                ..RemoteCiPrInfo::default()
            },
            1_700_000_000,
        ))
        .unwrap();
        assert_eq!(never["stale"], serde_json::json!(true));
        assert!(never["age_seconds"].is_null());
        assert!(never["open_prs"].is_null());
        assert!(never["ci_fetched_at"].is_null());
        assert_eq!(never["ci_state"], serde_json::json!("unauthenticated"));
    }

    #[test]
    fn the_document_is_an_extensible_object_not_a_bare_array() {
        let snapshot = Snapshot {
            schema: SCHEMA_ID,
            schema_version: SCHEMA_VERSION,
            tool_version: "9.9.9",
            generated_at: "2023-11-14T22:13:20Z".into(),
            generated_at_unix: 1_700_000_000,
            repository_count: 0,
            repositories: Vec::new(),
        };
        let v = serde_json::to_value(&snapshot).unwrap();
        assert!(v.is_object());
        assert_eq!(v["schema"], serde_json::json!(SCHEMA_ID));
        assert_eq!(v["schema_version"], serde_json::json!(1));
        assert!(v["repositories"].is_array());
    }

    // -- batch mapping ------------------------------------------------------

    fn batch(
        status: Result<&str, &str>,
        upstream: &str,
        counts: &str,
    ) -> Vec<Result<String, String>> {
        let ok = |s: &str| Ok(s.to_string());
        vec![
            ok("main"),
            status.map(str::to_string).map_err(str::to_string),
            ok(upstream),
            ok(counts),
            ok("1700000000"),
            Err("fatal: Needed a single revision".into()),
            Err("fatal: Needed a single revision".into()),
            Err("fatal: Needed a single revision".into()),
            Err("fatal: Needed a single revision".into()),
            ok(".git/rebase-merge"),
            ok(".git/rebase-apply"),
        ]
    }

    #[test]
    fn batch_results_map_to_counts_conflicts_and_sync() {
        let status = " M src/main.rs\n?? new.txt\nUU src/conflict.rs\n";
        let local =
            local_status_from_batch(&batch(Ok(status), "origin/main", "2\t3"), false).unwrap();
        assert_eq!(
            local,
            LocalStatus {
                branch: "main".into(),
                has_upstream: true,
                ahead: 2,
                behind: 3,
                uncommitted: 3,
                conflicts: 1,
                operation: GitOpState::None,
                last_commit_unix: Some(1_700_000_000),
            }
        );

        // No upstream: `parse_sync_status` zeroes the counters, and
        // `repository_status` is what turns that into `null`.
        let no_upstream = local_status_from_batch(&batch(Ok(""), "", ""), false).unwrap();
        assert!(!no_upstream.has_upstream);
        assert_eq!(no_upstream.uncommitted, 0);

        // An in-progress rebase is reported even though REBASE_HEAD is absent
        // — the state directory is the reliable signal.
        let rebasing = local_status_from_batch(&batch(Ok(""), "", ""), true).unwrap();
        assert_eq!(rebasing.operation, GitOpState::Rebase);
    }

    #[test]
    fn a_failing_status_command_becomes_an_error_not_zeros() {
        let err = local_status_from_batch(
            &batch(
                Err("fatal: not a git repository\n  (or any parent)"),
                "",
                "",
            ),
            false,
        )
        .unwrap_err();
        assert!(err.contains("not a git repository"), "{err}");
        // Multi-line git stderr is collapsed into one line.
        assert!(!err.contains('\n'), "{err}");
        assert!(local_status_from_batch(&[], false).is_err());
    }

    #[test]
    fn long_git_errors_are_truncated() {
        let long = "x".repeat(500);
        let short = short_error(&long);
        assert!(short.chars().count() <= 201, "{}", short.chars().count());
        assert!(short.ends_with('…'));
        assert_eq!(short_error("   "), "git reported an unspecified failure");
    }

    // -- focused start-up ---------------------------------------------------

    #[test]
    fn focusing_a_path_shows_and_selects_it_without_registering_it() {
        let dir = std::env::temp_dir().join(format!("gdt-cli-focus-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();

        let mut app = App::new();
        let index = app.repos.len();
        focus_repository(&mut app, dir.clone());

        let added = app.repos.last().expect("the focused repository is shown");
        assert_eq!(added.path, dir);
        // Marked, so the row cannot be mistaken for something the user
        // registered and will see again tomorrow.
        assert!(
            added.name.contains("not registered") || added.name.contains("未登録"),
            "{}",
            added.name
        );
        assert_eq!(
            app.filtered_home().get(app.home_selected).copied(),
            Some(index),
            "the focused repository must be the selected row"
        );

        // The whole point of "ephemeral": a command-line argument must not
        // grow the user's configuration behind their back.
        let saved = config::load_repositories().unwrap_or_default();
        assert!(
            saved.iter().all(|repo| repo.path != dir),
            "focusing a path must not write it to config.json"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn focusing_an_already_registered_path_selects_it_instead_of_duplicating_it() {
        let dir = std::env::temp_dir().join(format!("gdt-cli-dup-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();

        let mut app = App::new();
        app.repos.push(Repository {
            name: "already-here".into(),
            path: dir.clone(),
            group: None,
        });
        let index = app.repos.len() - 1;
        let count = app.repos.len();

        // The same repository reached by a path that needs canonicalising.
        focus_repository(&mut app, dir.join("."));

        assert_eq!(app.repos.len(), count, "no duplicate row");
        assert_eq!(app.repos[index].name, "already-here", "name kept as-is");
        assert_eq!(
            app.filtered_home().get(app.home_selected).copied(),
            Some(index)
        );

        std::fs::remove_dir_all(&dir).ok();
    }
}
