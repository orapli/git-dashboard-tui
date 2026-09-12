use super::*;
use crate::config::Repository;
use crate::git::{BranchInfo, ChangedFile, CommitSummary, GitOpState, Summary};
use crossterm::event::KeyCode;

fn repo(name: &str, path: &str) -> Repository {
    Repository {
        name: name.to_string(),
        path: PathBuf::from(path),
        group: None,
    }
}

/// A real directory, so `local_tool_ready` (which insists on one) passes.
fn temp_repo_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("gdt-tools-{}-{tag}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn snapshot() -> RepoSnapshot {
    RepoSnapshot {
        summary: Summary {
            current_branch: "feature/x".into(),
            ..Summary::default()
        },
        commits: vec![CommitSummary {
            hash: "abc1234def".into(),
            author: "a".into(),
            date: "2026-01-01".into(),
            message: "m".into(),
            graph: String::new(),
            refs: vec![],
        }],
        commits_err: None,
        branches: vec![BranchInfo {
            name: "release/1.2".into(),
            is_remote: false,
            author: "a".into(),
            date: "2026-01-01".into(),
            date_unix: 0,
            message: "m".into(),
        }],
        branches_err: None,
        tags: vec![],
        tags_err: None,
        stashes: vec![],
        stashes_err: None,
        working_files: vec![ChangedFile {
            status: "M".into(),
            path: "src/main.rs".into(),
            old_path: None,
            additions: 1,
            deletions: 0,
        }],
        working_err: None,
        contributors: vec![],
        contributors_err: None,
        worktrees: vec![],
        worktrees_err: None,
    }
}

/// An app with one repository open on the Repo screen, its snapshot loaded.
fn app_on_repo(dir: &Path) -> App {
    let mut app = App::new();
    app.prefs.custom_commands.clear();
    app.repos = vec![repo("alpha", &dir.display().to_string())];
    app.repo_index = Some(0);
    app.repo_data = Some(snapshot());
    app.screen = Screen::Repo;
    app.repo_tab = RepoTab::Commits;
    app.list_selected = 0;
    app
}

// ---- ⑫ placeholders ------------------------------------------------------

#[test]
fn editor_preserves_spaces_and_passes_paths_as_arguments() {
    let path = Path::new("/tmp/repo with spaces");
    let c = editor_command("code --wait --goto '{path}'", path).unwrap();
    assert_eq!(c.args, vec!["--wait", "--goto", "/tmp/repo with spaces"]);
    let c = editor_command("'/opt/editor tools/editor' --wait", path).unwrap();
    assert_eq!(c.program, "/opt/editor tools/editor");
    assert_eq!(c.args, vec!["--wait", "/tmp/repo with spaces"]);
    assert!(editor_command("'unclosed", path).is_err());
    assert!(crate::resolve_program("./repo-tool").is_err());
}

#[test]
fn the_diff_view_supplies_file_line_branch_and_hash() {
    let dir = temp_repo_dir("ctx");
    let mut app = app_on_repo(&dir);
    app.screen = Screen::Diff;
    app.diff = Some(DiffView {
        title: "t".into(),
        target: "abc1234def".into(),
        base: None,
        three_dot: false,
        files: vec![ChangedFile {
            status: "M".into(),
            path: "src/main.rs".into(),
            old_path: None,
            additions: 1,
            deletions: 0,
        }],
        file_idx: 0,
        lines: vec![
            // A hunk header has no line number: the context must report the
            // first numbered line at or below the scroll position.
            DiffLine::default(),
            DiffLine {
                new_no: Some(120),
                ..DiffLine::default()
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

    let ctx = app.tool_context(&dir);
    assert_eq!(ctx.repo, dir);
    assert_eq!(ctx.file, Some(dir.join("src/main.rs")));
    assert_eq!(ctx.line, Some(120));
    assert_eq!(ctx.branch.as_deref(), Some("feature/x"));
    assert_eq!(ctx.hash.as_deref(), Some("abc1234def"));

    // The working-tree sentinel is not a revision.
    if let Some(diff) = app.diff.as_mut() {
        diff.target = "WORKING_TREE".into();
    }
    assert_eq!(app.tool_context(&dir).hash, None);
}

#[test]
fn the_repo_screen_supplies_what_the_selected_tab_is_about() {
    let dir = temp_repo_dir("ctx2");
    let mut app = app_on_repo(&dir);
    assert_eq!(
        app.tool_context(&dir).hash.as_deref(),
        Some("abc1234def"),
        "Commits tab selects a commit"
    );

    app.repo_tab = RepoTab::Branches;
    assert_eq!(
        app.tool_context(&dir).branch.as_deref(),
        Some("release/1.2")
    );

    app.repo_tab = RepoTab::Status;
    assert_eq!(
        app.tool_context(&dir).file,
        Some(dir.join("src/main.rs")),
        "Status tab selects a file"
    );

    // A repository that is not the one open supplies no file or commit: the
    // diff on screen belongs to a different repository.
    let other = PathBuf::from("/tmp/some-other-repo");
    let ctx = app.tool_context(&other);
    assert_eq!(ctx.repo, other);
    assert_eq!(ctx.file, None);
    assert_eq!(ctx.hash, None);
}

#[test]
fn a_custom_command_launches_with_the_context_substituted() {
    let dir = temp_repo_dir("launch");
    let mut app = app_on_repo(&dir);
    app.prefs.custom_commands = vec![CustomCommand {
        label: "show".into(),
        command: "sh -c 'git show {hash}' {branch}".into(),
        wait: true,
    }];
    app.open_tool_menu(dir.clone());
    app.handle_key(KeyEvent::from(KeyCode::Char('1')));

    let tool = app.take_work_tool().expect("custom command queued");
    assert!(tool.wait);
    assert!(tool.command.program.ends_with("sh"));
    assert_eq!(
        tool.command.args,
        vec!["-c", "git show abc1234def", "feature/x"]
    );
    assert_eq!(tool.command.cwd, dir);
    assert!(app.tool_menu.is_none(), "menu closes once the tool runs");
    assert!(app.error.is_none());
}

#[test]
fn a_placeholder_without_a_value_refuses_and_keeps_the_menu_open() {
    let dir = temp_repo_dir("refuse");
    let mut app = App::new();
    app.prefs.custom_commands = vec![CustomCommand {
        label: "open file".into(),
        command: "sh {file}".into(),
        wait: false,
    }];
    app.repos = vec![repo("alpha", &dir.display().to_string())];
    app.open_tool_menu(dir.clone());
    app.handle_key(KeyEvent::from(KeyCode::Char('1')));

    assert!(app.take_work_tool().is_none(), "nothing may be launched");
    let error = app.error.clone().expect("the refusal is explained");
    assert!(error.contains("{file}"), "{error}");
    assert!(
        app.tool_menu.is_some(),
        "the menu stays open so the user can pick something else"
    );
}

#[test]
fn the_menu_documents_the_placeholders_in_both_languages() {
    let dir = temp_repo_dir("menu");
    let mut app = App::new();
    app.prefs.custom_commands = vec![CustomCommand {
        label: "jj".into(),
        command: "definitely-not-installed-9f3c log".into(),
        wait: false,
    }];
    app.open_tool_menu(dir.clone());

    let en = app.tool_menu_lines().join("\n");
    for placeholder in PLACEHOLDERS {
        assert!(en.contains(placeholder), "{placeholder} missing from {en}");
    }
    assert!(en.contains("1  jj: definitely-not-installed-9f3c log"));
    assert!(en.contains("not installed"), "{en}");

    app.set_language_for_test(crate::config::Language::Japanese);
    let ja = app.tool_menu_lines().join("\n");
    for placeholder in PLACEHOLDERS {
        assert!(ja.contains(placeholder));
    }
    assert!(ja.contains("プレースホルダ"), "{ja}");
    assert!(ja.contains("未導入"), "{ja}");
}

#[test]
fn tools_menu_closes_without_changing_the_screen_and_rejects_ssh() {
    let mut a = App::new();
    a.open_tool_menu("ssh://host/work/repo".into());
    a.handle_key(KeyEvent::from(KeyCode::Char('l')));
    assert!(a.take_work_tool().is_none());
    assert!(a.error.is_some());
    a.handle_key(KeyEvent::from(KeyCode::Esc));
    assert!(a.tool_menu.is_none());
    assert_eq!(a.screen, Screen::Home);
}

// ---- ⑩ landing tab -------------------------------------------------------

#[test]
fn opening_a_repository_lands_where_the_reason_for_opening_it_is_visible() {
    let cases = [
        (HomeRow::default(), RepoTab::Commits),
        (
            HomeRow {
                dirty: 7,
                ..HomeRow::default()
            },
            RepoTab::Status,
        ),
        (
            HomeRow {
                conflicts: 1,
                ..HomeRow::default()
            },
            RepoTab::Status,
        ),
        (
            HomeRow {
                op_state: GitOpState::Rebase,
                ..HomeRow::default()
            },
            RepoTab::Status,
        ),
    ];
    for (row, expected) in cases {
        let mut app = App::new();
        app.repos = vec![repo("alpha", "/tmp/gdt-landing")];
        app.home_rows.insert(0, row.clone());
        app.open_repo(0);
        assert_eq!(app.screen, Screen::Repo);
        assert_eq!(app.repo_tab, expected, "row {row:?}");
    }

    // Nothing known about the repository yet: the predictable default.
    assert_eq!(App::landing_tab(None, None), RepoTab::Commits);
    // A cached snapshot is enough on its own.
    assert_eq!(
        App::landing_tab(None, Some(&snapshot())),
        RepoTab::Status,
        "the cached snapshot has a modified file"
    );
}

#[test]
fn a_later_load_does_not_move_the_tab_under_the_user() {
    let mut app = App::new();
    app.repos = vec![repo("alpha", "/tmp/gdt-landing2")];
    app.home_rows.insert(
        0,
        HomeRow {
            dirty: 2,
            ..HomeRow::default()
        },
    );
    app.open_repo(0);
    assert_eq!(app.repo_tab, RepoTab::Status);
    app.handle_key(KeyEvent::from(KeyCode::Char('2')));
    assert_eq!(app.repo_tab, RepoTab::Commits);
    // The repository finishing its load must not reach back into the tab.
    app.apply_msg_for_test(Msg::RepoLoaded {
        index: 0,
        data: Box::new(Ok(snapshot())),
    });
    assert_eq!(app.repo_tab, RepoTab::Commits);
}

// ---- ⑬ custom commands ---------------------------------------------------

#[test]
fn custom_commands_are_added_edited_and_deleted_from_the_editor() {
    let mut app = App::new();
    app.prefs.custom_commands.clear();
    app.screen = Screen::Settings;
    app.handle_key(KeyEvent::from(KeyCode::Char('x')));
    assert!(app.tool_editor.is_some());

    let type_in = |app: &mut App, text: &str| {
        for c in text.chars() {
            app.handle_key(KeyEvent::from(KeyCode::Char(c)));
        }
        app.handle_key(KeyEvent::from(KeyCode::Enter));
    };

    app.handle_key(KeyEvent::from(KeyCode::Char('a')));
    type_in(&mut app, "Review");
    type_in(&mut app, "sh -c 'review {file}'");
    assert_eq!(
        app.custom_commands(),
        [CustomCommand {
            label: "Review".into(),
            command: "sh -c 'review {file}'".into(),
            wait: false,
        }]
    );
    assert!(app.error.is_none());

    // `w` toggles waiting, exactly as the editor preference does.
    app.handle_key(KeyEvent::from(KeyCode::Char('w')));
    assert!(app.custom_commands()[0].wait);

    // Editing prefills both fields with what is already there — the point of
    // "edit" rather than "delete and re-add" — and keeps position and wait.
    app.handle_key(KeyEvent::from(KeyCode::Char('e')));
    type_in(&mut app, "!");
    for _ in 0.."sh -c 'review {file}'".len() {
        app.handle_key(KeyEvent::from(KeyCode::Backspace));
    }
    type_in(&mut app, "sh -c 'review2'");
    assert_eq!(app.custom_commands().len(), 1);
    assert_eq!(app.custom_commands()[0].label, "Review!");
    assert_eq!(app.custom_commands()[0].command, "sh -c 'review2'");
    assert!(app.custom_commands()[0].wait);

    // What was saved survives a reload of prefs.json.
    let reloaded = crate::config::load_preferences().unwrap();
    assert_eq!(reloaded.custom_commands, app.custom_commands());

    app.handle_key(KeyEvent::from(KeyCode::Char('d')));
    assert!(app.custom_commands().is_empty());
    app.handle_key(KeyEvent::from(KeyCode::Esc));
    assert!(app.tool_editor.is_none());
    assert_eq!(app.screen, Screen::Settings);
}

#[test]
fn the_editor_refuses_a_broken_template_and_a_seventh_entry() {
    let mut app = App::new();
    app.prefs.custom_commands.clear();
    app.open_tool_editor();
    app.handle_key(KeyEvent::from(KeyCode::Char('a')));
    for c in "bad".chars() {
        app.handle_key(KeyEvent::from(KeyCode::Char(c)));
    }
    app.handle_key(KeyEvent::from(KeyCode::Enter));
    for c in "'unclosed".chars() {
        app.handle_key(KeyEvent::from(KeyCode::Char(c)));
    }
    app.handle_key(KeyEvent::from(KeyCode::Enter));
    assert!(app.custom_commands().is_empty());
    assert!(app.error.is_some(), "the quote error is reported");

    app.error = None;
    app.prefs.custom_commands = (0..MAX_CUSTOM_COMMANDS)
        .map(|i| CustomCommand {
            label: format!("c{i}"),
            command: "sh -c true".into(),
            wait: false,
        })
        .collect();
    app.open_tool_editor();
    app.handle_key(KeyEvent::from(KeyCode::Char('a')));
    assert!(app.error.is_some(), "the cap is explained, not silent");
    assert_eq!(app.custom_commands().len(), MAX_CUSTOM_COMMANDS);

    // A hand-edited prefs.json with more than the cap still opens: the
    // extras are simply not offered a key.
    app.prefs.custom_commands.push(CustomCommand {
        label: "seventh".into(),
        command: "sh -c true".into(),
        wait: false,
    });
    app.tool_editor = None;
    app.open_tool_menu(temp_repo_dir("cap"));
    let lines = app.tool_menu_lines().join("\n");
    assert!(!lines.contains("seventh"), "{lines}");
    assert_eq!(app.menu_custom_commands().len(), MAX_CUSTOM_COMMANDS);
}

// ---- ⑭ yank --------------------------------------------------------------

#[test]
fn yank_copies_the_identifier_for_the_selection() {
    let dir = temp_repo_dir("yank");
    let mut app = app_on_repo(&dir);
    crate::clipboard::take_emitted();

    app.handle_key(KeyEvent::from(KeyCode::Char('y')));
    assert!(app.status.contains("abc1234def"), "{}", app.status);
    // Says what was sent, not that the clipboard was written: nothing here
    // can observe whether the terminal accepted it.
    assert!(app.status.contains("OSC 52"), "{}", app.status);
    let seq = crate::clipboard::take_emitted().expect("a sequence was emitted");
    assert_eq!(seq, crate::clipboard::osc52_sequence("abc1234def"));

    app.repo_tab = RepoTab::Branches;
    app.handle_key(KeyEvent::from(KeyCode::Char('y')));
    assert_eq!(
        crate::clipboard::take_emitted(),
        Some(crate::clipboard::osc52_sequence("release/1.2"))
    );

    app.repo_tab = RepoTab::Status;
    app.handle_key(KeyEvent::from(KeyCode::Char('y')));
    assert_eq!(
        crate::clipboard::take_emitted(),
        Some(crate::clipboard::osc52_sequence(
            &dir.join("src/main.rs").display().to_string()
        ))
    );

    // On Home the useful identifier is the repository path.
    app.screen = Screen::Home;
    app.handle_key(KeyEvent::from(KeyCode::Char('y')));
    assert_eq!(
        crate::clipboard::take_emitted(),
        Some(crate::clipboard::osc52_sequence(&dir.display().to_string()))
    );
}

#[test]
fn yank_sanitises_repository_controlled_text_before_emitting() {
    let dir = temp_repo_dir("yank2");
    let mut app = app_on_repo(&dir);
    app.repo_tab = RepoTab::Branches;
    // A branch name is repository-controlled text — the same text
    // `git::strip_control_sequences` refuses to trust on screen.
    if let Some(data) = app.repo_data.as_mut() {
        data.branches[0].name = "main\u{1b}]52;c;ZXZpbA==\u{7}".into();
    }
    crate::clipboard::take_emitted();
    app.handle_key(KeyEvent::from(KeyCode::Char('y')));
    let seq = crate::clipboard::take_emitted().unwrap();
    assert_eq!(seq, crate::clipboard::osc52_sequence("main]52;c;ZXZpbA=="));
    assert_eq!(
        seq.matches('\u{1b}').count(),
        1,
        "only our own introducer reaches the terminal: {seq:?}"
    );
}

#[test]
fn the_popups_and_the_settings_header_render_at_any_size() {
    let dir = temp_repo_dir("render");
    let mut app = app_on_repo(&dir);
    app.prefs.custom_commands = (0..MAX_CUSTOM_COMMANDS)
        .map(|i| CustomCommand {
            label: format!("command {i}"),
            command: format!("sh -c 'script{i} {{file}}'"),
            wait: i % 2 == 0,
        })
        .collect();
    let sizes = [(120u16, 40u16), (80, 24), (20, 8)];

    // A full menu is taller than the box used to be: it has to fit its own
    // content and still not index outside a smaller terminal.
    app.open_tool_menu(dir.clone());
    for (w, h) in sizes {
        crate::ui::tests::render(&app, w, h);
    }

    app.tool_menu = None;
    app.screen = Screen::Settings;
    for (w, h) in sizes {
        crate::ui::tests::render(&app, w, h);
    }

    app.open_tool_editor();
    for (w, h) in sizes {
        crate::ui::tests::render(&app, w, h);
    }
    // ...including both typing steps.
    app.prefs.custom_commands.truncate(1);
    app.handle_key(KeyEvent::from(KeyCode::Char('a')));
    crate::ui::tests::render(&app, 80, 24);
    app.handle_key(KeyEvent::from(KeyCode::Char('x')));
    app.handle_key(KeyEvent::from(KeyCode::Enter));
    crate::ui::tests::render(&app, 80, 24);
}
