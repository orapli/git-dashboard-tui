rust_i18n::i18n!("locales", fallback = "en");

pub mod app;
pub mod colors;
pub mod config;
pub mod git;
pub mod i18n;
pub mod syntax;
pub mod ui;

use crossterm::event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyEventKind};
use crossterm::terminal::{self, LeaveAlternateScreen};
use crossterm::{cursor, execute};
use std::io::{self, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::Duration;

pub use app::{App, RepoTab, Screen, SettingsTab};
pub use colors::Palette;
pub use git::TimeSpan;

pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let orig_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = execute!(io::stdout(), DisableMouseCapture);
        ratatui::restore();
        orig_hook(info);
    }));

    enter_tui();
    let mut terminal = ratatui::init();
    let mut app = App::new();
    let result = event_loop(&mut terminal, &mut app);
    leave_tui();
    ratatui::restore();
    result?;
    Ok(())
}

fn enter_tui() {
    let _ = execute!(io::stdout(), EnableMouseCapture);
}

fn event_loop(terminal: &mut ratatui::DefaultTerminal, app: &mut App) -> io::Result<()> {
    while !app.should_quit {
        app.drain_messages();
        app.maybe_auto_refresh();
        if let Some(tool) = app.take_work_tool() {
            let result = if tool.wait {
                leave_tui();
                let result = run_work_tool(&tool);
                enter_tui();
                *terminal = ratatui::init();
                drain_pending_keys();
                result
            } else {
                run_work_tool(&tool)
            };
            if let Err(error) = result {
                app.error = Some(error);
            }
            app.after_external_work(&tool.command.cwd);
            continue;
        }
        if let Some(ext) = app.take_external() {
            leave_tui();
            let msg = run_external(&ext);
            enter_tui();
            *terminal = ratatui::init();
            drain_pending_keys();
            match msg {
                Ok(s) => app.status = s,
                Err(e) => app.error = Some(e),
            }
            continue;
        }
        if let Some(dir) = app.take_terminal() {
            leave_tui();
            let msg = run_terminal(&dir);
            enter_tui();
            *terminal = ratatui::init();
            drain_pending_keys();
            app.after_external_work(&dir);
            if let Err(e) = msg {
                app.error = Some(e);
            }
            continue;
        }
        terminal.draw(|frame| ui::draw(frame, app))?;

        if event::poll(Duration::from_millis(100))? {
            match event::read()? {
                Event::Key(key) if key.kind == KeyEventKind::Press => {
                    app.handle_key(key);
                }
                Event::Mouse(mouse) => {
                    app.handle_mouse(mouse);
                }
                _ => {}
            }
        }
    }
    Ok(())
}

fn run_terminal(cwd: &std::path::Path) -> Result<(), String> {
    #[cfg(windows)]
    let shell = std::env::var("COMSPEC").unwrap_or_else(|_| "cmd.exe".to_string());
    #[cfg(not(windows))]
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string());

    let mut cmd = Command::new(&shell);
    cmd.current_dir(cwd)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());

    eprintln!(
        "\n--- git-dashboard-tui: Shell opened at {} (type 'exit' to return) ---\n",
        cwd.display()
    );
    let _ = io::stderr().flush();

    let _ = cmd
        .status()
        .map_err(|e| format!("Failed to spawn shell '{shell}': {e}"))?;
    Ok(())
}

fn leave_tui() {
    let _ = terminal::disable_raw_mode();
    let mut out = io::stdout();
    let _ = execute!(out, LeaveAlternateScreen, DisableMouseCapture, cursor::Show);
    let _ = out.flush();
}

fn drain_pending_keys() {
    while event::poll(Duration::from_millis(0)).unwrap_or(false) {
        let _ = event::read();
    }
}

fn run_external(ext: &app::ExternalDiff) -> Result<String, String> {
    let program = resolve_program(&ext.program)?;
    let cmdline = format!(
        "{} {}  (cwd {})",
        program.display(),
        ext.args.join(" "),
        ext.cwd.display()
    );
    eprintln!("\n--- git-dashboard-tui: {cmdline}\n");
    let _ = io::stderr().flush();

    let status = Command::new(&program)
        .args(&ext.args)
        .current_dir(&ext.cwd)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .map_err(|e| format!("{cmdline}: {e}"))?;

    if status.success() {
        Ok(cmdline)
    } else {
        eprintln!("\n{cmdline}\nexited {status}");
        eprint!("Enter で git-dashboard-tui に戻る / press Enter to return: ");
        let _ = io::stderr().flush();
        let _ = io::stdin().read_line(&mut String::new());
        Err(format!("{cmdline}  →  {status}"))
    }
}

fn run_work_tool(tool: &app::WorkTool) -> Result<(), String> {
    let program = resolve_program(&tool.command.program)?;
    let mut command = Command::new(program);
    command
        .args(&tool.command.args)
        .current_dir(&tool.command.cwd);
    if tool.wait {
        let status = command
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status()
            .map_err(|e| e.to_string())?;
        if status.success() {
            Ok(())
        } else {
            Err(format!("{}: {status}", tool.command.program))
        }
    } else {
        let mut child = command
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| e.to_string())?;
        std::thread::spawn(move || {
            let _ = child.wait();
        });
        Ok(())
    }
}

fn resolve_program(name: &str) -> Result<PathBuf, String> {
    let p = PathBuf::from(name);
    if p.is_absolute() {
        return if let Some(found) = executable_candidate(&p, cfg!(windows)) {
            Ok(found)
        } else {
            Err(format!("command not found: {name}"))
        };
    }
    // A relative program name would be resolved against the *repository* being
    // viewed, because the child runs with cwd set to it — `./tool` in a diff
    // command would execute a binary shipped by that repository.
    if name.contains('/') || name.contains('\\') {
        return Err(format!(
            "コマンドには絶対パスか PATH 上のコマンド名を指定してください: {name}"
        ));
    }
    if let Some(path) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path) {
            if !dir.is_absolute() {
                continue;
            }
            let cand = dir.join(name);
            if let Some(found) = executable_candidate(&cand, cfg!(windows)) {
                return Ok(found);
            }
        }
    }
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let extras = [
        PathBuf::from("/opt/homebrew/bin"),
        PathBuf::from("/usr/local/bin"),
        PathBuf::from("/opt/homebrew/opt/hunk/bin"),
    ];
    for dir in extras {
        let cand = dir.join(name);
        if let Some(found) = executable_candidate(&cand, cfg!(windows)) {
            return Ok(found);
        }
    }
    if let Some(home) = home {
        for dir in [
            home.join(".hunk"),
            home.join(".hunk/bin"),
            home.join(".local/bin"),
        ] {
            let cand = dir.join(name);
            if let Some(found) = executable_candidate(&cand, cfg!(windows)) {
                return Ok(found);
            }
        }
    }
    Err(format!(
        "command not found: {name} (install it on PATH / PATH上に導入してください)"
    ))
}

// Only native executable suffixes are inferred. Do not turn PATH script entries
// into implicit shell execution; command arguments must remain literal.
fn executable_candidate(path: &std::path::Path, windows: bool) -> Option<PathBuf> {
    if path.is_file() {
        return Some(path.to_path_buf());
    }
    if windows && path.extension().is_none() {
        for extension in ["exe", "com"] {
            let candidate = path.with_extension(extension);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {

    #[test]
    fn windows_native_executables_are_found_without_enabling_scripts() {
        let root = std::env::temp_dir().join(format!("gdt-native-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        for name in ["lazygit", "gitui", "editor with spaces"] {
            let exe = root.join(format!("{name}.exe"));
            std::fs::write(&exe, "fixture").unwrap();
            assert_eq!(
                executable_candidate(&root.join(name), true),
                Some(exe.clone())
            );
            assert_eq!(executable_candidate(&root.join(name), false), None);
            assert_eq!(executable_candidate(&exe, true), Some(exe));
        }
        std::fs::write(root.join("script.cmd"), "fixture").unwrap();
        assert_eq!(executable_candidate(&root.join("script"), true), None);
        std::fs::remove_dir_all(root).unwrap();
    }

    use super::*;

    /// `run_external` spawns the resolved program with its working directory
    /// set to the repository being viewed, so a *relative* program name would
    /// be resolved against that repository — letting a repository you merely
    /// opened supply the binary that runs. Rejecting relative paths is the
    /// only thing preventing that, so it needs a test of its own.
    #[test]
    fn resolve_program_rejects_relative_paths() {
        for name in ["./tool", "../tool", "scripts/diff", "a/b/c"] {
            let err = resolve_program(name).expect_err(&format!("{name} must be rejected"));
            assert!(
                err.contains(name),
                "error should name the offending program: {err}"
            );
        }
        // Windows-style separators are rejected on every platform, since the
        // same cwd-relative resolution applies there.
        assert!(resolve_program(r"scripts\diff").is_err());
    }

    #[test]
    fn resolve_program_accepts_a_bare_name_found_on_path() {
        // Put a known file on PATH rather than assuming some system binary
        // exists: `sh` isn't on Windows, and naming any real program makes
        // the test depend on the runner's image instead of on the lookup.
        let dir = std::env::temp_dir().join(format!("gdt-path-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let tool = dir.join("gdt-fake-difftool");
        std::fs::write(&tool, b"#!/bin/sh\n").unwrap();

        let old_path = std::env::var_os("PATH");
        let mut entries = vec![dir.clone()];
        if let Some(ref p) = old_path {
            entries.extend(std::env::split_paths(p));
        }
        let joined = std::env::join_paths(entries).unwrap();
        // SAFETY: single-threaded within this test; PATH is restored below.
        unsafe { std::env::set_var("PATH", &joined) };

        let resolved = resolve_program("gdt-fake-difftool");

        match old_path {
            Some(p) => unsafe { std::env::set_var("PATH", p) },
            None => unsafe { std::env::remove_var("PATH") },
        }
        std::fs::remove_dir_all(&dir).ok();

        let resolved = resolved.expect("a bare name on PATH should resolve");
        assert!(resolved.is_absolute(), "PATH lookup must yield a full path");
        assert!(resolved.ends_with("gdt-fake-difftool"));
    }

    #[test]
    fn resolve_program_reports_a_missing_command_instead_of_guessing() {
        let err = resolve_program("definitely-not-a-real-program-9f3c")
            .expect_err("a nonexistent command must not resolve");
        assert!(err.contains("definitely-not-a-real-program-9f3c"));
    }

    #[test]
    fn resolve_program_accepts_an_absolute_path_only_when_it_exists() {
        let dir = std::env::temp_dir().join(format!("gdt-resolve-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let real = dir.join("mytool");
        std::fs::write(&real, b"#!/bin/sh\n").unwrap();

        assert_eq!(resolve_program(real.to_str().unwrap()).unwrap(), real);

        // An absolute path that doesn't exist is an error, not something
        // handed to Command::new to fail cryptically later.
        let missing = dir.join("nope");
        assert!(resolve_program(missing.to_str().unwrap()).is_err());

        // A directory is not a program.
        assert!(resolve_program(dir.to_str().unwrap()).is_err());

        std::fs::remove_dir_all(&dir).ok();
    }
}
