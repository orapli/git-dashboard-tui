//! GitHub refreshes have their own cache period; local reloads never force API calls.
use super::{
    GithubState, RemoteCiPrInfo, parse_gh_prs, parse_gh_runs, parse_ssh_repo, quiet_command,
    run_git_cmd, run_with_timeout,
};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock},
    time::Duration,
};

pub const REFRESH_SECS: i64 = 300;
const RETRY_SECS: i64 = 60;

fn retry_after(info: &RemoteCiPrInfo) -> i64 {
    if matches!(info.ci_state, GithubState::Ready | GithubState::NoRuns)
        && info.pr_state == GithubState::Ready
    {
        REFRESH_SECS
    } else {
        RETRY_SECS
    }
}

pub fn get_status(path: &Path) -> Option<RemoteCiPrInfo> {
    get_status_with(path, chrono::Utc::now().timestamp(), |args| {
        let mut command = quiet_command("gh");
        command.args(args).current_dir(path);
        let output = run_with_timeout(command, Duration::from_secs(2)).map_err(|e| {
            if e.contains("No such file") || e.contains("not found") {
                GithubState::Unavailable
            } else {
                GithubState::Failed
            }
        })?;
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
            ..Default::default()
        });
    }
    let remote = run_git_cmd(path, &["remote", "-v"]).ok()?;
    if !remote.contains("github.com") {
        return None;
    }
    type Entry = Arc<Mutex<Option<RemoteCiPrInfo>>>;
    static CACHE: OnceLock<Mutex<HashMap<(PathBuf, String), Entry>>> = OnceLock::new();
    let cache = CACHE.get_or_init(Mutex::default);
    let key = (
        path.canonicalize().unwrap_or_else(|_| path.to_path_buf()),
        remote,
    );
    let entry = cache
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .entry(key)
        .or_insert_with(|| Arc::new(Mutex::new(None)))
        .clone();
    let mut stored = entry.lock().unwrap_or_else(|e| e.into_inner());
    Some(cached(&mut stored, now, run))
}

fn cached(
    stored: &mut Option<RemoteCiPrInfo>,
    now: i64,
    run: impl FnMut(&[&str]) -> Result<String, GithubState>,
) -> RemoteCiPrInfo {
    if let Some(info) = stored.as_ref()
        && now >= info.checked_at
        && now - info.checked_at < retry_after(info)
    {
        return info.clone();
    }
    let info = fetch(stored.clone().unwrap_or_default(), now, run);
    *stored = Some(info.clone());
    info
}

fn fetch(
    mut info: RemoteCiPrInfo,
    now: i64,
    mut run: impl FnMut(&[&str]) -> Result<String, GithubState>,
) -> RemoteCiPrInfo {
    info.checked_at = now;
    match run(&["pr", "list", "--json", "number", "--limit", "100"]) {
        Ok(json) => match parse_gh_prs(&json) {
            Some(count) => {
                info.open_prs = Some(count);
                info.pr_state = GithubState::Ready;
                info.pr_fetched_at = now;
            }
            None => info.pr_state = GithubState::Failed,
        },
        Err(state) => info.pr_state = state,
    }
    match run(&[
        "run",
        "list",
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
                    info.ci_branch = None;
                    info.ci_fetched_at = now;
                }
                Some(runs) => {
                    let (status, url) = parse_gh_runs(&json);
                    if status.is_some() {
                        info.ci_state = GithubState::Ready;
                        info.ci_status = status;
                        info.last_run_url = url;
                        info.ci_branch = runs[0]
                            .get("headBranch")
                            .and_then(|v| v.as_str())
                            .map(str::to_owned);
                        info.ci_fetched_at = now;
                    } else {
                        info.ci_state = GithubState::Failed;
                    }
                }
                None => info.ci_state = GithubState::Failed,
            }
        }
        Err(state) => info.ci_state = state,
    }
    info
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let old = get_status_with(&path, 100, |args| Ok(if args[0] == "pr" { "[{\"number\":1}]" }
            else { r#"[{"conclusion":"failure","url":"https://github.com/example/old/actions/runs/1","headBranch":"old"}]"# }.into())).unwrap();
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
        let new = get_status_with(&path, 162, |_| Ok("[]".into())).unwrap();
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
        cached(&mut stored, 100, run);
        cached(&mut stored, 399, |_| panic!("local reload called gh"));
        assert_eq!(calls.get(), 2);
        cached(&mut stored, 400, |_| {
            calls.set(calls.get() + 1);
            Err(GithubState::Failed)
        });
        assert_eq!(calls.get(), 4);
        cached(&mut stored, 459, |_| panic!("failure retry was too early"));
        cached(&mut stored, 460, |_| {
            calls.set(calls.get() + 1);
            Ok("[]".into())
        });
        assert_eq!(calls.get(), 6);
    }

    #[test]
    fn empty_runs_auth_errors_and_failures_are_distinct_and_keep_cache() {
        let empty = fetch(RemoteCiPrInfo::default(), 100, |_| Ok("[]".into()));
        assert_eq!(empty.ci_state, GithubState::NoRuns);
        assert_eq!(empty.open_prs, Some(0));
        let success = fetch(empty, 200, |args| {
            Ok(if args[0] == "pr" { "[]" }
            else { r#"[{"conclusion":"failure","url":"https://github.com/a/b/actions/runs/1","headBranch":"other"}]"# }.into())
        });
        assert_eq!(success.ci_branch.as_deref(), Some("other"));
        assert_eq!(retry_after(&success), REFRESH_SECS);
        for state in [
            GithubState::Unauthenticated,
            GithubState::Failed,
            GithubState::Unavailable,
        ] {
            let failed = fetch(success.clone(), 500, |_| Err(state));
            assert_eq!(failed.ci_state, state);
            assert_eq!(failed.ci_status.as_deref(), Some("failure"));
            assert_eq!(failed.ci_fetched_at, 200);
            assert_eq!(failed.checked_at, 500);
            assert_eq!(retry_after(&failed), RETRY_SECS);
        }
        let invalid = fetch(success, 600, |_| Ok("{}".into()));
        assert_eq!(invalid.ci_state, GithubState::Failed);
        assert_eq!(invalid.pr_state, GithubState::Failed);
    }
}
