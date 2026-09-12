//! GitHub refreshes have their own cache period; local reloads never force API calls.
use super::{
    BranchPr, GithubState, RemoteCiPrInfo, parse_gh_runs, parse_ssh_repo, quiet_command,
    run_git_cmd, run_with_timeout,
};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock},
    time::Duration,
};

pub const REFRESH_SECS: i64 = 300;
/// A plain failure — a non-zero exit, an unparseable payload — is cheap to
/// re-attempt and is often a transient API blip, so it is retried soon.
const RETRY_SECS: i64 = 60;
/// A timeout already spent the whole [`GH_TIMEOUT`] budget on a link that has
/// just proven slow, and Home refreshes run several repositories in parallel,
/// so re-attempting every minute would keep a slow network permanently busy
/// failing. It still retries well inside the refresh period.
const TIMEOUT_RETRY_SECS: i64 = 180;
/// `gh` has to start, complete a TLS handshake and run a query against
/// api.github.com. The previous 2 s was below the round-trip time of a
/// healthy but distant (or VPN'd, or token-refreshing) setup, which showed as
/// a permanent "fetch failed" retried every minute. At 10 s the three
/// sequential calls a refresh makes add up to 30 s — the same ceiling
/// `GIT_TIMEOUT` already allows a single local git command.
const GH_TIMEOUT: Duration = Duration::from_secs(10);
/// Fields pulled from the pull-request list. `gh` is being asked for the list
/// anyway; every field here answers a question the dashboard shows, and
/// nothing else is requested.
const PR_FIELDS: &str = "number,title,headRefName,isDraft,reviewDecision,url";
/// `@me` is resolved by GitHub's search backend, so "waiting on my review"
/// needs no separate call to find out who the user is (and cannot go stale
/// against a re-authenticated `gh`).
const REVIEW_SEARCH: &str = "review-requested:@me";
/// Enough to answer "is someone waiting on me, and roughly how many"; larger
/// counts are rendered as "N+" rather than paged.
const REVIEW_LIMIT: &str = "20";
/// Pull-request titles are written by strangers and rendered into a terminal.
const MAX_TITLE_CHARS: usize = 80;

/// One repository's cached answer, shared so that two refresh threads
/// looking at the same repository serialise on it instead of both calling
/// `gh`.
type Entry = Arc<Mutex<Option<RemoteCiPrInfo>>>;
/// What makes a cached answer *this* repository's: its path, the remotes it
/// points at, and the branch the branch-scoped queries were asked about.
type CacheKey = (PathBuf, String, String);

/// How long this repository waits before `gh` is asked again.
///
/// `review_state` is deliberately not consulted: the review-request search is
/// the optional part of a refresh, so losing it must never drag the whole
/// repository into a one-minute retry loop.
fn retry_after(info: &RemoteCiPrInfo) -> i64 {
    state_retry(info.ci_state).min(state_retry(info.pr_state))
}

fn state_retry(state: GithubState) -> i64 {
    match state {
        // Settled answers — and the failures only the user can clear, by
        // running `gh auth login` or installing `gh`. Asking again in a
        // minute cannot change any of these; it only spawns processes for
        // every repository, every minute, forever.
        GithubState::Ready
        | GithubState::NoRuns
        | GithubState::Detached
        | GithubState::Unauthenticated
        | GithubState::Unavailable
        | GithubState::Unsupported => REFRESH_SECS,
        GithubState::TimedOut => TIMEOUT_RETRY_SECS,
        GithubState::Failed | GithubState::Unknown => RETRY_SECS,
    }
}

/// Which state a failed `gh` invocation should be reported as.
///
/// The timeout message is the one [`run_with_timeout`] produces when it kills
/// the child; a spawn failure carries the OS error text instead.
fn classify_gh_error(message: &str) -> GithubState {
    if message.contains("No such file") || message.contains("not found") {
        GithubState::Unavailable
    } else if message.contains("タイムアウト") {
        GithubState::TimedOut
    } else {
        GithubState::Failed
    }
}

pub fn get_status(path: &Path) -> Option<RemoteCiPrInfo> {
    get_status_with(path, chrono::Utc::now().timestamp(), |args| {
        let mut command = quiet_command("gh");
        command.args(args).current_dir(path);
        let output = run_with_timeout(command, GH_TIMEOUT).map_err(|e| classify_gh_error(&e))?;
        if !output.status.success() {
            let error = String::from_utf8_lossy(&output.stderr).to_lowercase();
            return Err(
                if error.contains("auth login")
                    || error.contains("authentication")
                    || error.contains("401")
                    || output.status.code() == Some(4)
                {
                    GithubState::Unauthenticated
                } else {
                    GithubState::Failed
                },
            );
        }
        String::from_utf8(output.stdout).map_err(|_| GithubState::Failed)
    })
}

fn get_status_with(
    path: &Path,
    now: i64,
    run: impl FnMut(&[&str]) -> Result<String, GithubState>,
) -> Option<RemoteCiPrInfo> {
    if parse_ssh_repo(path).is_some() {
        return Some(RemoteCiPrInfo {
            ci_state: GithubState::Unsupported,
            pr_state: GithubState::Unsupported,
            review_state: GithubState::Unsupported,
            ..Default::default()
        });
    }
    let remote = run_git_cmd(path, &["remote", "-v"]).ok()?;
    if !remote.contains("github.com") {
        return None;
    }
    // The CI run and the pull request this refresh reports on are both
    // branch-scoped, so the branch is part of the cache identity: checking
    // out another branch must not keep showing the previous branch's answers
    // for the rest of the refresh period. This is a local git call, not an
    // API call — `get_summary` knows the branch already, but threading it
    // through would change a signature another caller owns.
    //
    // A detached HEAD prints nothing, and an unreadable repository errors;
    // both mean "no branch to scope to".
    let branch = run_git_cmd(path, &["branch", "--show-current"])
        .map(|b| b.trim().to_string())
        .unwrap_or_default();
    static CACHE: OnceLock<Mutex<HashMap<CacheKey, Entry>>> = OnceLock::new();
    let cache = CACHE.get_or_init(Mutex::default);
    let key = (
        path.canonicalize().unwrap_or_else(|_| path.to_path_buf()),
        remote,
        branch.clone(),
    );
    let entry = cache
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .entry(key)
        .or_insert_with(|| Arc::new(Mutex::new(None)))
        .clone();
    let mut stored = entry.lock().unwrap_or_else(|e| e.into_inner());
    let branch = Some(branch.as_str()).filter(|b| !b.is_empty());
    Some(cached(&mut stored, now, branch, run))
}

fn cached(
    stored: &mut Option<RemoteCiPrInfo>,
    now: i64,
    branch: Option<&str>,
    run: impl FnMut(&[&str]) -> Result<String, GithubState>,
) -> RemoteCiPrInfo {
    if let Some(info) = stored.as_ref()
        && now >= info.checked_at
        && now - info.checked_at < retry_after(info)
    {
        return info.clone();
    }
    let info = fetch(stored.clone().unwrap_or_default(), now, branch, run);
    *stored = Some(info.clone());
    info
}

/// One refresh of a repository. Up to three `gh` calls, in decreasing order
/// of importance:
///
/// 1. the open pull requests (count, and the current branch's own PR),
/// 2. the latest workflow run **on the current branch** — skipped entirely on
///    a detached HEAD, where there is no branch to ask about,
/// 3. the pull requests in this repository waiting on the current user's
///    review — optional, and its failure changes nothing but its own state.
///
/// A first call that fails because `gh` is missing or unauthenticated ends
/// the refresh: those are properties of the environment, not of the query, so
/// the remaining calls would only reproduce the same failure more slowly.
fn fetch(
    mut info: RemoteCiPrInfo,
    now: i64,
    branch: Option<&str>,
    mut run: impl FnMut(&[&str]) -> Result<String, GithubState>,
) -> RemoteCiPrInfo {
    info.checked_at = now;
    match run(&["pr", "list", "--json", PR_FIELDS, "--limit", "100"]) {
        Ok(json) => match parse_pr_list(&json, branch) {
            Some((count, branch_pr)) => {
                info.open_prs = Some(count);
                info.branch_pr = branch_pr;
                info.pr_state = GithubState::Ready;
                info.pr_fetched_at = now;
            }
            None => info.pr_state = GithubState::Failed,
        },
        Err(state) => info.pr_state = state,
    }
    if matches!(
        info.pr_state,
        GithubState::Unavailable | GithubState::Unauthenticated
    ) {
        info.ci_state = info.pr_state;
        info.review_state = info.pr_state;
        return info;
    }
    match branch {
        // Not a fall-back to the repository-wide latest run: that run belongs
        // to whoever pushed last, and reporting it as this repository's CI is
        // exactly the misleading signal this scoping removes.
        None => {
            info.ci_state = GithubState::Detached;
            info.ci_status = None;
            info.last_run_url = None;
            info.ci_branch = None;
            info.ci_fetched_at = now;
        }
        Some(branch) => match run(&[
            "run",
            "list",
            "--branch",
            branch,
            "--json",
            "conclusion,status,url,headBranch",
            "--limit",
            "1",
        ]) {
            Ok(json) => {
                let value = serde_json::from_str::<serde_json::Value>(&json).ok();
                match value.as_ref().and_then(|v| v.as_array()) {
                    Some(runs) if runs.is_empty() => {
                        info.ci_state = GithubState::NoRuns;
                        info.ci_status = None;
                        info.last_run_url = None;
                        // "No runs" is an answer about this branch, so the
                        // branch is still what the state describes.
                        info.ci_branch = Some(branch.to_string());
                        info.ci_fetched_at = now;
                    }
                    Some(runs) => {
                        let (status, url) = parse_gh_runs(&json);
                        if status.is_some() {
                            info.ci_state = GithubState::Ready;
                            info.ci_status = status;
                            info.last_run_url = url;
                            info.ci_branch = Some(
                                runs[0]
                                    .get("headBranch")
                                    .and_then(|v| v.as_str())
                                    .filter(|b| !b.is_empty())
                                    .unwrap_or(branch)
                                    .to_string(),
                            );
                            info.ci_fetched_at = now;
                        } else {
                            info.ci_state = GithubState::Failed;
                        }
                    }
                    None => info.ci_state = GithubState::Failed,
                }
            }
            Err(state) => info.ci_state = state,
        },
    }
    // Optional, and last: everything above is already stored, so a failure
    // here costs this one signal and nothing else. It is not consulted by
    // `retry_after` either.
    match run(&[
        "pr",
        "list",
        "--search",
        REVIEW_SEARCH,
        "--json",
        "number",
        "--limit",
        REVIEW_LIMIT,
    ]) {
        Ok(json) => match serde_json::from_str::<serde_json::Value>(&json)
            .ok()
            .as_ref()
            .and_then(|v| v.as_array())
            .map(Vec::len)
        {
            Some(count) => {
                info.review_requests = Some(count);
                info.review_state = GithubState::Ready;
            }
            None => info.review_state = GithubState::Failed,
        },
        Err(state) => info.review_state = state,
    }
    info
}

/// The open-pull-request count, and the one opened from `branch` if it is in
/// the reply. `None` means the payload was not the expected array, which the
/// caller reports as a failed fetch.
///
/// The count keeps its historical meaning — the number of entries returned,
/// capped by the `--limit 100` the caller passes and rendered as "100+" — so
/// a branch whose pull request sits beyond that limit is counted but not
/// identified.
fn parse_pr_list(json: &str, branch: Option<&str>) -> Option<(usize, Option<Box<BranchPr>>)> {
    let value: serde_json::Value = serde_json::from_str(json).ok()?;
    let prs = value.as_array()?;
    // Every string here reaches a terminal, and a pull request title (or a
    // decision string from a newer GitHub) is not this program's text.
    let field = |pr: &serde_json::Value, name: &str| -> Option<String> {
        pr.get(name)
            .and_then(|v| v.as_str())
            .map(|s| s.chars().filter(|c| !c.is_control()).collect::<String>())
            .filter(|s| !s.is_empty())
    };
    let branch_pr = branch
        .and_then(|b| {
            prs.iter()
                .find(|pr| pr.get("headRefName").and_then(|v| v.as_str()) == Some(b))
        })
        .map(|pr| {
            Box::new(BranchPr {
                number: pr
                    .get("number")
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or_default() as usize,
                title: field(pr, "title")
                    .unwrap_or_default()
                    .chars()
                    .take(MAX_TITLE_CHARS)
                    .collect(),
                url: field(pr, "url").unwrap_or_default(),
                is_draft: pr
                    .get("isDraft")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(false),
                review_decision: field(pr, "reviewDecision"),
            })
        });
    Some((prs.len(), branch_pr))
}

#[cfg(test)]
mod tests {
    use super::*;

    const PRS: &str = r#"[
        {"number":7,"title":"Other work","headRefName":"other","isDraft":false,
         "reviewDecision":"APPROVED","url":"https://github.com/a/b/pull/7"},
        {"number":42,"title":"Scope CI to the branch","headRefName":"feature","isDraft":true,
         "reviewDecision":"","url":"https://github.com/a/b/pull/42"}
    ]"#;
    const RUN_ON_FEATURE: &str = r#"[{"conclusion":"failure","status":"completed",
        "url":"https://github.com/a/b/actions/runs/1","headBranch":"feature"}]"#;

    /// Which of the three queries a set of arguments is, so a fake `gh` can
    /// answer them apart. The review search is a `pr list` too, told apart by
    /// its `--search`.
    fn query(args: &[&str]) -> &'static str {
        if args.first() == Some(&"run") {
            "runs"
        } else if args.contains(&"--search") {
            "review"
        } else {
            "prs"
        }
    }

    fn fake<'a>(
        prs: &'a str,
        runs: &'a str,
        review: &'a str,
    ) -> impl FnMut(&[&str]) -> Result<String, GithubState> + 'a {
        move |args: &[&str]| {
            Ok(match query(args) {
                "runs" => runs,
                "review" => review,
                _ => prs,
            }
            .to_string())
        }
    }

    #[test]
    fn changing_remote_does_not_reuse_another_repositorys_values() {
        let path = std::env::temp_dir().join(format!("gdt-github-origin-{}", std::process::id()));
        std::fs::create_dir_all(&path).unwrap();
        run_git_cmd(&path, &["init", "-q"]).unwrap();
        run_git_cmd(
            &path,
            &[
                "remote",
                "add",
                "origin",
                "https://github.com/example/old.git",
            ],
        )
        .unwrap();
        let old = get_status_with(&path, 100, fake("[{\"number\":1}]", "[]", "[]")).unwrap();
        assert_eq!(old.open_prs, Some(1));
        get_status_with(&path, 101, |_| {
            panic!("unchanged remote should reuse cache")
        });
        run_git_cmd(
            &path,
            &[
                "remote",
                "set-url",
                "origin",
                "https://github.com/example/new.git",
            ],
        )
        .unwrap();
        let failed = get_status_with(&path, 102, |_| Err(GithubState::Failed)).unwrap();
        assert_eq!(failed.ci_state, GithubState::Failed);
        assert_eq!(failed.ci_status, None);
        assert_eq!(failed.open_prs, None);
        assert_eq!(failed.last_run_url, None);
        let new = get_status_with(&path, 162, fake("[]", "[]", "[]")).unwrap();
        assert_eq!(new.open_prs, Some(0));
        assert_eq!(new.ci_state, GithubState::NoRuns);
        std::fs::remove_dir_all(path).unwrap();
    }

    #[test]
    fn local_reloads_reuse_cache_and_failures_wait_before_retry() {
        let mut stored = None;
        let calls = std::cell::Cell::new(0);
        let run = |_: &[&str]| {
            calls.set(calls.get() + 1);
            Ok("[]".into())
        };
        cached(&mut stored, 100, Some("main"), run);
        cached(&mut stored, 399, Some("main"), |_| {
            panic!("local reload called gh")
        });
        // PRs, the branch's runs, and the review search.
        assert_eq!(calls.get(), 3);
        cached(&mut stored, 400, Some("main"), |_| {
            calls.set(calls.get() + 1);
            Err(GithubState::Failed)
        });
        assert_eq!(calls.get(), 6);
        cached(&mut stored, 459, Some("main"), |_| {
            panic!("failure retry was too early")
        });
        cached(&mut stored, 460, Some("main"), |_| {
            calls.set(calls.get() + 1);
            Ok("[]".into())
        });
        assert_eq!(calls.get(), 9);
    }

    #[test]
    fn empty_runs_auth_errors_and_failures_are_distinct_and_keep_cache() {
        let empty = fetch(
            RemoteCiPrInfo::default(),
            100,
            Some("main"),
            fake("[]", "[]", "[]"),
        );
        assert_eq!(empty.ci_state, GithubState::NoRuns);
        assert_eq!(empty.open_prs, Some(0));
        assert_eq!(empty.ci_branch.as_deref(), Some("main"));
        let success = fetch(
            empty,
            200,
            Some("feature"),
            fake(PRS, RUN_ON_FEATURE, "[{\"number\":9}]"),
        );
        assert_eq!(success.ci_branch.as_deref(), Some("feature"));
        assert_eq!(retry_after(&success), REFRESH_SECS);
        for state in [
            GithubState::Failed,
            GithubState::Unauthenticated,
            GithubState::Unavailable,
            GithubState::TimedOut,
        ] {
            let failed = fetch(success.clone(), 500, Some("feature"), |_| Err(state));
            assert_eq!(failed.ci_state, state);
            assert_eq!(failed.ci_status.as_deref(), Some("failure"));
            assert_eq!(failed.ci_fetched_at, 200);
            assert_eq!(failed.checked_at, 500);
            // A user-fixable failure is not worth re-asking every minute; a
            // timeout backs off further than a plain failure does.
            assert_eq!(
                retry_after(&failed),
                match state {
                    GithubState::Failed => RETRY_SECS,
                    GithubState::TimedOut => TIMEOUT_RETRY_SECS,
                    _ => REFRESH_SECS,
                },
                "{state:?}"
            );
        }
        let invalid = fetch(success, 600, Some("feature"), fake("{}", "{}", "{}"));
        assert_eq!(invalid.ci_state, GithubState::Failed);
        assert_eq!(invalid.pr_state, GithubState::Failed);
        assert_eq!(invalid.review_state, GithubState::Failed);
    }

    /// ⑦: the run list must be asked about this branch. Before scoping, a
    /// colleague's failing push raised the warning on every row.
    #[test]
    fn ci_is_asked_for_the_current_branch_only() {
        let seen = std::cell::RefCell::new(Vec::new());
        let info = fetch(RemoteCiPrInfo::default(), 100, Some("feature"), |args| {
            seen.borrow_mut()
                .push(args.iter().map(|a| a.to_string()).collect::<Vec<_>>());
            Ok(match query(args) {
                "runs" => RUN_ON_FEATURE,
                _ => "[]",
            }
            .to_string())
        });
        let run_args = seen
            .borrow()
            .iter()
            .find(|a| a.first().map(String::as_str) == Some("run"))
            .cloned()
            .expect("the run list was not queried");
        let branch_flag = run_args.iter().position(|a| a == "--branch");
        assert_eq!(
            branch_flag
                .and_then(|i| run_args.get(i + 1))
                .map(String::as_str),
            Some("feature"),
            "{run_args:?}"
        );
        assert_eq!(info.ci_state, GithubState::Ready);
        assert_eq!(info.ci_branch.as_deref(), Some("feature"));
    }

    /// A detached HEAD has no branch to scope to, so it gets its own state
    /// rather than silently reporting the repository-wide latest run.
    #[test]
    fn a_detached_head_is_its_own_state_not_a_branchless_ci_answer() {
        let previous = fetch(
            RemoteCiPrInfo::default(),
            100,
            Some("feature"),
            fake(PRS, RUN_ON_FEATURE, "[]"),
        );
        assert_eq!(previous.ci_status.as_deref(), Some("failure"));
        let detached = fetch(previous, 200, None, |args| {
            assert_ne!(query(args), "runs", "a detached HEAD must not query runs");
            Ok("[]".to_string())
        });
        assert_eq!(detached.ci_state, GithubState::Detached);
        assert_eq!(detached.ci_status, None);
        assert_eq!(detached.ci_branch, None);
        assert_eq!(detached.last_run_url, None);
        assert_eq!(detached.ci_fetched_at, 200);
        // Settled, not broken: it waits the full refresh period.
        assert_eq!(retry_after(&detached), REFRESH_SECS);
        // The branch's pull request is branch-scoped too.
        assert_eq!(detached.branch_pr, None);
    }

    #[test]
    fn the_branchs_pull_request_is_picked_out_of_the_list_that_was_already_fetched() {
        let draft = fetch(
            RemoteCiPrInfo::default(),
            100,
            Some("feature"),
            fake(PRS, "[]", "[]"),
        );
        assert_eq!(draft.open_prs, Some(2));
        let pr = draft.branch_pr.clone().expect("the branch's PR");
        assert_eq!(pr.number, 42);
        assert_eq!(pr.title, "Scope CI to the branch");
        assert_eq!(pr.url, "https://github.com/a/b/pull/42");
        assert!(pr.is_draft);
        // An empty `reviewDecision` means "no decision", not a decision named "".
        assert_eq!(pr.review_decision, None);
        assert!(!pr.changes_requested() && !pr.approved());

        let approved = fetch(draft, 200, Some("other"), fake(PRS, "[]", "[]"))
            .branch_pr
            .expect("the branch's PR");
        assert_eq!(approved.number, 7);
        assert!(!approved.is_draft);
        assert!(approved.approved() && !approved.changes_requested());

        let changes = r#"[{"number":3,"title":"x","headRefName":"feature","isDraft":false,
            "reviewDecision":"CHANGES_REQUESTED","url":"u"}]"#;
        let (count, branch_pr) = parse_pr_list(changes, Some("feature")).unwrap();
        assert_eq!(count, 1);
        assert!(branch_pr.unwrap().changes_requested());
        // A branch with no pull request keeps the count and reports no PR.
        assert_eq!(parse_pr_list(PRS, Some("main")).unwrap(), (2, None));
        assert_eq!(parse_pr_list(PRS, None).unwrap(), (2, None));
        assert_eq!(parse_pr_list("nonsense", Some("feature")), None);
        // A title is terminal output: control characters are stripped.
        let hostile = r#"[{"number":1,"headRefName":"f","title":"a\u001b[31mb\nc"}]"#;
        let pr = parse_pr_list(hostile, Some("f")).unwrap().1.unwrap();
        assert_eq!(pr.title, "a[31mbc");
    }

    /// The review-request search is the optional call: when it fails the row
    /// loses that one signal, and every other part of the refresh stands.
    #[test]
    fn a_failed_review_request_search_degrades_only_itself() {
        let good = fetch(
            RemoteCiPrInfo::default(),
            100,
            Some("feature"),
            fake(PRS, RUN_ON_FEATURE, r#"[{"number":9},{"number":10}]"#),
        );
        assert_eq!(good.review_requests, Some(2));
        assert_eq!(good.review_state, GithubState::Ready);

        let degraded = fetch(good, 200, Some("feature"), |args| match query(args) {
            "runs" => Ok(RUN_ON_FEATURE.to_string()),
            "review" => Err(GithubState::TimedOut),
            _ => Ok(PRS.to_string()),
        });
        assert_eq!(degraded.review_state, GithubState::TimedOut);
        assert_eq!(degraded.pr_state, GithubState::Ready);
        assert_eq!(degraded.ci_state, GithubState::Ready);
        assert_eq!(degraded.branch_pr.as_ref().unwrap().number, 42);
        // The optional call must not shorten the whole repository's retry.
        assert_eq!(retry_after(&degraded), REFRESH_SECS);
    }

    /// `gh` missing or unauthenticated is a property of the machine, not of
    /// the query, so the refresh stops instead of reproducing it three times.
    #[test]
    fn an_environment_failure_stops_the_refresh_after_one_call() {
        for state in [GithubState::Unavailable, GithubState::Unauthenticated] {
            let calls = std::cell::Cell::new(0);
            let info = fetch(RemoteCiPrInfo::default(), 100, Some("main"), |_| {
                calls.set(calls.get() + 1);
                Err(state)
            });
            assert_eq!(calls.get(), 1, "{state:?}");
            assert_eq!(info.pr_state, state);
            assert_eq!(info.ci_state, state);
            assert_eq!(info.review_state, state);
        }
    }

    /// The on-disk cache predates every field added here; loading it must not
    /// fail, and must not resurrect signals it never held.
    #[test]
    fn a_cache_file_written_before_the_new_fields_still_loads() {
        let old = r#"{"ci_state":"Ready","pr_state":"Ready","ci_branch":"main",
            "ci_fetched_at":10,"pr_fetched_at":11,"checked_at":12,"open_prs":3,
            "ci_status":"success","last_run_url":"https://github.com/a/b/actions/runs/1"}"#;
        let info: RemoteCiPrInfo = serde_json::from_str(old).unwrap();
        assert_eq!(info.open_prs, Some(3));
        assert_eq!(info.ci_status.as_deref(), Some("success"));
        assert_eq!(info.branch_pr, None);
        assert_eq!(info.review_requests, None);
        assert_eq!(info.review_state, GithubState::Unknown);
        // And an empty object, the extreme case of the same thing.
        assert_eq!(
            serde_json::from_str::<RemoteCiPrInfo>("{}").unwrap(),
            RemoteCiPrInfo::default()
        );
        // Round-tripping the new shape keeps everything.
        let full = fetch(
            RemoteCiPrInfo::default(),
            100,
            Some("feature"),
            fake(PRS, RUN_ON_FEATURE, "[{\"number\":9}]"),
        );
        let json = serde_json::to_string(&full).unwrap();
        assert_eq!(serde_json::from_str::<RemoteCiPrInfo>(&json).unwrap(), full);
    }

    #[test]
    fn a_slow_gh_is_told_apart_from_a_missing_one_and_a_plain_failure() {
        assert_eq!(
            classify_gh_error(
                "コマンドの起動に失敗しました: No such file or directory (os error 2)"
            ),
            GithubState::Unavailable
        );
        assert_eq!(classify_gh_error("boom"), GithubState::Failed);
        assert_eq!(
            classify_gh_error(
                "コマンドが10秒でタイムアウトしました（リモートまたはファイルシステムが応答していません）"
            ),
            GithubState::TimedOut
        );
    }

    /// Pins the classifier to the message `run_with_timeout` actually emits,
    /// so a reworded timeout cannot silently downgrade it to `Failed`.
    #[test]
    #[cfg(unix)]
    fn a_real_timeout_classifies_as_timed_out() {
        let mut cmd = std::process::Command::new("sleep");
        cmd.arg("5");
        let error = run_with_timeout(cmd, Duration::from_millis(200)).unwrap_err();
        assert_eq!(classify_gh_error(&error), GithubState::TimedOut);
    }
}
