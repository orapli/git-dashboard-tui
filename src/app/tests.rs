use super::*;
use crate::config::{Member, Repository};
use crate::git::{CommitSummary, DiffRowKind, TimeSpan};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use std::collections::HashMap;
use std::path::PathBuf;

fn repo(name: &str, path: &str) -> Repository {
    Repository {
        name: name.to_string(),
        path: PathBuf::from(path),
        group: None,
    }
}

#[test]
fn filter_matches_name_or_path() {
    let mut r1 = repo("git-dashboard", "/work/git-dashboard");
    r1.group = Some("Tools".to_string());
    let mut r2 = repo("aero-grep", "/work/search/aero-grep");
    r2.group = Some("Search".to_string());
    let r3 = repo("misc-app", "/work/misc-app");
    let repos = vec![r1, r2, r3];

    assert_eq!(filter_repo_indices(&repos, "", None), vec![0, 1, 2]);
    assert_eq!(filter_repo_indices(&repos, "DASH", None), vec![0]);
    assert_eq!(filter_repo_indices(&repos, "search", None), vec![1]);
    assert!(filter_repo_indices(&repos, "zzz", None).is_empty());

    // Group filtering
    assert_eq!(filter_repo_indices(&repos, "", Some("Tools")), vec![0]);
    assert_eq!(filter_repo_indices(&repos, "", Some("Search")), vec![1]);
    assert_eq!(filter_repo_indices(&repos, "", Some("")), vec![2]); // Ungrouped
}

#[test]
fn move_index_clamps() {
    assert_eq!(move_index(0, 0, 1), 0);
    assert_eq!(move_index(0, 3, 1), 1);
    assert_eq!(move_index(2, 3, 1), 2);
    assert_eq!(move_index(0, 3, -1), 0);
    assert_eq!(move_index(1, 3, -1), 0);
}

#[test]
fn tab_cycle_is_circular() {
    let mut t = RepoTab::Status;
    for _ in 0..7 {
        t = t.next();
    }
    assert_eq!(t, RepoTab::Status);
    t = t.prev();
    assert_eq!(t, RepoTab::Worktrees);
}

#[test]
fn home_keys_move_and_open_settings() {
    let mut app = App::new();
    app.repos = vec![
        repo("alpha", "/tmp/alpha"),
        repo("beta", "/tmp/beta"),
        repo("gamma", "/tmp/gamma"),
    ];
    app.home_selected = 0;
    app.handle_key(KeyEvent::from(KeyCode::Char('j')));
    assert_eq!(app.home_selected, 1);
    app.handle_key(KeyEvent::from(KeyCode::Char('k')));
    assert_eq!(app.home_selected, 0);
    app.handle_key(KeyEvent::from(KeyCode::Char('s')));
    assert_eq!(app.screen, Screen::Settings);
    app.handle_key(KeyEvent::from(KeyCode::Esc));
    assert_eq!(app.screen, Screen::Home);
}

#[test]
fn slash_enters_filter_and_narrows_list() {
    let mut app = App::new();
    app.repos = vec![repo("alpha", "/tmp/alpha"), repo("beta", "/tmp/beta")];
    app.handle_key(KeyEvent::from(KeyCode::Char('/')));
    assert!(app.is_filtering());
    app.handle_key(KeyEvent::from(KeyCode::Char('b')));
    assert_eq!(app.filtered_home(), vec![1]);
    app.handle_key(KeyEvent::from(KeyCode::Esc));
    assert!(!app.is_filtering());
    assert_eq!(app.filtered_home(), vec![0, 1]);
}

#[test]
fn hunk_start_detects_change_blocks() {
    let lines = vec![
        DiffLine {
            kind: DiffRowKind::Context,
            text: " a".into(),
            old_no: Some(1),
            new_no: Some(1),
        },
        DiffLine {
            kind: DiffRowKind::Removed,
            text: "-b".into(),
            old_no: Some(2),
            new_no: None,
        },
        DiffLine {
            kind: DiffRowKind::Added,
            text: "+c".into(),
            old_no: None,
            new_no: Some(2),
        },
        DiffLine {
            kind: DiffRowKind::Context,
            text: " d".into(),
            old_no: Some(3),
            new_no: Some(3),
        },
        DiffLine {
            kind: DiffRowKind::Added,
            text: "+e".into(),
            old_no: None,
            new_no: Some(4),
        },
    ];
    assert!(!is_hunk_start(&lines, 0));
    assert!(is_hunk_start(&lines, 1));
    assert!(!is_hunk_start(&lines, 2));
    assert!(is_hunk_start(&lines, 4));
    let hunks = collect_hunks(&lines);
    assert_eq!(hunks.len(), 2);
    assert_eq!(hunks[0].start, 1);
    assert_eq!(hunks[0].end, 3);
    assert_eq!(hunks[1].start, 4);
    assert_eq!(hunks[1].end, 5);
    assert!(hunks[0].label.contains("L2"));
}

#[test]
fn next_hunk_wraps_and_syncs_scroll() {
    let mut app = App::new();
    app.screen = Screen::Diff;
    app.diff = Some(DiffView {
        title: "t".into(),
        target: "abc".into(),
        base: None,
        three_dot: false,
        files: vec![],
        file_idx: 0,
        lines: vec![
            DiffLine {
                kind: DiffRowKind::Context,
                text: " a".into(),
                old_no: Some(1),
                new_no: Some(1),
            },
            DiffLine {
                kind: DiffRowKind::Added,
                text: "+b".into(),
                old_no: None,
                new_no: Some(2),
            },
            DiffLine {
                kind: DiffRowKind::Context,
                text: " c".into(),
                old_no: Some(3),
                new_no: Some(3),
            },
            DiffLine {
                kind: DiffRowKind::Removed,
                text: "-d".into(),
                old_no: Some(4),
                new_no: None,
            },
        ],
        hunks: vec![],
        hunk_idx: 0,
        scroll: 0,
        loading: false,
        header: None,
        error: None,
        blame: None,
        blame_loading: false,
        pending_scroll_restore: None,
    });
    if let Some(d) = app.diff.as_mut() {
        apply_hunks(d);
    }
    assert_eq!(app.diff.as_ref().unwrap().hunks.len(), 2);
    assert_eq!(app.diff.as_ref().unwrap().hunk_idx, 0);
    assert_eq!(app.diff.as_ref().unwrap().scroll, 1);
    app.handle_key(KeyEvent::from(KeyCode::Char('n')));
    assert_eq!(app.diff.as_ref().unwrap().hunk_idx, 1);
    assert_eq!(app.diff.as_ref().unwrap().scroll, 3);
    app.handle_key(KeyEvent::from(KeyCode::Char('n')));
    assert_eq!(app.diff.as_ref().unwrap().hunk_idx, 0);
    app.handle_key(KeyEvent::from(KeyCode::Char('p')));
    assert_eq!(app.diff.as_ref().unwrap().hunk_idx, 1);
}

#[test]
fn scrolling_diff_updates_hunk_selection() {
    let mut d = DiffView {
        title: "t".into(),
        target: "abc".into(),
        base: None,
        three_dot: false,
        files: vec![],
        file_idx: 0,
        lines: vec![],
        hunks: vec![
            Hunk {
                start: 0,
                end: 2,
                label: "#1".into(),
            },
            Hunk {
                start: 5,
                end: 8,
                label: "#2".into(),
            },
        ],
        hunk_idx: 0,
        scroll: 0,
        loading: false,
        header: None,
        error: None,
        blame: None,
        blame_loading: false,
        pending_scroll_restore: None,
    };
    d.scroll = 6;
    sync_hunk_from_scroll(&mut d);
    assert_eq!(d.hunk_idx, 1);
    d.scroll = 1;
    sync_hunk_from_scroll(&mut d);
    assert_eq!(d.hunk_idx, 0);
}

fn sample_diff_view() -> DiffView {
    DiffView {
        title: "t".into(),
        target: "abc".into(),
        base: None,
        three_dot: false,
        files: vec![],
        file_idx: 0,
        lines: vec![],
        hunks: vec![],
        hunk_idx: 0,
        scroll: 0,
        loading: false,
        header: None,
        error: None,
        blame: Some(vec![]),
        blame_loading: false,
        pending_scroll_restore: None,
    }
}

#[test]
fn diff_screen_toggles_ignore_whitespace_and_full_file() {
    let mut app = App::new();
    // Start from a known state rather than whatever a previous test run
    // persisted to the shared config dir.
    app.prefs.diff_ignore_whitespace = false;
    app.prefs.diff_full_file = false;
    app.screen = Screen::Diff;
    app.diff = Some(sample_diff_view());

    assert!(!app.diff_ignore_whitespace());
    app.handle_key(KeyEvent::from(KeyCode::Char('w')));
    assert!(app.diff_ignore_whitespace());
    app.handle_key(KeyEvent::from(KeyCode::Char('w')));
    assert!(!app.diff_ignore_whitespace());

    assert!(!app.diff_full_file());
    app.handle_key(KeyEvent::from(KeyCode::Char('f')));
    assert!(app.diff_full_file());
}

#[test]
fn navigation_back_preserves_diff_repo_state_then_cleans_up_home() {
    let mut app = App::new();
    app.repo_index = Some(0);
    app.repo_data = Some(RepoSnapshot {
        summary: crate::git::Summary {
            repo_name: "kept".into(),
            repo_path: "/kept".into(),
            current_branch: "main".into(),
            total_commits: 0,
            total_contributors: 0,
            total_branches: 0,
            total_files: 0,
            total_size_bytes: 0,
            total_size_formatted: "0".into(),
            has_upstream: false,
            ahead: 0,
            behind: 0,
            has_remote: false,
            uncommitted_changes: 0,
            remote_ci_pr: None,
            op_state: crate::git::GitOpState::None,
            conflicts: 0,
        },
        commits: vec![],
        commits_err: None,
        branches: vec![],
        branches_err: None,
        tags: vec![],
        tags_err: None,
        stashes: vec![],
        stashes_err: None,
        working_files: vec![],
        working_err: None,
        contributors: vec![],
        contributors_err: None,
        worktrees: vec![],
        worktrees_err: None,
    });
    app.diff = Some(sample_diff_view());
    app.screen = Screen::Diff;
    app.execute_navigation(NavigationAction::Back);
    assert_eq!(app.screen, Screen::Repo);
    assert!(app.diff.is_none());
    assert!(app.repo_data.is_some());
    app.diff = Some(sample_diff_view());
    app.screen = Screen::Diff;
    app.handle_key(KeyEvent::from(KeyCode::Char('?')));
    app.execute_navigation(NavigationAction::Back);
    assert_eq!(app.screen, Screen::Diff);
    assert!(app.diff.is_some());
    app.execute_navigation(NavigationAction::Back);
    assert_eq!(app.screen, Screen::Repo);
    assert!(app.diff.is_none());
    app.execute_navigation(NavigationAction::Back);
    assert_eq!(app.screen, Screen::Home);
    assert!(app.repo_data.is_none());
    assert!(app.repo_index.is_none());
}

#[test]
fn diff_blame_toggle_reverts_when_there_is_nothing_to_blame_yet() {
    // No repo_index and no diff.files: request_blame can't launch a job.
    // The preference must revert (not stick at "on" with no data and no
    // explanation) and the user must be told why.
    let mut app = App::new();
    app.prefs.diff_show_blame = false;
    app.screen = Screen::Diff;
    app.diff = Some(sample_diff_view());

    app.handle_key(KeyEvent::from(KeyCode::Char('b')));
    assert!(!app.diff_show_blame());
    assert!(!app.status.is_empty());
}

#[test]
fn diff_blame_toggle_succeeds_with_a_repo_and_file_open() {
    let mut app = App::new();
    app.prefs.diff_show_blame = false;
    app.repos = vec![repo("r", "/tmp/r")];
    app.repo_index = Some(0);
    app.screen = Screen::Diff;
    let mut diff = sample_diff_view();
    diff.files = vec![crate::git::ChangedFile {
        status: "M".into(),
        path: "f.rs".into(),
        old_path: None,
        additions: 1,
        deletions: 0,
    }];
    app.diff = Some(diff);

    app.handle_key(KeyEvent::from(KeyCode::Char('b')));
    assert!(app.diff_show_blame());
    app.handle_key(KeyEvent::from(KeyCode::Char('b')));
    assert!(!app.diff_show_blame());
    // Turning blame off clears any stale annotations from the previous file.
    assert!(app.diff.as_ref().unwrap().blame.is_none());
}

#[test]
fn auto_refresh_cycles_through_off_30s_1m_5m() {
    let mut app = App::new();
    // Start from a known point in the cycle rather than whatever a previous
    // test run persisted to the shared config dir.
    app.prefs.auto_refresh_secs = 60;
    app.handle_key(KeyEvent::from(KeyCode::Char('s'))); // -> Settings
    app.handle_key(KeyEvent::from(KeyCode::Char('i')));
    assert_eq!(app.prefs.auto_refresh_secs, 300);
    app.handle_key(KeyEvent::from(KeyCode::Char('i')));
    assert_eq!(app.prefs.auto_refresh_secs, 0);
    app.handle_key(KeyEvent::from(KeyCode::Char('i')));
    assert_eq!(app.prefs.auto_refresh_secs, 30);
    app.handle_key(KeyEvent::from(KeyCode::Char('i')));
    assert_eq!(app.prefs.auto_refresh_secs, 60);
}

#[test]
fn auto_refresh_only_fires_on_home_after_the_interval_elapses() {
    use std::time::{Duration, Instant};
    let mut app = App::new();
    app.repos = vec![repo("alpha", "/tmp/alpha")];
    app.prefs.auto_refresh_secs = 30;

    // Just refreshed: too soon to fire again.
    app.last_auto_refresh = Instant::now();
    app.status.clear();
    app.maybe_auto_refresh();
    assert!(app.status.is_empty());

    // Interval elapsed, but not on Home: must not fire (avoids disrupting an
    // open diff/repo view with a fresh "Analyzing..." status).
    app.last_auto_refresh = Instant::now() - Duration::from_secs(31);
    app.screen = Screen::Repo;
    app.maybe_auto_refresh();
    assert!(app.status.is_empty());

    // Interval elapsed and on Home: fires, and resets the timer so the very
    // next tick does not immediately fire again.
    app.screen = Screen::Home;
    app.last_auto_refresh = Instant::now() - Duration::from_secs(31);
    app.maybe_auto_refresh();
    assert!(!app.status.is_empty());
    assert!(app.last_auto_refresh.elapsed() < Duration::from_secs(1));

    // Disabled (0): never fires regardless of elapsed time.
    app.prefs.auto_refresh_secs = 0;
    app.last_auto_refresh = Instant::now() - Duration::from_secs(9999);
    app.status.clear();
    app.maybe_auto_refresh();
    assert!(app.status.is_empty());
}

#[test]
fn bulk_cross_repo_jobs_route_to_the_secondary_worker() {
    // SearchCommits and LoadGlobalMembers each run one git invocation per
    // registered repository, same shape as Pull/Fetch's remote-timeout risk
    // — all four must stay off the shared worker or a single unreachable
    // repo blocks every diff/status load queued behind it.
    let path = PathBuf::from("/tmp/r");
    assert!(
        Job::Pull {
            index: 0,
            path: path.clone(),
            lang: crate::config::Language::English,
        }
        .is_secondary_worker()
    );
    assert!(
        Job::Fetch {
            index: 0,
            path: path.clone(),
            lang: crate::config::Language::English,
        }
        .is_secondary_worker()
    );
    assert!(
        Job::SearchCommits {
            seq: 0,
            query: "x".into(),
            repos: vec![],
        }
        .is_secondary_worker()
    );
    assert!(
        Job::LoadGlobalMembers {
            generation: 0,
            repos: vec![],
            members: vec![],
        }
        .is_secondary_worker()
    );

    // Per-file/per-repo interactive loads must stay on the primary worker —
    // routing these to the secondary one would defeat the whole point.
    assert!(
        !Job::LoadDiff {
            seq: 0,
            path: path.clone(),
            base: None,
            target: "HEAD".into(),
            file: "f".into(),
            three_dot: false,
            ignore_whitespace: false,
            full: false,
        }
        .is_secondary_worker()
    );
    assert!(
        !Job::LoadHome {
            generation: 0,
            index: 0,
            path,
            members: vec![],
        }
        .is_secondary_worker()
    );
}

#[test]
fn ignore_whitespace_toggle_preserves_scroll_position_through_the_worker() {
    use std::process::Command;

    let temp_dir = std::env::temp_dir().join("gdt_app_test_scroll_preserve");
    let _ = std::fs::remove_dir_all(&temp_dir);
    std::fs::create_dir_all(&temp_dir).unwrap();
    for args in [
        vec!["init"],
        vec!["config", "user.name", "T"],
        vec!["config", "user.email", "t@e.com"],
    ] {
        Command::new("git")
            .args(&args)
            .current_dir(&temp_dir)
            .output()
            .unwrap();
    }
    // 40 unchanged lines, committed, then a change near the bottom so the
    // resulting diff has real length for a scroll position to mean anything.
    let base: String = (0..40).map(|i| format!("line {i}\n")).collect();
    std::fs::write(temp_dir.join("f.txt"), &base).unwrap();
    Command::new("git")
        .args(["add", "."])
        .current_dir(&temp_dir)
        .output()
        .unwrap();
    Command::new("git")
        .args(["commit", "-m", "base"])
        .current_dir(&temp_dir)
        .output()
        .unwrap();
    let modified = base.replace("line 35", "line 35 CHANGED");
    std::fs::write(temp_dir.join("f.txt"), modified).unwrap();

    let mut app = App::new();
    app.repos = vec![repo("r", temp_dir.to_str().unwrap())];
    app.repo_index = Some(0);
    app.screen = Screen::Diff;
    let mut diff = sample_diff_view();
    diff.target = crate::git::WORKING_TREE.to_string();
    diff.files = vec![crate::git::ChangedFile {
        status: "M".into(),
        path: "f.txt".into(),
        old_path: None,
        additions: 1,
        deletions: 1,
    }];
    app.diff = Some(diff);

    // First load, to get real line content in.
    app.load_selected_diff_file();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while app.diff.as_ref().is_some_and(|d| d.loading) {
        assert!(
            std::time::Instant::now() < deadline,
            "initial load timed out"
        );
        std::thread::sleep(std::time::Duration::from_millis(20));
        app.drain_messages();
    }
    let line_count = app.diff.as_ref().unwrap().lines.len();
    assert!(
        line_count > 5,
        "expected a multi-line diff, got {line_count}"
    );

    // Scroll well past where apply_hunks' "jump to the first hunk" default
    // would land, so a passing test can't be an accident of both landing on
    // the same spot.
    let target_scroll = line_count - 2;
    app.diff.as_mut().unwrap().scroll = target_scroll;

    app.toggle_diff_ignore_whitespace();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while app.diff.as_ref().is_some_and(|d| d.loading) {
        assert!(std::time::Instant::now() < deadline, "reload timed out");
        std::thread::sleep(std::time::Duration::from_millis(20));
        app.drain_messages();
    }

    assert_eq!(app.diff.as_ref().unwrap().scroll, target_scroll);

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
fn search_commits_across_reports_failed_repos_without_losing_other_hits() {
    use std::process::Command;

    let temp_dir = std::env::temp_dir().join("gdt_search_across_partial_failure");
    let _ = std::fs::remove_dir_all(&temp_dir);
    std::fs::create_dir_all(&temp_dir).unwrap();
    for args in [
        vec!["init"],
        vec!["config", "user.name", "T"],
        vec!["config", "user.email", "t@e.com"],
    ] {
        Command::new("git")
            .args(&args)
            .current_dir(&temp_dir)
            .output()
            .unwrap();
    }
    std::fs::write(temp_dir.join("f.txt"), "x").unwrap();
    Command::new("git")
        .args(["add", "."])
        .current_dir(&temp_dir)
        .output()
        .unwrap();
    Command::new("git")
        .args(["commit", "-m", "unique-partial-failure-marker"])
        .current_dir(&temp_dir)
        .output()
        .unwrap();

    let missing = std::env::temp_dir().join("gdt_this_path_does_not_exist_at_all");
    let _ = std::fs::remove_dir_all(&missing);

    let result = search_commits_across(
        "unique-partial-failure-marker",
        &[
            (0, "good-repo".to_string(), temp_dir.clone()),
            (1, "bad-repo".to_string(), missing),
        ],
    );
    // The failing repo must not silently collapse into "no matches" —
    // unwrap_or_default() previously made this indistinguishable from a
    // real empty result.
    assert_eq!(result.failed_repos, vec!["bad-repo".to_string()]);
    // ...and it must not take the good repo's real hit down with it.
    assert_eq!(result.hits.len(), 1);
    assert_eq!(result.hits[0].repo_name, "good-repo");

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
fn jump_to_search_hit_refuses_a_stale_repo_index() {
    // hit.repo_index is a snapshot from when the search was dispatched; if
    // the repo at that position changed name (deleted/reordered/replaced)
    // before the user pressed Enter, jumping there would open the wrong
    // repository silently. Detect the mismatch instead.
    let mut app = App::new();
    app.repos = vec![repo("actually-a-different-repo", "/tmp/x")];
    app.commit_search = Some(CommitSearchState {
        query: "q".into(),
        hits: vec![CommitSearchHit {
            repo_index: 0,
            repo_name: "original-repo-name".into(),
            hash: "abc123".into(),
            author: "A".into(),
            date: "2024-01-01".into(),
            message: "m".into(),
        }],
        selected: 0,
        loading: false,
    });
    app.screen = Screen::CommitSearch;

    app.handle_key(KeyEvent::from(KeyCode::Enter));

    assert!(app.error.is_some());
    // Must not have opened the (wrong) repo at that index.
    assert_eq!(app.screen, Screen::CommitSearch);
    assert_eq!(app.repo_index, None);
}

#[test]
fn commit_search_end_to_end_through_the_worker() {
    use std::process::Command;

    let temp_dir = std::env::temp_dir().join("gdt_app_test_commit_search");
    let _ = std::fs::remove_dir_all(&temp_dir);
    std::fs::create_dir_all(&temp_dir).unwrap();
    for args in [
        vec!["init"],
        vec!["config", "user.name", "Tester"],
        vec!["config", "user.email", "t@example.com"],
    ] {
        Command::new("git")
            .args(&args)
            .current_dir(&temp_dir)
            .output()
            .unwrap();
    }
    std::fs::write(temp_dir.join("f.txt"), "hello").unwrap();
    Command::new("git")
        .args(["add", "."])
        .current_dir(&temp_dir)
        .output()
        .unwrap();
    Command::new("git")
        .args(["commit", "-m", "unique-search-marker-9f3c"])
        .current_dir(&temp_dir)
        .output()
        .unwrap();

    let mut app = App::new();
    app.repos = vec![repo("search-target", temp_dir.to_str().unwrap())];
    app.screen = Screen::Home;

    app.handle_key(KeyEvent::from(KeyCode::Char('S')));
    assert_eq!(app.input, Some(InputKind::CommitSearchQuery));
    for c in "unique-search-marker-9f3c".chars() {
        app.handle_key(KeyEvent::from(KeyCode::Char(c)));
    }
    app.handle_key(KeyEvent::from(KeyCode::Enter));
    assert_eq!(app.screen, Screen::CommitSearch);
    assert!(app.commit_search.as_ref().unwrap().loading);

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while app.commit_search.as_ref().is_some_and(|s| s.loading) {
        assert!(
            std::time::Instant::now() < deadline,
            "search did not complete in time"
        );
        std::thread::sleep(std::time::Duration::from_millis(20));
        app.drain_messages();
    }

    let search = app.commit_search.as_ref().unwrap();
    assert_eq!(search.hits.len(), 1);
    assert_eq!(search.hits[0].repo_index, 0);
    assert_eq!(search.hits[0].message, "unique-search-marker-9f3c");
    let expected_hash = search.hits[0].hash.clone();

    app.handle_key(KeyEvent::from(KeyCode::Enter));
    assert_eq!(app.repo_index, Some(0));
    assert_eq!(app.screen, Screen::Diff);
    assert_eq!(app.diff.as_ref().unwrap().target, expected_hash);

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
fn theme_toggles_between_mocha_and_latte() {
    let mut app = App::new();
    app.prefs.theme = crate::colors::THEME_MOCHA.to_string();
    app.handle_key(KeyEvent::from(KeyCode::Char('s'))); // -> Settings
    app.handle_key(KeyEvent::from(KeyCode::Char('T')));
    assert_eq!(app.theme_name(), crate::colors::THEME_LATTE);
    app.handle_key(KeyEvent::from(KeyCode::Char('T')));
    assert_eq!(app.theme_name(), crate::colors::THEME_MOCHA);
}

#[test]
fn open_repo_lands_on_commits() {
    let mut app = App::new();
    app.repos = vec![repo("alpha", "/tmp/alpha")];
    app.open_repo(0);
    assert_eq!(app.screen, Screen::Repo);
    assert_eq!(app.repo_tab, RepoTab::Commits);
}

#[test]
fn q_quits_from_repo_and_esc_goes_home() {
    let mut app = App::new();
    app.screen = Screen::Repo;
    app.handle_key(KeyEvent::from(KeyCode::Esc));
    assert_eq!(app.screen, Screen::Home);
    assert!(!app.should_quit);
    app.screen = Screen::Repo;
    app.handle_key(KeyEvent::from(KeyCode::Char('q')));
    assert!(app.should_quit);
}

#[test]
fn help_returns_to_previous_screen() {
    let mut app = App::new();
    app.screen = Screen::Repo;
    app.handle_key(KeyEvent::from(KeyCode::Char('?')));
    assert_eq!(app.screen, Screen::Help);
    app.handle_key(KeyEvent::from(KeyCode::Esc));
    assert_eq!(app.screen, Screen::Repo);

    // '?' also toggles back
    app.handle_key(KeyEvent::from(KeyCode::Char('?')));
    assert_eq!(app.screen, Screen::Help);
    app.handle_key(KeyEvent::from(KeyCode::Char('?')));
    assert_eq!(app.screen, Screen::Repo);

    // 'q' also returns back without quitting
    app.handle_key(KeyEvent::from(KeyCode::Char('?')));
    assert_eq!(app.screen, Screen::Help);
    app.handle_key(KeyEvent::from(KeyCode::Char('q')));
    assert_eq!(app.screen, Screen::Repo);
    assert!(!app.should_quit);
}

#[test]
fn expand_tilde_and_strip_ansi() {
    // Resolve the expected home the same way the code under test does:
    // reading $HOME directly fails on Windows, where home_dir() falls back to
    // %USERPROFILE% and the assertion would compare against a bogus literal.
    let home = home_dir().expect("a home directory is set in the test environment");
    assert_eq!(expand_user_path("~/work/repo"), home.join("work/repo"));
    assert_eq!(strip_ansi("\u{1b}[32mgreen\u{1b}[0m"), "green");
}

#[test]
fn commit_filter_narrows_visible_indices() {
    let mut app = App::new();
    app.screen = Screen::Repo;
    app.repo_tab = RepoTab::Commits;
    app.repo_data = Some(RepoSnapshot {
        summary: crate::git::Summary {
            repo_name: "x".into(),
            repo_path: "/tmp/x".into(),
            current_branch: "main".into(),
            total_commits: 2,
            total_contributors: 1,
            total_branches: 1,
            total_files: 1,
            total_size_bytes: 0,
            total_size_formatted: "0".into(),
            has_upstream: false,
            ahead: 0,
            behind: 0,
            has_remote: false,
            uncommitted_changes: 0,
            remote_ci_pr: None,
            op_state: crate::git::GitOpState::None,
            conflicts: 0,
        },
        commits: vec![
            CommitSummary {
                hash: "aaa".into(),
                author: "Ann".into(),
                date: "2026-01-01".into(),
                message: "fix footer".into(),
                graph: String::new(),
                refs: Vec::new(),
            },
            CommitSummary {
                hash: "bbb".into(),
                author: "Bob".into(),
                date: "2026-01-02".into(),
                message: "add tui".into(),
                graph: String::new(),
                refs: Vec::new(),
            },
        ],
        commits_err: None,
        branches: vec![],
        branches_err: None,
        tags: vec![],
        tags_err: None,
        stashes: vec![],
        stashes_err: None,
        working_files: vec![],
        working_err: None,
        contributors: vec![],
        contributors_err: None,
        worktrees: vec![],
        worktrees_err: None,
    });
    app.handle_key(KeyEvent::from(KeyCode::Char('/')));
    app.handle_key(KeyEvent::from(KeyCode::Char('t')));
    app.handle_key(KeyEvent::from(KeyCode::Char('u')));
    app.handle_key(KeyEvent::from(KeyCode::Char('i')));
    assert_eq!(app.visible_indices(), vec![1]);
}

#[test]
fn sort_repo_indices_by_name_and_date() {
    let repos = vec![repo("zeta", "/z"), repo("alpha", "/a")];
    let mut rows = HashMap::new();
    rows.insert(
        0,
        HomeRow {
            branch: "main".into(),
            ahead: 0,
            behind: 0,
            dirty: 0,
            last_commit: "2026-01-01".into(),
            open_prs: None,
            ci_status: None,
            op_state: crate::git::GitOpState::None,
            conflicts: 0,
            ..Default::default()
        },
    );
    rows.insert(
        1,
        HomeRow {
            branch: "main".into(),
            ahead: 0,
            behind: 0,
            dirty: 0,
            last_commit: "2026-08-01".into(),
            open_prs: None,
            ci_status: None,
            op_state: crate::git::GitOpState::None,
            conflicts: 0,
            ..Default::default()
        },
    );
    let mut idx = vec![0, 1];
    sort_repo_indices(&mut idx, &repos, &rows, 0);
    assert_eq!(idx, vec![1, 0]);
    sort_repo_indices(&mut idx, &repos, &rows, 2);
    assert_eq!(idx, vec![1, 0]);
}

fn home_row(
    ci_status: Option<&str>,
    op_state: crate::git::GitOpState,
    conflicts: usize,
) -> HomeRow {
    HomeRow {
        branch: "main".into(),
        ahead: 0,
        behind: 0,
        dirty: 0,
        last_commit: "2026-01-01".into(),
        open_prs: None,
        ci_status: ci_status.map(str::to_string),
        op_state,
        conflicts,
        ..Default::default()
    }
}

#[test]
fn needs_attention_flags_failing_ci_conflicts_and_op_state() {
    use crate::git::GitOpState;
    assert!(!needs_attention(&home_row(None, GitOpState::None, 0)));
    assert!(!needs_attention(&home_row(
        Some("success"),
        GitOpState::None,
        0
    )));
    assert!(needs_attention(&home_row(
        Some("failure"),
        GitOpState::None,
        0
    )));
    assert!(needs_attention(&home_row(None, GitOpState::Rebase, 0)));
    assert!(needs_attention(&home_row(None, GitOpState::None, 1)));
}

#[test]
fn classify_ci_status_treats_cancelled_timed_out_and_action_required_as_failing() {
    // A timed-out or cancelled run, or one blocked on a human action, means
    // something needs a look — the same as an outright test failure. These
    // were previously falling through to the neutral "Other" bucket, so the
    // `n` filter treated a timed-out pipeline as "nothing wrong here".
    assert_eq!(classify_ci_status("success"), CiOutcome::Success);
    assert_eq!(classify_ci_status("failure"), CiOutcome::Failure);
    assert_eq!(classify_ci_status("cancelled"), CiOutcome::Failure);
    assert_eq!(classify_ci_status("timed_out"), CiOutcome::Failure);
    assert_eq!(classify_ci_status("action_required"), CiOutcome::Failure);
    assert_eq!(classify_ci_status("in_progress"), CiOutcome::Other);
    assert_eq!(classify_ci_status("skipped"), CiOutcome::Other);
    assert_eq!(classify_ci_status(""), CiOutcome::Other);

    assert!(needs_attention(&home_row(
        Some("cancelled"),
        crate::git::GitOpState::None,
        0
    )));
    assert!(needs_attention(&home_row(
        Some("timed_out"),
        crate::git::GitOpState::None,
        0
    )));
}

#[test]
fn attention_only_filter_hides_clean_repos_and_unloaded_rows() {
    let mut app = App::new();
    app.repos = vec![
        repo("clean", "/tmp/clean"),
        repo("failing-ci", "/tmp/failing"),
        repo("not-loaded-yet", "/tmp/pending"),
    ];
    app.home_rows.insert(
        0,
        home_row(Some("success"), crate::git::GitOpState::None, 0),
    );
    app.home_rows.insert(
        1,
        home_row(Some("failure"), crate::git::GitOpState::None, 0),
    );
    // index 2 has no row yet — must not be guessed as needing attention

    assert_eq!(app.filtered_home(), vec![0, 1, 2]);

    app.handle_key(KeyEvent::from(KeyCode::Char('n')));
    assert!(app.attention_only);
    assert_eq!(app.filtered_home(), vec![1]);

    app.handle_key(KeyEvent::from(KeyCode::Char('n')));
    assert!(!app.attention_only);
    assert_eq!(app.filtered_home(), vec![0, 1, 2]);
}

#[test]
fn home_selected_is_reclamped_when_a_background_load_shrinks_the_filtered_list() {
    let mut app = App::new();
    app.prefs.repo_sort = 0; // name ascending, so filtered order is deterministic
    app.repos = vec![
        repo("a", "/tmp/a"),
        repo("b", "/tmp/b"),
        repo("c", "/tmp/c"),
    ];
    app.home_rows.insert(
        0,
        home_row(Some("failure"), crate::git::GitOpState::None, 0),
    );
    app.home_rows.insert(
        2,
        home_row(Some("failure"), crate::git::GitOpState::None, 0),
    );
    // repo 1 ("b") has no row loaded yet, so it's excluded either way.
    app.attention_only = true;
    assert_eq!(app.filtered_home(), vec![0, 2]);

    app.home_selected = 1; // pointing at repo 2 ("c"), the second attention-needing row
    let generation = app.home_generation();
    app.apply_msg(Msg::HomeLoaded {
        generation,
        index: 2,
        row: Ok(home_row(Some("success"), crate::git::GitOpState::None, 0)),
    });

    // "c" no longer needs attention, so the filtered list shrank to just
    // [0] — home_selected must not still point past the end of it.
    assert_eq!(app.filtered_home(), vec![0]);
    assert_eq!(app.home_selected, 0);
}

#[test]
fn resolve_hunk_command_variants() {
    let repo = PathBuf::from("/tmp/repo");
    let wt = resolve_diff_command("hunk", &repo, None, "WORKING_TREE").unwrap();
    assert_eq!(wt.args, vec!["diff"]);
    let show = resolve_diff_command("hunk", &repo, None, "abc123").unwrap();
    assert_eq!(show.args, vec!["show", "abc123"]);
    let range = resolve_diff_command("hunk", &repo, Some("aaa"), "bbb").unwrap();
    assert_eq!(range.args, vec!["diff", "aaa..bbb"]);
    let templated =
        resolve_diff_command("hunk show {base} {target}", &repo, Some("aaa"), "bbb").unwrap();
    assert_eq!(templated.args, vec!["diff", "aaa..bbb"]);
    assert!(resolve_diff_command("", &repo, None, "abc").is_err());
}

#[test]
fn test_toggle_member_active() {
    let mut app = App::new();
    app.members = vec![Member {
        canonical_name: "Alice".into(),
        aliases: vec!["alice".into()],
        is_active: true,
    }];
    assert!(app.members[0].is_active);
    app.toggle_member_active(0);
    assert!(!app.members[0].is_active);
    app.toggle_member_active(0);
    assert!(app.members[0].is_active);
}

#[test]
fn test_settings_tab_switch() {
    let mut app = App::new();
    app.screen = Screen::Settings;
    assert_eq!(app.settings_tab, SettingsTab::Repositories);
    app.handle_key(KeyEvent::from(KeyCode::Tab));
    assert_eq!(app.settings_tab, SettingsTab::Members);
    app.handle_key(KeyEvent::from(KeyCode::BackTab));
    assert_eq!(app.settings_tab, SettingsTab::Repositories);
    app.handle_key(KeyEvent::from(KeyCode::Char('1')));
    assert_eq!(app.settings_tab, SettingsTab::Repositories);
    app.handle_key(KeyEvent::from(KeyCode::Char('2')));
    assert_eq!(app.settings_tab, SettingsTab::Members);
}

#[test]
fn test_group_cycle_and_filter() {
    let mut app = App::new();
    let mut r1 = repo("alpha", "/tmp/alpha");
    r1.group = Some("Work".to_string());
    let mut r2 = repo("beta", "/tmp/beta");
    r2.group = Some("Personal".to_string());
    let r3 = repo("gamma", "/tmp/gamma");
    app.repos = vec![r1, r2, r3];

    assert_eq!(app.group_filter, None);
    assert_eq!(app.filtered_home().len(), 3);

    app.cycle_group_filter(1);
    assert_eq!(app.group_filter, Some("Personal".to_string()));
    assert_eq!(app.filtered_home(), vec![1]);

    app.cycle_group_filter(1);
    assert_eq!(app.group_filter, Some("Work".to_string()));
    assert_eq!(app.filtered_home(), vec![0]);

    app.cycle_group_filter(1);
    assert_eq!(app.group_filter, Some("".to_string()));
    assert_eq!(app.filtered_home(), vec![2]);

    app.cycle_group_filter(1);
    assert_eq!(app.group_filter, None);
    assert_eq!(app.filtered_home().len(), 3);

    app.cycle_group_filter(-1);
    assert_eq!(app.group_filter, Some("".to_string()));
}

#[test]
fn test_time_span_cycle() {
    let mut app = App::new();
    assert_eq!(app.contributor_time_span, TimeSpan::All);
    app.cycle_contributor_time_span();
    assert_eq!(app.contributor_time_span, TimeSpan::OneWeek);
    app.cycle_contributor_time_span();
    assert_eq!(app.contributor_time_span, TimeSpan::OneMonth);
    app.cycle_contributor_time_span();
    assert_eq!(app.contributor_time_span, TimeSpan::ThreeMonths);
    app.cycle_contributor_time_span();
    assert_eq!(app.contributor_time_span, TimeSpan::All);
}

#[test]
fn test_global_members_navigation() {
    let mut app = App::new();
    app.global_members = vec![
        GlobalMember {
            canonical_name: "Alice".into(),
            aliases: vec!["alice".into()],
            is_active: true,
            total_commits: 100,
            repo_count: 2,
            latest_commit_date: "2026-08-15".into(),
            contributions: vec![
                MemberRepoContribution {
                    repo_name: "repo1".into(),
                    repo_path: PathBuf::from("/tmp/repo1"),
                    repo_index: 0,
                    commit_count: 70,
                    first_commit: "2026-01-01".into(),
                    last_commit: "2026-08-15".into(),
                },
                MemberRepoContribution {
                    repo_name: "repo2".into(),
                    repo_path: PathBuf::from("/tmp/repo2"),
                    repo_index: 1,
                    commit_count: 30,
                    first_commit: "2026-02-01".into(),
                    last_commit: "2026-07-20".into(),
                },
            ],
        },
        GlobalMember {
            canonical_name: "Bob".into(),
            aliases: vec!["bob".into()],
            is_active: false,
            total_commits: 40,
            repo_count: 1,
            latest_commit_date: "2026-06-01".into(),
            contributions: vec![MemberRepoContribution {
                repo_name: "repo1".into(),
                repo_path: PathBuf::from("/tmp/repo1"),
                repo_index: 0,
                commit_count: 40,
                first_commit: "2026-03-01".into(),
                last_commit: "2026-06-01".into(),
            }],
        },
    ];
    app.screen = Screen::GlobalMembers;
    assert_eq!(app.global_member_selected, 0);
    assert_eq!(app.global_member_pane, FocusPane::List);

    // Move to Bob
    app.handle_key(KeyEvent::from(KeyCode::Char('j')));
    assert_eq!(app.global_member_selected, 1);

    // Filter active only
    app.handle_key(KeyEvent::from(KeyCode::Char('m')));
    assert!(app.active_only);
    assert_eq!(app.filtered_global_members(), vec![0]);

    // Toggle active back
    app.handle_key(KeyEvent::from(KeyCode::Char('m')));
    assert!(!app.active_only);

    // Switch to Alice and switch pane to content
    app.global_member_selected = 0;
    app.handle_key(KeyEvent::from(KeyCode::Tab));
    assert_eq!(app.global_member_pane, FocusPane::Content);

    // Move repo selection
    app.handle_key(KeyEvent::from(KeyCode::Char('j')));
    assert_eq!(app.global_member_repo_selected, 1);

    // Switch back to list pane
    app.handle_key(KeyEvent::from(KeyCode::Char('h')));
    assert_eq!(app.global_member_pane, FocusPane::List);

    // Esc returns to Home
    app.handle_key(KeyEvent::from(KeyCode::Esc));
    assert_eq!(app.screen, Screen::Home);
}

#[test]
fn mouse_global_member_viewports_apply_filter_offset_and_reclick_open() {
    let mut app = App::new();
    app.repos = vec![repo("repo-a", "/repo-a"), repo("repo-b", "/repo-b")];
    app.global_members = vec![
        GlobalMember {
            canonical_name: "Alice".into(),
            aliases: vec![],
            is_active: true,
            total_commits: 1,
            repo_count: 1,
            latest_commit_date: "".into(),
            contributions: vec![MemberRepoContribution {
                repo_name: "repo-a".into(),
                repo_path: "/repo-a".into(),
                repo_index: 0,
                commit_count: 1,
                first_commit: "".into(),
                last_commit: "".into(),
            }],
        },
        GlobalMember {
            canonical_name: "Bob".into(),
            aliases: vec!["bobby".into()],
            is_active: true,
            total_commits: 2,
            repo_count: 1,
            latest_commit_date: "".into(),
            contributions: vec![MemberRepoContribution {
                repo_name: "repo-b".into(),
                repo_path: "/repo-b".into(),
                repo_index: 1,
                commit_count: 2,
                first_commit: "".into(),
                last_commit: "".into(),
            }],
        },
    ];
    app.screen = Screen::GlobalMembers;
    app.global_member_filter.clear();
    app.global_members_viewport.set(ListViewport {
        x: 1,
        y: 5,
        width: 20,
        height: 1,
        offset: 1,
    });
    app.global_member_repos_viewport.set(ListViewport {
        x: 25,
        y: 5,
        width: 20,
        height: 1,
        offset: 0,
    });
    app.global_member_selected = 0;
    app.handle_mouse_click(2, 5);
    assert_eq!(app.filtered_global_members(), vec![0, 1]);
    assert_eq!(app.global_member_selected, 1);
    assert_eq!(app.global_member_repo_selected, 0);
    assert_eq!(app.global_member_pane, FocusPane::List);
    app.global_member_filter = "bob".into();
    app.global_member_selected = 0;
    assert_eq!(app.filtered_global_members(), vec![1]);
    app.handle_mouse_click(25, 4); // table header is outside the recorded data viewport
    assert_eq!(app.screen, Screen::GlobalMembers);
    assert_eq!(
        app.global_member_repos_viewport
            .get()
            .index_at_position(25, 5),
        Some(0)
    );
    app.handle_mouse_click(25, 5);
    assert_eq!(app.global_member_pane, FocusPane::Content);
    app.handle_mouse_click(25, 5);
    assert_eq!(app.screen, Screen::Repo);
    assert_eq!(app.repo_index, Some(1));
}

#[test]
fn test_open_terminal_request() {
    let mut app = App::new();
    let temp = std::env::temp_dir();
    app.repos = vec![repo("local-temp", &temp.to_string_lossy())];
    app.screen = Screen::Home;
    app.home_selected = 0;

    assert_eq!(app.take_terminal(), None);
    app.handle_key(KeyEvent::from(KeyCode::Char('t')));
    assert_eq!(app.take_terminal(), Some(temp.clone()));

    // Shift+T also works
    app.handle_key(KeyEvent::from(KeyCode::Char('T')));
    assert_eq!(app.take_terminal(), Some(temp));
}

#[test]
fn test_sanitize_path_input() {
    assert_eq!(sanitize_path_input("'~/work/my repo'"), "~/work/my repo");
    assert_eq!(sanitize_path_input("\"/tmp/repo\""), "/tmp/repo");
    assert_eq!(
        sanitize_path_input("/path/with\\ space"),
        "/path/with space"
    );
}

#[test]
fn test_path_completion_basic() {
    let completions = complete_path(".");
    assert!(!completions.is_empty());
    for c in completions {
        assert!(c.ends_with('/'));
    }
}

#[test]
fn test_repo_finder_workflow() {
    let mut app = App::new();
    let temp_dir = std::env::temp_dir().join(format!("test_finder_{}", std::process::id()));
    let _ = std::fs::create_dir_all(&temp_dir);

    let r1 = temp_dir.join("repo1");
    let r2 = temp_dir.join("repo2");
    let _ = std::fs::create_dir_all(r1.join(".git"));
    let _ = std::fs::create_dir_all(r2.join(".git"));

    // Open repo finder pointing to temp_dir
    app.open_repo_finder(Some(temp_dir.clone()));
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while app.repo_finder.as_ref().unwrap().loading {
        app.drain_messages();
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert_eq!(app.screen, Screen::RepoFinder);
    assert!(app.repo_finder.is_some());
    let finder = app.repo_finder.as_ref().unwrap();
    assert_eq!(finder.repos.len(), 2);
    assert!(finder.repos[0].is_selected);
    assert!(finder.repos[1].is_selected);

    // Discovery order varies by filesystem. Sorting preserves the selected
    // repository's identity, so it need not occupy row zero after completion.
    let selected = app.filtered_finder_repos()[finder.selected_idx];
    app.handle_key(KeyEvent::from(KeyCode::Char(' ')));
    assert!(!app.repo_finder.as_ref().unwrap().repos[selected].is_selected);

    // Select all again
    app.handle_key(KeyEvent::from(KeyCode::Char('a')));
    assert!(app.repo_finder.as_ref().unwrap().repos[0].is_selected);

    // Import
    app.handle_key(KeyEvent::from(KeyCode::Enter));
    assert_eq!(app.screen, Screen::Home);
    assert!(app.repos.iter().any(|r| r.name == "repo1"));
    assert!(app.repos.iter().any(|r| r.name == "repo2"));

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_mouse_scroll_home() {
    let mut app = App::new();
    app.repos = vec![repo("r1", "/p1"), repo("r2", "/p2"), repo("r3", "/p3")];
    app.home_selected = 0;

    let scroll_down = crossterm::event::MouseEvent {
        kind: crossterm::event::MouseEventKind::ScrollDown,
        column: 10,
        row: 5,
        modifiers: crossterm::event::KeyModifiers::NONE,
    };
    app.handle_mouse(scroll_down);
    assert_eq!(app.home_selected, 1);

    let scroll_up = crossterm::event::MouseEvent {
        kind: crossterm::event::MouseEventKind::ScrollUp,
        column: 10,
        row: 5,
        modifiers: crossterm::event::KeyModifiers::NONE,
    };
    app.handle_mouse(scroll_up);
    assert_eq!(app.home_selected, 0);
}

fn mouse(kind: MouseEventKind, column: u16, row: u16) -> MouseEvent {
    MouseEvent {
        kind,
        column,
        row,
        modifiers: KeyModifiers::NONE,
    }
}

#[test]
fn mouse_settings_uses_rendered_bilingual_tabs_and_rows() {
    let mut app = App::new();
    app.repos = (0..30)
        .map(|i| repo(&format!("repo-{i}"), &format!("/repo-{i}")))
        .collect();
    app.members = vec![
        Member {
            canonical_name: "Alice".into(),
            aliases: vec![],
            is_active: true,
        },
        Member {
            canonical_name: "Bob".into(),
            aliases: vec![],
            is_active: true,
        },
    ];
    app.settings_selected = 29;
    app.screen = Screen::Settings;
    crate::ui::tests::render(&app, 40, 24);
    let vp = app.settings_viewport.get();
    assert!(vp.offset > 0);
    app.handle_mouse(mouse(MouseEventKind::Down(MouseButton::Left), vp.x, vp.y));
    assert_eq!(app.settings_selected, vp.offset);
    // Change only this test instance; the key binding persists preferences and
    // would contaminate later tests that construct a fresh App.
    app.set_language_for_test(crate::config::Language::Japanese);
    crate::ui::tests::render(&app, 100, 24);
    let member_x = app.settings_tab_bounds.borrow()[1].0;
    app.handle_mouse(mouse(
        MouseEventKind::Down(MouseButton::Left),
        member_x,
        app.settings_tab_row.get(),
    ));
    assert_eq!(app.settings_tab, SettingsTab::Members);
    // The row hitbox follows the Japanese layout and remains bounded.
    crate::ui::tests::render(&app, 100, 24);
    let vp = app.settings_viewport.get();
    app.handle_mouse(mouse(
        MouseEventKind::Down(MouseButton::Left),
        vp.x + vp.width - 1,
        vp.y + 1,
    ));
    assert_eq!(app.settings_member_selected, 1);
}

#[test]
fn mouse_finder_selects_checkbox_registered_rows_and_filtered_wheel() {
    let mut app = App::new();
    app.screen = Screen::RepoFinder;
    app.repo_finder = Some(RepoFinderState {
        loading: false,
        errors: vec![],
        scan_root: PathBuf::from("/tmp"),
        selected_idx: 0,
        filter: String::new(),
        repos: (0..12)
            .map(|i| FoundRepo {
                path: PathBuf::from(format!("/repo-{i}")),
                name: match i {
                    0 => "alpha".into(),
                    1 => "beta".into(),
                    2 => "gamma".into(),
                    _ => format!("repo-{i}"),
                },
                branch: "main".into(),
                is_already_added: i == 1,
                is_selected: i == 1,
            })
            .collect(),
    });
    app.repo_finder.as_mut().unwrap().selected_idx = 11;
    crate::ui::tests::render(&app, 40, 12);
    let vp = app.finder_viewport.get();
    assert!(vp.offset > 0);
    app.handle_mouse(mouse(MouseEventKind::Down(MouseButton::Left), vp.x, vp.y));
    assert_eq!(app.repo_finder.as_ref().unwrap().selected_idx, vp.offset);
    app.repo_finder.as_mut().unwrap().selected_idx = 0;
    crate::ui::tests::render(&app, 100, 24);
    let vp = app.finder_viewport.get();
    app.handle_mouse(mouse(
        MouseEventKind::Down(MouseButton::Left),
        vp.x + 2,
        vp.y,
    ));
    assert!(app.repo_finder.as_ref().unwrap().repos[0].is_selected);
    // Clicking an unselected row's checkbox selects that row and toggles it
    // in the same event.
    app.handle_mouse(mouse(
        MouseEventKind::Down(MouseButton::Left),
        vp.x + 2,
        vp.y + 2,
    ));
    assert_eq!(app.repo_finder.as_ref().unwrap().selected_idx, 2);
    assert!(app.repo_finder.as_ref().unwrap().repos[2].is_selected);
    // A registered row can be selected but its checkbox cannot be changed.
    app.handle_mouse(mouse(
        MouseEventKind::Down(MouseButton::Left),
        vp.x,
        vp.y + 1,
    ));
    app.handle_mouse(mouse(
        MouseEventKind::Down(MouseButton::Left),
        vp.x + 2,
        vp.y + 1,
    ));
    assert!(app.repo_finder.as_ref().unwrap().repos[1].is_selected);
    app.repo_finder.as_mut().unwrap().filter = "gamma".into();
    crate::ui::tests::render(&app, 100, 24);
    assert_eq!(app.filtered_finder_repos(), vec![2]);
    app.handle_mouse(mouse(MouseEventKind::ScrollDown, vp.x, vp.y));
    assert_eq!(app.repo_finder.as_ref().unwrap().selected_idx, 0);
}

#[test]
fn mouse_scroll_and_click_are_blocked_by_input_and_home_reclick_opens() {
    let mut app = App::new();
    app.screen = Screen::RepoFinder;
    app.repo_finder = Some(RepoFinderState {
        loading: false,
        errors: vec![],
        scan_root: PathBuf::from("/tmp"),
        selected_idx: 0,
        filter: String::new(),
        repos: vec![
            FoundRepo {
                path: PathBuf::from("/one"),
                name: "one".into(),
                branch: "main".into(),
                is_already_added: false,
                is_selected: false,
            },
            FoundRepo {
                path: PathBuf::from("/two"),
                name: "two".into(),
                branch: "main".into(),
                is_already_added: false,
                is_selected: false,
            },
        ],
    });
    app.input = Some(InputKind::FinderScanPath);
    app.handle_mouse(mouse(MouseEventKind::ScrollDown, 2, 2));
    assert_eq!(app.repo_finder.as_ref().unwrap().selected_idx, 0);
    app.input = None;
    app.handle_mouse(mouse(MouseEventKind::ScrollDown, 2, 2));
    assert_eq!(app.repo_finder.as_ref().unwrap().selected_idx, 1);
    app.repos = vec![repo("one", "/one"), repo("two", "/two")];
    app.screen = Screen::Home;
    crate::ui::tests::render(&app, 120, 24);
    let (header, _) = app.home_table_bounds.get();
    app.handle_mouse(mouse(
        MouseEventKind::Down(MouseButton::Left),
        5,
        header + 1,
    ));
    app.handle_mouse(mouse(
        MouseEventKind::Down(MouseButton::Left),
        5,
        header + 1,
    ));
    assert_eq!(app.screen, Screen::Repo);
}

#[test]
fn test_mouse_click_repo_tabs() {
    let mut app = App::new();
    app.screen = Screen::Repo;
    app.repo_tab = RepoTab::Status;
    // The click hit-test uses bounds the renderer records, so draw once first.
    crate::ui::tests::render(&app, 120, 24);

    let click_commits = crossterm::event::MouseEvent {
        kind: crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left),
        column: 20, // Commits tab range
        row: 1,
        modifiers: crossterm::event::KeyModifiers::NONE,
    };
    app.handle_mouse(click_commits);
    assert_eq!(app.repo_tab, RepoTab::Commits);

    let click_branches = crossterm::event::MouseEvent {
        kind: crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left),
        column: 35, // Branches tab range
        row: 1,
        modifiers: crossterm::event::KeyModifiers::NONE,
    };
    app.handle_mouse(click_branches);
    assert_eq!(app.repo_tab, RepoTab::Branches);
}

#[test]
fn test_mouse_scroll_diff() {
    let mut app = App::new();
    app.screen = Screen::Diff;
    app.diff = Some(DiffView {
        title: "test".to_string(),
        target: "HEAD".to_string(),
        base: None,
        three_dot: false,
        files: vec![],
        file_idx: 0,
        lines: vec![],
        hunks: vec![],
        hunk_idx: 0,
        scroll: 0,
        loading: false,
        header: None,
        error: None,
        blame: None,
        blame_loading: false,
        pending_scroll_restore: None,
    });

    let scroll_down = crossterm::event::MouseEvent {
        kind: crossterm::event::MouseEventKind::ScrollDown,
        column: 10,
        row: 10,
        modifiers: crossterm::event::KeyModifiers::NONE,
    };
    app.handle_mouse(scroll_down);
    assert_eq!(app.diff.as_ref().unwrap().scroll, 3);
}

#[test]
fn test_worktrees_tab_and_terminal() {
    let mut app = App::new();
    app.repos = vec![repo("main_repo", "/path/to/repo")];
    app.repo_index = Some(0);
    app.screen = Screen::Repo;
    app.repo_tab = RepoTab::Worktrees;

    app.repo_data = Some(RepoSnapshot {
        summary: crate::git::Summary {
            repo_name: "main_repo".into(),
            repo_path: "/path/to/repo".into(),
            current_branch: "main".into(),
            total_commits: 1,
            total_contributors: 1,
            total_branches: 1,
            total_files: 1,
            total_size_bytes: 0,
            total_size_formatted: "0".into(),
            has_upstream: false,
            ahead: 0,
            behind: 0,
            has_remote: false,
            uncommitted_changes: 0,
            remote_ci_pr: None,
            op_state: crate::git::GitOpState::None,
            conflicts: 0,
        },
        commits: vec![],
        commits_err: None,
        branches: vec![],
        branches_err: None,
        tags: vec![],
        tags_err: None,
        stashes: vec![],
        stashes_err: None,
        working_files: vec![],
        working_err: None,
        contributors: vec![],
        contributors_err: None,
        worktrees: vec![crate::git::WorktreeInfo {
            path: "/path/to/worktree-feature".into(),
            head: "abcdef1".into(),
            branch: Some("feature-x".into()),
            is_bare: false,
            is_detached: false,
            is_locked: false,
            is_prunable: false,
        }],
        worktrees_err: None,
    });

    assert_eq!(app.visible_indices(), vec![0]);

    // Press Enter to open terminal in the worktree
    app.handle_key(KeyEvent::from(KeyCode::Enter));
    assert_eq!(
        app.take_terminal(),
        Some(PathBuf::from("/path/to/worktree-feature"))
    );
}

#[test]
fn short_hash_is_char_boundary_safe() {
    // A corrupt .git/HEAD or a worktree record can hold arbitrary text, where
    // slicing &s[..7] panics — in the render path, on every redraw.
    assert_eq!(short_hash("abc1234def"), "abc1234");
    assert_eq!(short_hash("参照テスト"), "参照テスト");
    assert_eq!(short_hash("日本語のテキストです"), "日本語のテキス");
    assert_eq!(short_hash("ab"), "ab");
    assert_eq!(short_hash(""), "");
}

#[test]
fn clamp_index_handles_shrinking_lists() {
    assert_eq!(clamp_index(150, 3), 2);
    assert_eq!(clamp_index(150, 0), 0);
    assert_eq!(clamp_index(1, 3), 1);
}

#[test]
fn home_mouse_click_accounts_for_the_scrolled_viewport() {
    // The Home table scrolls its own viewport, so the top visible row is not
    // index 0 once the list is longer than the screen. Clicking a row has to
    // add the renderer's recorded offset, or every click selects the wrong
    // repository (and the first click jumps the viewport to the top).
    let mut app = App::new();
    app.prefs.repo_sort = 0; // name ascending, so order is deterministic
    app.repos = (0..30)
        .map(|i| repo(&format!("repo-{i:02}"), &format!("/tmp/r{i:02}")))
        .collect();

    // Viewport scrolled so that index 10 is the first visible row.
    app.home_offset.set(10);
    // Row 3 is the first data row (rows 0-2 are title/border/header).
    app.handle_mouse_click(0, 3);
    assert_eq!(app.home_selected, 10);

    app.handle_mouse_click(0, 5);
    assert_eq!(app.home_selected, 12);

    // With no scrolling, the same screen row means the first repository.
    app.home_offset.set(0);
    app.handle_mouse_click(0, 3);
    assert_eq!(app.home_selected, 0);
}

#[test]
fn mouse_click_is_ignored_while_a_prompt_or_confirm_is_open() {
    // A click landing "behind" a modal must not move a selection the user
    // can't currently see.
    let mut app = App::new();
    app.repos = vec![repo("a", "/tmp/a"), repo("b", "/tmp/b")];
    app.home_selected = 0;

    app.input = Some(InputKind::Filter);
    app.handle_mouse_click(0, 4);
    assert_eq!(app.home_selected, 0);
    app.input = None;

    app.confirm = Some(Confirm::DeleteRepo(0));
    app.handle_mouse_click(0, 4);
    assert_eq!(app.home_selected, 0);
}

#[test]
fn mouse_scroll_moves_the_selection_within_bounds_on_each_screen() {
    let mut app = App::new();
    app.repos = vec![
        repo("a", "/tmp/a"),
        repo("b", "/tmp/b"),
        repo("c", "/tmp/c"),
    ];
    app.screen = Screen::Home;
    app.home_selected = 0;

    app.handle_mouse_scroll(1);
    assert_eq!(app.home_selected, 1);
    app.handle_mouse_scroll(-1);
    assert_eq!(app.home_selected, 0);
    // Must clamp rather than underflow at the top...
    app.handle_mouse_scroll(-1);
    assert_eq!(app.home_selected, 0);
    // ...and at the bottom.
    for _ in 0..10 {
        app.handle_mouse_scroll(1);
    }
    assert_eq!(app.home_selected, 2);

    // A screen with no list must not panic on a scroll event.
    app.screen = Screen::Help;
    app.handle_mouse_scroll(1);
}

#[test]
fn mouse_tab_click_resets_the_list_cursor() {
    // Switching tabs by mouse used to leave a stale selection behind, so
    // Enter/d/space silently did nothing until the cursor was moved.
    let mut app = App::new();
    app.screen = Screen::Repo;
    app.repo_tab = RepoTab::Contributors;
    app.list_selected = 150;
    app.list_filter = "abc".to_string();
    crate::ui::tests::render(&app, 120, 24);

    app.handle_mouse_click(2, 1);

    assert_eq!(app.repo_tab, RepoTab::Status);
    assert_eq!(app.list_selected, 0);
    assert!(app.list_filter.is_empty());
}

#[test]
fn sanitize_path_input_undoes_shell_and_drag_and_drop_quoting() {
    // Dragging a folder into a terminal yields shell-escaped text; pasting a
    // path from elsewhere often arrives quoted. Both must round-trip to the
    // plain path, or "add repository" fails on any path with a space.
    assert_eq!(sanitize_path_input("  /work/repo  "), "/work/repo");
    assert_eq!(sanitize_path_input("'/work/my repo'"), "/work/my repo");
    assert_eq!(sanitize_path_input("\"/work/my repo\""), "/work/my repo");
    assert_eq!(sanitize_path_input(r"/work/my\ repo"), "/work/my repo");
    assert_eq!(sanitize_path_input(r"/work/repo\(1\)"), "/work/repo(1)");
    // A lone quote is not a quoted string and must be left alone.
    assert_eq!(sanitize_path_input("'/work/repo"), "'/work/repo");
    assert_eq!(sanitize_path_input(""), "");
}

#[test]
fn expand_user_path_resolves_tilde_against_home() {
    let home = home_dir().expect("HOME is set in the test environment");
    assert_eq!(expand_user_path("~"), home);
    assert_eq!(expand_user_path("~/work/repo"), home.join("work/repo"));
    // Only a leading `~/` (or a bare `~`) is special — `~foo` is a literal
    // path, not another user's home, since that isn't resolved here.
    assert_eq!(expand_user_path("~foo"), PathBuf::from("~foo"));
    assert_eq!(expand_user_path("/abs/path"), PathBuf::from("/abs/path"));
    // Quoting is stripped before expansion, so a dragged-in `~` path works.
    assert_eq!(expand_user_path("'~/work'"), home.join("work"));
}

#[test]
fn complete_path_lists_children_and_filters_by_prefix() {
    let base = std::env::temp_dir().join(format!("gdt-complete-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    for d in ["alpha", "alpine", "beta"] {
        std::fs::create_dir_all(base.join(d)).unwrap();
    }
    std::fs::write(base.join("afile.txt"), b"x").unwrap();

    let base_s = base.to_str().unwrap();

    // A directory path lists its children (directories only — completion is
    // for picking a repository folder).
    let all = complete_path(&format!("{base_s}/"));
    assert!(all.iter().any(|c| c.ends_with("alpha/")), "got {all:?}");
    assert!(all.iter().any(|c| c.ends_with("beta/")), "got {all:?}");
    assert!(
        !all.iter().any(|c| c.contains("afile.txt")),
        "files must not be offered: {all:?}"
    );

    // A partial final segment filters to matching entries.
    let alp = complete_path(&format!("{base_s}/alp"));
    assert_eq!(alp.len(), 2, "expected alpha+alpine, got {alp:?}");
    assert!(alp.iter().all(|c| c.contains("alp")));

    // A prefix matching nothing yields nothing rather than falling back to
    // the whole directory.
    assert!(complete_path(&format!("{base_s}/zzz")).is_empty());

    // A nonexistent directory is not an error, just no completions.
    assert!(complete_path("/definitely/not/here/xyz").is_empty());

    std::fs::remove_dir_all(&base).ok();
}

/// A spinner that never stops is worse than none: it says "still working"
/// about something that finished. Every path that starts one has to end it.
mod activity_lifecycle {
    use super::*;
    use crate::app::{Activity, Msg};

    fn app_with_repos(n: usize) -> App {
        let mut app = App::new();
        app.repos = (0..n)
            .map(|i| crate::config::Repository {
                name: format!("r{i}"),
                path: std::path::PathBuf::from(format!("/tmp/r{i}")),
                group: None,
            })
            .collect();
        app.busy.clear();
        app
    }

    /// A pull that succeeds immediately queues the row reload, so the marker
    /// turns over from `Pull` to `Refresh` rather than clearing. That is the
    /// point: the row really is still being updated, and blinking the spinner
    /// off between the two would misreport it as done.
    #[test]
    fn a_finished_pull_hands_over_to_the_row_reload() {
        let mut app = app_with_repos(2);
        app.busy.insert(1, Activity::Pull);
        app.apply_msg_for_test(Msg::OpDone {
            ok: true,
            text: "done".to_string(),
            repo_index: Some(1),
        });
        assert_eq!(app.activity(1), Some(Activity::Refresh));

        // ...and the reload finishing is what finally clears it.
        app.apply_msg_for_test(Msg::HomeLoaded {
            generation: u64::MAX,
            index: 1,
            row: Ok(HomeRow::default()),
        });
        assert_eq!(app.activity(1), None);
        assert_eq!(app.busy_count(), 0);
    }

    /// A pull that fails still finished. Reporting the error and leaving the
    /// row spinning would be the worst of both.
    #[test]
    fn a_failed_pull_also_clears_its_marker() {
        let mut app = app_with_repos(2);
        app.busy.insert(0, Activity::Pull);
        app.apply_msg_for_test(Msg::OpDone {
            ok: false,
            text: "remote unreachable".to_string(),
            repo_index: Some(0),
        });
        assert_eq!(app.activity(0), None);
        assert!(app.error.is_some());
    }

    /// A refresh superseded by a newer one is discarded on arrival — but the
    /// job it belonged to has still stopped running, so the marker must go.
    #[test]
    fn a_superseded_home_row_still_clears_its_marker() {
        let mut app = app_with_repos(2);
        app.busy.insert(0, Activity::Refresh);
        app.apply_msg_for_test(Msg::HomeLoaded {
            // Deliberately stale: `apply_msg` drops the row itself.
            generation: u64::MAX,
            index: 0,
            row: Ok(HomeRow::default()),
        });
        assert_eq!(app.activity(0), None, "stale result left the row spinning");
    }

    #[test]
    fn a_repository_that_failed_to_load_does_not_keep_spinning() {
        let mut app = app_with_repos(1);
        app.busy.insert(0, Activity::Refresh);
        app.apply_msg_for_test(Msg::HomeLoaded {
            generation: u64::MAX,
            index: 0,
            row: Err("not a git repository".to_string()),
        });
        assert_eq!(app.activity(0), None);
    }

    /// Markers are keyed by repository, so a duplicate completion is a no-op
    /// rather than something that could unbalance a counter.
    #[test]
    fn clearing_twice_is_harmless() {
        let mut app = app_with_repos(1);
        app.busy.insert(0, Activity::Fetch);
        for _ in 0..2 {
            app.apply_msg_for_test(Msg::OpDone {
                ok: false,
                text: "failed".to_string(),
                repo_index: Some(0),
            });
        }
        assert_eq!(app.busy_count(), 0);
    }

    #[test]
    fn one_repository_finishing_leaves_the_others_running() {
        let mut app = app_with_repos(3);
        for i in 0..3 {
            app.busy.insert(i, Activity::Fetch);
        }
        // Failed, so it does not hand over to a reload and simply stops.
        app.apply_msg_for_test(Msg::OpDone {
            ok: false,
            text: "remote unreachable".to_string(),
            repo_index: Some(1),
        });
        assert_eq!(app.activity(0), Some(Activity::Fetch));
        assert_eq!(app.activity(1), None);
        assert_eq!(app.activity(2), Some(Activity::Fetch));
        assert_eq!(app.busy_count(), 2);
    }
}

#[test]
fn first_registration_guide_dismisses_without_clearing_filter() {
    let mut app = App::new();
    app.prefs.onboarding_dismissed = false;
    app.screen = Screen::Home;
    app.error = None;
    app.home_filter = "alpha".into();
    app.show_onboarding(false);
    assert!(!app.onboarding_visible);
    app.show_onboarding(true);
    assert!(app.onboarding_visible);
    app.handle_key(KeyEvent::from(KeyCode::Esc));
    assert!(!app.onboarding_visible);
    assert!(app.prefs.onboarding_dismissed);
    assert_eq!(app.home_filter, "alpha");
    app.show_onboarding(true);
    assert!(!app.onboarding_visible);
}

#[test]
fn bulk_registration_starts_with_folder_choice_and_can_cancel() {
    let mut app = App::new();
    app.screen = Screen::Home;
    app.input = None;
    app.confirm = None;
    app.handle_key(KeyEvent::from(KeyCode::Char('A')));
    assert!(matches!(app.input, Some(InputKind::FinderScanPath)));
    assert_eq!(app.screen, Screen::Home);
    app.handle_key(KeyEvent::from(KeyCode::Esc));
    assert!(app.input.is_none());
    assert_eq!(app.screen, Screen::Home);
}

#[test]
fn failed_first_import_keeps_selection_and_does_not_show_success_guide() {
    let mut app = App::new();
    app.repos.clear();
    app.onboarding_visible = false;
    app.config_state.repos_failed = true;
    app.screen = Screen::RepoFinder;
    app.repo_finder = Some(RepoFinderState {
        loading: false,
        errors: vec![],
        scan_root: PathBuf::from("/tmp"),
        repos: vec![FoundRepo {
            path: PathBuf::from("/tmp/sample"),
            name: "sample".into(),
            branch: "main".into(),
            is_already_added: false,
            is_selected: true,
        }],
        selected_idx: 0,
        filter: String::new(),
    });
    app.import_finder_selected();
    assert!(app.repos.is_empty());
    assert!(app.error.is_some());
    assert!(app.repo_finder.as_ref().unwrap().repos[0].is_selected);
    assert_eq!(app.screen, Screen::RepoFinder);
    assert!(!app.onboarding_visible);
}

#[test]
fn finder_can_cancel_before_worker_responds_and_discards_late_results() {
    let mut app = App::new();
    let (tx, rx) = mpsc::channel();
    app.finder_tx = tx; // Hold the worker: cancellation must not depend on filesystem speed.
    app.open_repo_finder(Some(PathBuf::from("/slow-folder")));
    let seq = app.finder_generation.load(Ordering::Relaxed);
    assert!(matches!(rx.try_recv(), Ok(Job::ScanRepos { .. })));
    assert!(app.repo_finder.as_ref().unwrap().loading);
    app.handle_key(KeyEvent::from(KeyCode::Esc));
    assert_eq!(app.screen, Screen::Home);
    assert!(!app.repo_finder.as_ref().unwrap().loading);
    app.open_repo_finder(Some(PathBuf::from("/new-folder")));
    app.apply_msg_for_test(Msg::FinderRepo {
        seq,
        repo: FoundRepo {
            path: "/old".into(),
            name: "old".into(),
            branch: "main".into(),
            is_selected: true,
            is_already_added: false,
        },
    });
    app.apply_msg_for_test(Msg::FinderDone {
        seq,
        errors: vec!["old error".into()],
    });
    assert!(app.repo_finder.as_ref().unwrap().repos.is_empty());
    assert!(app.repo_finder.as_ref().unwrap().loading);
    app.handle_key(KeyEvent::from(KeyCode::Char('q')));
    assert_eq!(app.screen, Screen::Home);
    assert!(!app.should_quit);
}

#[test]
fn finder_completion_preserves_selection_when_discovery_order_differs() {
    let mut app = App::new();
    let (tx, _rx) = mpsc::channel();
    app.finder_tx = tx;
    app.repos.clear();
    app.open_repo_finder(Some("/scan".into()));
    let seq = app.finder_generation.load(Ordering::Relaxed);
    for name in ["z-last", "a-first"] {
        app.apply_msg_for_test(Msg::FinderRepo {
            seq,
            repo: FoundRepo {
                path: format!("/scan/{name}").into(),
                name: name.into(),
                branch: "main".into(),
                is_selected: true,
                is_already_added: false,
            },
        });
    }
    app.apply_msg_for_test(Msg::FinderDone {
        seq,
        errors: vec![],
    });
    let finder = app.repo_finder.as_ref().unwrap();
    assert_eq!(finder.repos[finder.selected_idx].name, "z-last");
    app.handle_key(KeyEvent::from(KeyCode::Char(' ')));
    let finder = app.repo_finder.as_ref().unwrap();
    assert!(finder.repos[0].is_selected);
    assert!(!finder.repos[1].is_selected);
}

#[test]
fn navigation_popup_keyboard_selects_a_destination_and_wheel_does_not_reach_screen() {
    let mut app = App::new();
    app.repos = vec![repo("one", "/one"), repo("two", "/two")];
    app.nav_popup = true;
    app.home_selected = 0;
    app.handle_mouse_scroll(1);
    assert_eq!(app.home_selected, 0);

    app.handle_key(KeyEvent::from(KeyCode::Down));
    app.handle_key(KeyEvent::from(KeyCode::Enter));
    assert_eq!(app.screen, Screen::Workspace);
    assert!(!app.nav_popup);
}
