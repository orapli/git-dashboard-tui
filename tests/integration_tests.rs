use git_dashboard_tui::config::Member;
use git_dashboard_tui::git::{
    DiffRowKind, GitOpState, apply_stash, drop_stash, find_git_repos, get_activity, get_branches,
    get_changed_files, get_file_blame, get_file_diff, get_recent_commits, get_stash_list,
    get_summary, get_tags, get_worktrees,
};
use std::fs::{self, File};
use std::io::Write;
use std::path::PathBuf;
use std::process::Command;

struct TempRepo {
    path: PathBuf,
}

impl TempRepo {
    fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "gitdash_int_test_{}_{}_{}",
            name,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();

        let repo = Self { path };
        repo.git(&["init", "-b", "main"]);
        repo.git(&["config", "user.name", "Test User"]);
        repo.git(&["config", "user.email", "test@example.com"]);
        repo.git(&["config", "commit.gpgSign", "false"]);
        // Windows runners default to core.autocrlf=true, so a file written
        // with "\n" comes back out of git with "\r\n" and byte-exact content
        // assertions fail for reasons that have nothing to do with the code
        // under test. Keep fixtures byte-identical on every platform.
        repo.git(&["config", "core.autocrlf", "false"]);
        repo
    }

    fn git(&self, args: &[&str]) -> String {
        let output = Command::new("git")
            .args(args)
            .current_dir(&self.path)
            .env("LANG", "C")
            .env("LC_ALL", "C")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .output()
            .unwrap_or_else(|e| panic!("Failed to run git {:?}: {}", args, e));

        assert!(
            output.status.success(),
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    }

    fn write_file(&self, rel_path: &str, content: &str) {
        let full_path = self.path.join(rel_path);
        if let Some(parent) = full_path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        let mut file = File::create(full_path).unwrap();
        file.write_all(content.as_bytes()).unwrap();
    }
}

impl Drop for TempRepo {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

#[test]
fn test_integration_git_summary_and_status() {
    let repo = TempRepo::new("summary");
    repo.write_file("README.md", "# Sample Repository\nHello world!\n");
    repo.write_file("src/main.rs", "fn main() {\n    println!(\"Hello\");\n}\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "Initial commit"]);

    let members = vec![Member {
        canonical_name: "Test User".to_string(),
        aliases: vec!["test@example.com".to_string()],
        is_active: true,
    }];

    let summary = get_summary(&repo.path, &members).expect("Summary should be retrieved");
    assert_eq!(summary.current_branch, "main");
    assert_eq!(summary.total_commits, 1);
    assert_eq!(summary.total_contributors, 1);
    assert_eq!(summary.total_files, 2);
    // Sizes come from git's own blob metadata rather than one stat per file
    assert!(summary.total_size_bytes > 0);
    assert!(!summary.total_size_formatted.is_empty());
    assert_eq!(summary.uncommitted_changes, 0);

    // Create an uncommitted change
    repo.write_file("new_file.txt", "uncommitted\n");
    let summary2 = get_summary(&repo.path, &members).expect("Summary should be retrieved");
    assert_eq!(summary2.uncommitted_changes, 1);
}

#[test]
fn test_integration_branches_and_tags() {
    let repo = TempRepo::new("branches_tags");
    repo.write_file("file.txt", "Initial\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "Initial commit on main"]);

    // Create a tag
    repo.git(&["tag", "-a", "v1.0.0", "-m", "Release v1.0.0"]);

    // Create feature branch
    repo.git(&["checkout", "-b", "feature/awesome"]);
    repo.write_file("feature.txt", "Feature code\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "Feature commit"]);

    let branches = get_branches(&repo.path).expect("Branches should be fetched");
    assert!(branches.len() >= 2);
    let branch_names: Vec<&str> = branches.iter().map(|b| b.name.as_str()).collect();
    assert!(branch_names.contains(&"main"));
    assert!(branch_names.contains(&"feature/awesome"));

    let tags = get_tags(&repo.path).expect("Tags should be fetched");
    assert_eq!(tags.len(), 1);
    assert_eq!(tags[0].name, "v1.0.0");
    assert!(tags[0].message.contains("Release v1.0.0"));
    assert!(!tags[0].hash.is_empty());
}

#[test]
fn test_integration_commit_log_and_graph() {
    let repo = TempRepo::new("commits");
    repo.write_file("README.md", "# Start\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "Initial commit"]);

    repo.git(&["tag", "v0.1.0"]);

    repo.write_file("README.md", "# Start\nUpdate 1\n");
    repo.git(&["commit", "-am", "Second commit"]);

    let commits = get_recent_commits(&repo.path).expect("Recent commits should be fetched");
    assert_eq!(commits.len(), 2);
    assert_eq!(commits[0].message, "Second commit");
    assert_eq!(commits[1].message, "Initial commit");
    assert_eq!(commits[0].author, "Test User");

    // Tag ref should be attached to second commit
    assert!(
        commits[1]
            .refs
            .iter()
            .any(|r| r.name == "v0.1.0" && r.is_tag)
    );
}

#[test]
fn test_integration_diverged_history_graph_carries_ansi_lane_colors() {
    // Every git invocation in this codebase sets `-c color.ui=never`
    // (git::GIT_CONFIG_ARGS) precisely so structured output never has stray
    // ANSI codes to corrupt parsing — this checks that get_recent_commits'
    // own `--color=always` still overrides that for the one field, through
    // the real exec layer, not just in isolation.
    let repo = TempRepo::new("diverged_graph");
    repo.write_file("f.txt", "base\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "base"]);

    repo.git(&["checkout", "-b", "feature"]);
    repo.write_file("f.txt", "feature\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "feature commit"]);
    repo.git(&["checkout", "main"]);
    // main must also move past the shared base, or this is just a straight
    // line (feature-tip -> base) with no second lane to color at all.
    repo.write_file("g.txt", "main-only\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "main commit"]);

    let commits = get_recent_commits(&repo.path).expect("commits should be fetched");
    assert_eq!(commits.len(), 3);
    // Two diverged lanes: git colors the connector reaching the second
    // lane's tip. If this ever stops appearing, --color=always silently
    // stopped overriding the `-c color.ui=never` every invocation sets.
    assert!(
        commits.iter().any(|c| c.graph.contains('\u{1b}')),
        "expected an ANSI-colored connector in a two-lane graph, got {:?}",
        commits.iter().map(|c| &c.graph).collect::<Vec<_>>()
    );
    // The important thing beyond that: no ANSI code ever reaches the hash,
    // author, date, or message fields, which parsing depends on being plain.
    for c in &commits {
        assert!(!c.hash.contains('\u{1b}'));
        assert!(!c.author.contains('\u{1b}'));
        assert!(!c.date.contains('\u{1b}'));
        assert!(!c.message.contains('\u{1b}'));
    }
}

#[test]
fn test_integration_diff_and_changed_files() {
    let repo = TempRepo::new("diff");
    repo.write_file("file.txt", "Line 1\nLine 2\nLine 3\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "Base commit"]);

    // Modify file.txt and create a new file
    repo.write_file("file.txt", "Line 1\nLine 2 (modified)\nLine 3\nLine 4\n");
    repo.write_file("added.rs", "pub fn added() {}\n");
    repo.git(&["add", "added.rs"]);

    let changed = get_changed_files(&repo.path, None, "WORKING_TREE", false)
        .expect("Changed files should be detected");
    assert_eq!(changed.len(), 2);

    let diff = get_file_diff(
        &repo.path,
        None,
        "WORKING_TREE",
        "file.txt",
        false,
        false,
        false,
    )
    .expect("File diff should be computed");
    assert!(!diff.is_binary);
    assert!(!diff.rows.is_empty());

    // Should have modified or added/removed rows
    let has_modified_or_change = diff.rows.iter().any(|r| {
        r.kind == DiffRowKind::Modified
            || r.kind == DiffRowKind::Added
            || r.kind == DiffRowKind::Removed
    });
    assert!(has_modified_or_change);
}

#[test]
fn test_integration_blame_and_activity() {
    let repo = TempRepo::new("blame_activity");
    repo.write_file("hello.txt", "First line\nSecond line\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "First commit"]);

    let blame = get_file_blame(&repo.path, "HEAD", "hello.txt").expect("Blame should work");
    assert_eq!(blame.len(), 2);
    assert_eq!(blame[0].author, "Test User");
    assert_eq!(blame[0].summary, "First commit");

    let activity = get_activity(&repo.path, 30).expect("Activity should work");
    assert_eq!(activity.daily.counts.iter().sum::<usize>(), 1);
}

#[test]
fn test_integration_stash_workflow() {
    let repo = TempRepo::new("stash");
    repo.write_file("file.txt", "Base\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "Base"]);

    repo.write_file("file.txt", "Modified before stash\n");
    repo.git(&["stash", "push", "-m", "WIP on feature"]);

    let stashes = get_stash_list(&repo.path).expect("Stashes should be listed");
    assert_eq!(stashes.len(), 1);
    assert!(stashes[0].message.contains("WIP on feature"));

    // Apply stash
    apply_stash(&repo.path, "stash@{0}").expect("Apply stash should succeed");
    let content = fs::read_to_string(repo.path.join("file.txt")).unwrap();
    assert_eq!(content, "Modified before stash\n");

    // Drop stash
    drop_stash(&repo.path, "stash@{0}").expect("Drop stash should succeed");
    let stashes_after = get_stash_list(&repo.path).expect("Stashes should be listed");
    assert_eq!(stashes_after.len(), 0);
}

#[test]
fn test_integration_repo_finder_and_batch_scan() {
    let scan_root = std::env::temp_dir().join(format!(
        "gitdash_scan_root_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let _ = fs::remove_dir_all(&scan_root);
    fs::create_dir_all(&scan_root).unwrap();

    let sub1 = scan_root.join("project_a");
    let sub2 = scan_root.join("nested").join("project_b");
    let non_repo = scan_root.join("docs");

    fs::create_dir_all(&sub1).unwrap();
    fs::create_dir_all(&sub2).unwrap();
    fs::create_dir_all(&non_repo).unwrap();

    Command::new("git")
        .args(["init", "-b", "main"])
        .current_dir(&sub1)
        .output()
        .unwrap();

    Command::new("git")
        .args(["init", "-b", "main"])
        .current_dir(&sub2)
        .output()
        .unwrap();

    let found = find_git_repos(&scan_root, 4);
    assert_eq!(found.len(), 2);
    assert!(found.contains(&sub1));
    assert!(found.contains(&sub2));

    let _ = fs::remove_dir_all(&scan_root);
}

#[test]
fn test_integration_worktrees_workflow() {
    let repo = TempRepo::new("worktree");
    repo.write_file("main_file.txt", "Initial\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "Initial on main"]);

    // Create a worktree pointing to a new branch
    let wt_path = repo.path.join("wt_feature");
    repo.git(&[
        "worktree",
        "add",
        "-b",
        "feature-wt",
        wt_path.to_str().unwrap(),
    ]);

    let worktrees = get_worktrees(&repo.path).expect("Worktrees should be retrieved");
    assert_eq!(worktrees.len(), 2);
    assert!(
        worktrees
            .iter()
            .any(|w| w.branch.as_deref() == Some("main"))
    );
    assert!(
        worktrees
            .iter()
            .any(|w| w.branch.as_deref() == Some("feature-wt"))
    );
}

#[test]
fn test_integration_root_commit_diff() {
    // The first commit has no parent, so `<target>^..<target>` is unresolvable
    // and used to render an empty file list with no explanation.
    let repo = TempRepo::new("rootcommit");
    repo.write_file("first.txt", "line one\nline two\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "Root commit"]);
    let root = repo.git(&["rev-parse", "HEAD"]);

    let changed =
        get_changed_files(&repo.path, None, &root, false).expect("root commit files listed");
    assert_eq!(changed.len(), 1, "root commit should list its added file");
    assert_eq!(changed[0].path, "first.txt");

    let diff = get_file_diff(&repo.path, None, &root, "first.txt", false, false, false)
        .expect("root commit diff computed");
    assert!(!diff.rows.is_empty(), "root commit diff should have rows");
    assert!(diff.rows.iter().any(|r| r.kind == DiffRowKind::Added));
}

#[test]
fn test_integration_merge_conflict_is_reported() {
    let repo = TempRepo::new("mergeconflict");
    repo.write_file("file.txt", "base\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "base commit"]);

    repo.git(&["checkout", "-b", "feature"]);
    repo.write_file("file.txt", "feature change\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "feature commit"]);

    repo.git(&["checkout", "main"]);
    repo.write_file("file.txt", "main change\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "main commit"]);

    let members: Vec<Member> = vec![];
    let clean = get_summary(&repo.path, &members).expect("summary before merge");
    assert_eq!(clean.op_state, GitOpState::None);
    assert_eq!(clean.conflicts, 0);

    // `git merge` exits non-zero on conflict — allowed to fail here, unlike
    // the TempRepo::git helper's other calls, which assert success.
    let status = Command::new("git")
        .args(["merge", "feature"])
        .current_dir(&repo.path)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .status()
        .unwrap();
    assert!(!status.success(), "merge should conflict");

    let conflicted = get_summary(&repo.path, &members).expect("summary during conflict");
    assert_eq!(conflicted.op_state, GitOpState::Merge);
    assert_eq!(conflicted.conflicts, 1);
}

#[test]
fn test_integration_rebase_in_progress_is_reported() {
    let repo = TempRepo::new("rebaseinprogress");
    repo.write_file("file.txt", "base\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "base commit"]);

    repo.git(&["checkout", "-b", "feature"]);
    repo.write_file("file.txt", "feature change\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "feature commit"]);

    repo.git(&["checkout", "main"]);
    repo.write_file("file.txt", "main change\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "main commit"]);

    let status = Command::new("git")
        .args(["rebase", "main", "feature"])
        .current_dir(&repo.path)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .status()
        .unwrap();
    assert!(!status.success(), "rebase should conflict and pause");

    let members: Vec<Member> = vec![];
    let summary = get_summary(&repo.path, &members).expect("summary during rebase");
    assert_eq!(summary.op_state, GitOpState::Rebase);
    assert_eq!(summary.conflicts, 1);
}

#[test]
fn test_integration_rebase_state_clears_after_continue() {
    // git leaves the REBASE_HEAD *ref file* behind after `rebase --continue`
    // completes successfully — only .git/rebase-merge (which this fix checks
    // instead) is actually removed. A summary taken after completion must
    // not still show ⚠REBASE.
    let repo = TempRepo::new("rebase_clears_after_continue");
    repo.write_file("file.txt", "base\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "base commit"]);

    repo.git(&["checkout", "-b", "feature"]);
    repo.write_file("file.txt", "feature change\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "feature commit"]);

    repo.git(&["checkout", "main"]);
    repo.write_file("file.txt", "main change\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "main commit"]);

    let status = Command::new("git")
        .args(["rebase", "main", "feature"])
        .current_dir(&repo.path)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .status()
        .unwrap();
    assert!(!status.success(), "rebase should conflict and pause");

    repo.write_file("file.txt", "feature change\n");
    repo.git(&["add", "file.txt"]);
    let status = Command::new("git")
        .args(["rebase", "--continue"])
        .current_dir(&repo.path)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_EDITOR", "true")
        .status()
        .unwrap();
    assert!(
        status.success(),
        "rebase --continue should complete cleanly"
    );

    let members: Vec<Member> = vec![];
    let summary = get_summary(&repo.path, &members).expect("summary after rebase");
    assert_eq!(summary.op_state, GitOpState::None);
    assert_eq!(summary.conflicts, 0);
}

#[test]
fn test_integration_stash_ops_reject_option_like_refs() {
    // Stash refs come from git's own output, but they are passed straight
    // back to git as an argument. check_safe_ref is what stops a ref that
    // looks like a flag from being read as one — worth proving through the
    // public API, not just at the validator's own unit level.
    let repo = TempRepo::new("stash_unsafe_ref");
    repo.write_file("file.txt", "Base\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "Base"]);
    repo.write_file("file.txt", "Modified\n");
    repo.git(&["stash", "push", "-m", "WIP"]);

    for bad in ["--all", "-x", "stash@{0}\nrm -rf /"] {
        assert!(
            apply_stash(&repo.path, bad).is_err(),
            "apply_stash must reject {bad:?}"
        );
        assert!(
            drop_stash(&repo.path, bad).is_err(),
            "drop_stash must reject {bad:?}"
        );
    }

    // The real stash is untouched by the rejected calls.
    let stashes = get_stash_list(&repo.path).expect("stashes listed");
    assert_eq!(stashes.len(), 1);
}

#[test]
fn test_integration_pull_fetch_report_missing_remote_and_upstream() {
    use git_dashboard_tui::config::Language;
    use git_dashboard_tui::git::{fetch_repository, pull_repository};

    let repo = TempRepo::new("no_remote");
    repo.write_file("file.txt", "Base\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "Base"]);

    // No remote at all: both must fail fast with an explanation rather than
    // shelling out to git and surfacing its raw error.
    let err = pull_repository(&repo.path, Language::English).unwrap_err();
    assert!(err.contains("No remote"), "unexpected pull error: {err}");
    let err = fetch_repository(&repo.path, Language::English).unwrap_err();
    assert!(err.contains("No remote"), "unexpected fetch error: {err}");

    // A remote that exists but no upstream for this branch is a different,
    // more specific failure — and must not be reported as "no remote".
    repo.git(&["remote", "add", "origin", "https://example.invalid/x.git"]);
    let err = pull_repository(&repo.path, Language::English).unwrap_err();
    assert!(err.contains("upstream"), "unexpected pull error: {err}");
}

/// A registered repository is not trusted, and its *content* reaches the
/// screen: file text, commit messages, author names. ratatui writes strings to
/// the terminal as given, so an escape sequence committed to a file would be
/// executed by the terminal — clearing the display, repainting it, or on
/// terminals that support OSC 52, writing the system clipboard.
#[test]
fn test_integration_repository_content_cannot_drive_the_terminal() {
    let repo = TempRepo::new("escape_injection");
    repo.write_file("payload.txt", "harmless first line\n");
    repo.git(&["add", "-A"]);
    repo.git(&["commit", "-m", "baseline"]);

    repo.write_file(
        "payload.txt",
        "harmless first line\n\
         clear:\x1b[2J\n\
         colour:\x1b[31mRED\x1b[0m\n\
         clipboard:\x1b]52;c;cGF5bG9hZA==\x07\n\
         cursor:\x1b[10;10Hmoved\n\
         carriage:before\rafter\n",
    );
    repo.git(&["add", "-A"]);
    repo.git(&["commit", "-m", "content with terminal escapes"]);

    let diff = git_dashboard_tui::git::get_file_diff(
        &repo.path,
        None,
        "HEAD",
        "payload.txt",
        false,
        false,
        false,
    )
    .expect("diff should load");

    let text: String = diff
        .rows
        .iter()
        .filter_map(|r| r.right_text.clone().or_else(|| r.left_text.clone()))
        .collect::<Vec<_>>()
        .join("\n");

    assert!(
        !text.contains('\u{1b}'),
        "an escape survived into displayed text: {text:?}"
    );
    assert!(!text.contains('\r'), "a carriage return survived: {text:?}");
    // The clipboard payload must not merely lose its introducer and show up
    // as text — the whole sequence goes.
    assert!(
        !text.contains("cGF5bG9hZA=="),
        "OSC payload leaked as text: {text:?}"
    );
    // The readable parts of each line are still there: this sanitises, it
    // does not blank the file out.
    for kept in ["harmless first line", "RED", "moved", "before", "after"] {
        assert!(text.contains(kept), "{kept:?} was lost: {text:?}");
    }
}

/// Go, Make and C indent with tabs. ratatui writes a tab to the terminal
/// verbatim and the terminal jumps to its own next tab stop, which the layout
/// knows nothing about — so the diff pane drew over the one beside it.
#[test]
fn test_integration_tab_indented_files_are_expanded_for_display() {
    let repo = TempRepo::new("tab_expansion");
    repo.write_file("main.go", "package main\n\nfunc main() {\n}\n");
    repo.git(&["add", "-A"]);
    repo.git(&["commit", "-m", "baseline"]);

    repo.write_file(
        "main.go",
        "package main\n\nfunc main() {\n\tx := 1\n\t\ty := 2\n\t_ = x + y\n}\n",
    );
    repo.git(&["add", "-A"]);
    repo.git(&["commit", "-m", "tab indented body"]);

    let diff = git_dashboard_tui::git::get_file_diff(
        &repo.path, None, "HEAD", "main.go", false, false, false,
    )
    .expect("diff should load");

    let lines: Vec<String> = diff
        .rows
        .iter()
        .filter_map(|r| r.right_text.clone())
        .collect();
    assert!(
        lines.iter().all(|l| !l.contains('\t')),
        "a tab reached the display layer: {lines:?}"
    );
    assert!(
        lines.iter().any(|l| l.starts_with("    x := 1")),
        "one tab should indent to column 4: {lines:?}"
    );
    assert!(
        lines.iter().any(|l| l.starts_with("        y := 2")),
        "two tabs should indent to column 8: {lines:?}"
    );
}

#[test]
fn config_override_and_external_work_refresh_roundtrip() {
    // A child process isolates the environment override from concurrent tests.
    if std::env::var_os("GDT_REFRESH_TEST_CHILD").is_none() {
        let root = TempRepo::new("external-refresh-config");
        let output = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "config_override_and_external_work_refresh_roundtrip",
                "--nocapture",
            ])
            .env("GDT_REFRESH_TEST_CHILD", "1")
            .env(
                "GIT_DASHBOARD_CONFIG_DIR",
                root.path.join("new/nested/config"),
            )
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    }
    use crossterm::event::{KeyCode, KeyEvent};
    use git_dashboard_tui::{
        app::{App, Screen},
        config,
    };
    fn settle(app: &mut App) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
        loop {
            app.drain_messages();
            if !app.is_busy() {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "background work did not finish: {:?}",
                app.error
            );
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(app.error.is_none(), "{:?}", app.error);
    }
    let config_dir = config::get_config_dir();
    assert!(!config_dir.exists());
    let repo = TempRepo::new("external-work-refresh");
    let original: String = (0..60).map(|i| format!("line {i}\n")).collect();
    repo.write_file("old.txt", &original);
    repo.write_file("aaa-new.txt", "base\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-qm", "initial"]);
    repo.write_file("old.txt", &(original.clone() + "edited\n"));
    #[cfg(unix)]
    let registered = {
        let alias = repo.path.with_extension("alias");
        std::os::unix::fs::symlink(&repo.path, &alias).unwrap();
        alias
    };
    #[cfg(not(unix))]
    let registered = repo.path.join("..").join(repo.path.file_name().unwrap());
    let entries = vec![config::Repository {
        name: "sample".into(),
        path: registered.clone(),
        group: None,
    }];
    config::save_repositories(&entries).unwrap();
    assert_eq!(config::load_repositories().unwrap(), entries);
    config::save_preferences(&config::Preferences {
        diff_full_file: true,
        ..Default::default()
    })
    .unwrap();
    let mut app = App::new();
    settle(&mut app);
    app.handle_key(KeyEvent::from(KeyCode::Enter));
    settle(&mut app);
    app.handle_key(KeyEvent::from(KeyCode::Char('1')));
    app.handle_key(KeyEvent::from(KeyCode::Enter));
    settle(&mut app);
    assert_eq!(app.screen, Screen::Diff);
    app.diff.as_mut().unwrap().scroll = 5;
    repo.write_file("aaa-new.txt", "new file\n");
    app.after_external_work(&repo.path.canonicalize().unwrap());
    settle(&mut app);
    let diff = app.diff.as_ref().unwrap();
    assert_eq!(diff.files.len(), 2);
    assert_eq!(diff.files[diff.file_idx].path, "old.txt");
    assert_eq!(diff.scroll, 5);
    assert_eq!(app.home_rows.get(&0).unwrap().dirty, 2);
    repo.write_file("old.txt", &original);
    app.after_external_work(&repo.path.canonicalize().unwrap());
    settle(&mut app);
    let diff = app.diff.as_ref().unwrap();
    assert_eq!(diff.files.len(), 1);
    assert_eq!(diff.files[diff.file_idx].path, "aaa-new.txt");
    assert_eq!(diff.scroll, 0);
    repo.write_file("aaa-new.txt", "base\n");
    app.after_external_work(&repo.path.canonicalize().unwrap());
    settle(&mut app);
    let diff = app.diff.as_ref().unwrap();
    assert!(diff.files.is_empty());
    assert!(diff.lines.is_empty());
    assert!(diff.blame.is_none());
    assert!(!diff.loading);
    assert_eq!(app.home_rows.get(&0).unwrap().dirty, 0);
    #[cfg(unix)]
    fs::remove_file(registered).unwrap();
}

#[test]
fn untracked_and_unborn_work_is_visible_without_changing_the_index() {
    use git_dashboard_tui::git::WORKING_TREE;
    let repo = TempRepo::new("untracked-preview");
    repo.write_file("new file.txt", "hello\n\tworld\n");
    repo.write_file("new-directory/one.txt", "one\n");
    repo.write_file("new-directory/two.txt", "two\n");
    repo.write_file("ignored.tmp", "private\n");
    repo.write_file(".gitignore", "*.tmp\n");
    repo.git(&["add", ".gitignore"]);
    let before = repo.git(&["status", "--porcelain"]);
    for committed in [false, true] {
        if committed {
            repo.git(&["commit", "-qm", "ignore rule"]);
        }
        let files = get_changed_files(&repo.path, None, WORKING_TREE, false).unwrap();
        assert_eq!(
            get_summary(&repo.path, &[]).unwrap().uncommitted_changes,
            files.len()
        );
        assert!(
            files
                .iter()
                .any(|f| f.path == "new file.txt" && f.status == "?")
        );
        assert!(!files.iter().any(|f| f.path == "ignored.tmp"));
        let diff = get_file_diff(
            &repo.path,
            None,
            WORKING_TREE,
            "new file.txt",
            false,
            false,
            false,
        )
        .unwrap();
        assert!(
            diff.rows
                .iter()
                .any(|r| r.right_text.as_deref() == Some("hello"))
        );
        assert!(
            diff.rows
                .iter()
                .any(|r| r.kind == DiffRowKind::Added && r.right_text.as_deref() == Some("hello"))
        );
        if !committed {
            assert_eq!(repo.git(&["status", "--porcelain"]), before);
        }
    }
    repo.write_file("binary.bin", "a\0b");
    assert!(
        get_file_diff(
            &repo.path,
            None,
            WORKING_TREE,
            "binary.bin",
            false,
            false,
            false
        )
        .unwrap()
        .is_binary
    );
    repo.write_file("empty.txt", "");
    assert!(
        get_changed_files(&repo.path, None, WORKING_TREE, false)
            .unwrap()
            .iter()
            .any(|f| f.path == "empty.txt")
    );
    assert!(
        get_file_diff(
            &repo.path,
            None,
            WORKING_TREE,
            "empty.txt",
            false,
            false,
            false
        )
        .unwrap()
        .rows
        .iter()
        .all(|r| r.kind != DiffRowKind::Added)
    );
    assert!(get_changed_files(&repo.path.join("missing"), None, WORKING_TREE, false).is_err());
}
