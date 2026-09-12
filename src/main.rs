use git_dashboard_tui::cli::{self, Invocation};
use std::process::ExitCode;

const HELP: &str = "\
git-dashboard-tui — find your next task across repositories in one terminal

USAGE:
    git-dashboard-tui [OPTIONS] [PATH]

OPTIONS:
    -h, --help       Print this help and exit
    -V, --version    Print the version and exit
        --json       Print a status snapshot as JSON and exit

ARGS:
    PATH             Start focused on the repository at PATH. It is shown
                     next to your registered repositories and selected,
                     marked `(not registered)`; nothing is written to
                     config.json. With --json, the snapshot covers only
                     that repository.

Run with no arguments to start the dashboard. Repositories are added from
inside the app: press `A` to scan a folder, or `a` for a single path.
Press `?` at any time for the keybindings of the current screen.

--json starts no terminal, so it can be piped, and makes no network calls:
CI and pull request fields come from the cache the dashboard already wrote
and say when they are absent or stale, and a repository that could not be
read is reported as failed rather than as zeros. Exit status is 2 for a
command line that could not be parsed, and 1 when the configuration or the
requested path could not be read.

Status viewing is observation-first. Explicit pull, fetch, stash apply and
stash drop actions can change repositories; pull follows your Git configuration.

Configuration is stored under your platform's config directory and is shared
with the git-dashboard GUI application.
Set GIT_DASHBOARD_CONFIG_DIR to use a separate configuration directory.

Docs and issues: https://github.com/orapli/git-dashboard-tui
";

fn main() -> ExitCode {
    // Parsed and fully handled before touching the terminal: `--help`,
    // `--version` and `--json` must work when stdout is a pipe (an installer
    // checking the binary runs, a status line shelling out), where
    // initialising the TUI would panic. `args_os` rather than `args` because
    // a repository path need not be valid UTF-8, and `args` panics on one.
    let invocation = match cli::parse_args(std::env::args_os().skip(1)) {
        Ok(invocation) => invocation,
        Err(message) => {
            eprintln!("git-dashboard-tui: {message}");
            eprintln!("Try 'git-dashboard-tui --help' for usage.");
            return ExitCode::from(cli::EXIT_USAGE);
        }
    };

    let focus = match invocation {
        Invocation::Help => {
            print!("{HELP}");
            return ExitCode::SUCCESS;
        }
        Invocation::Version => {
            println!("{} {}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));
            return ExitCode::SUCCESS;
        }
        Invocation::Snapshot { path } => return cli::run_snapshot(path),
        Invocation::Dashboard { path } => match path {
            None => None,
            // Validated here so a bad path is reported on a normal terminal,
            // not after the alternate screen has been entered and left.
            Some(path) => match cli::resolve_repository_path(&path) {
                Ok(path) => Some(path),
                Err(message) => {
                    eprintln!("git-dashboard-tui: {message}");
                    return ExitCode::FAILURE;
                }
            },
        },
    };

    if let Err(e) = git_dashboard_tui::run_with_focus(focus) {
        eprintln!("git-dashboard-tui: {e}");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}
