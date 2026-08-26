use std::process::ExitCode;

const HELP: &str = "\
git-dashboard-tui — a read-only, multi-repository Git dashboard for the terminal

USAGE:
    git-dashboard-tui [OPTIONS]

OPTIONS:
    -h, --help       Print this help and exit
    -V, --version    Print the version and exit

Run with no arguments to start the dashboard. Repositories are added from
inside the app: press `a` for a single path, or `A` to scan a folder.
Press `?` at any time for the keybindings of the current screen.

Configuration is stored under your platform's config directory and is shared
with the git-dashboard GUI application.

Docs and issues: https://github.com/orapli/git-dashboard-tui
";

fn main() -> ExitCode {
    // Handled before touching the terminal: these must work when stdout is a
    // pipe (an installer checking the binary runs, `--version` in a script),
    // where initialising the TUI would panic. Every flag is terminal, so only
    // the first argument is meaningful.
    match std::env::args().nth(1).as_deref() {
        None => {}
        Some("-h" | "--help") => {
            print!("{HELP}");
            return ExitCode::SUCCESS;
        }
        Some("-V" | "--version") => {
            println!("{} {}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));
            return ExitCode::SUCCESS;
        }
        Some(other) => {
            eprintln!("git-dashboard-tui: unrecognised argument '{other}'");
            eprintln!("Try 'git-dashboard-tui --help' for usage.");
            return ExitCode::from(2);
        }
    }

    if let Err(e) = git_dashboard_tui::run() {
        eprintln!("git-dashboard-tui: {e}");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}
