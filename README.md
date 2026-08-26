<div align="center">

# 🚀 git-dashboard-tui

**Watch every repository you own from one terminal — without touching a single one of them.**

[![CI](https://github.com/orapli/git-dashboard-tui/actions/workflows/ci.yml/badge.svg)](https://github.com/orapli/git-dashboard-tui/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-14B8A6)](LICENSE)
[![Rust](https://img.shields.io/badge/Rust-1.85%2B%20(2024%20edition)-orange)](https://www.rust-lang.org/)
[![Platforms](https://img.shields.io/badge/platforms-Linux%20%7C%20macOS%20%7C%20Windows-blue)](#installation)

[Quick start](#quick-start) · [Who this helps](#who-this-helps) · [Keybindings](#keybindings) · [日本語 README](README.ja.md)

</div>

git-dashboard-tui is a **read-only, multi-repository Git dashboard** for the terminal.
Point it at a dozen repositories and see, in one screen, which are dirty, which are behind,
which have a failing CI run, and which someone left mid-rebase. Then jump into a shell,
a diff, or a commit — and do the actual work with whatever tool you already use.

```bash
curl -fsSL https://raw.githubusercontent.com/orapli/git-dashboard-tui/main/install.sh | sh
git-dashboard-tui
```

That is the whole quickstart — Linux and macOS, no Rust toolchain required. The
installer picks the right binary for your platform and verifies its checksum. See
[Installation](#installation) for Windows, other options, and how to review the script
before running it.

Press `a` to add a repository, or `A` to scan a folder and import every Git repo under it.

```text
 Repositories   git-dashboard-tui
┌Repositories (4/4) [Group: All] updated ↓───────────────────────────────────────────────────────────────────────────┐
│  Name                       Branch                         Sync       Dirty    Updated            Path             │
│▸ [frontend] web-frontend    main ⚠MERGE ⚠1conflict         ↑0 ↓0      1        2026-08-26 06:39   /work/web-front… │
│  [backend] billing-service  main                           ↑0 ↓0      1        2026-08-26 06:37   /work/billing-s… │
│  [backend] api-gateway      main                           ↑0 ↓0      0        2026-08-26 06:35   /work/api-gatew… │
│  [infra] infra-terraform    main                           ↑0 ↓0      0        2026-08-26 06:35   /work/infra-ter… │
└────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┘
j/k move  enter open  [/] group  / filter  t shell  P/F pull/fetch all  M members  S search commits  n needs attention
```

## Who this helps

- **You maintain more repositories than you can keep in your head.** A platform or infra
  engineer with a dozen service repos gets one list showing branch, ahead/behind, dirty
  count, open PR count, and CI status — instead of `cd`-ing through directories running
  `git status` by hand.
- **You lead a team and need to spot what's stuck.** Press `n` to filter down to just the
  repositories with a failing CI run, an unresolved conflict, or a merge/rebase somebody
  abandoned halfway. Press `M` for a cross-repository contributor view.
- **You work over SSH on machines you don't clone to.** Register a repository as
  `ssh://host/path` and it appears in the same list as your local ones — status, log,
  diffs, and contributors all work without a local checkout.
- **You need to find a commit but not which repo it's in.** `S` searches commit messages
  across every registered repository at once and jumps straight into the match's diff.

## Why trust it with your repositories

This tool runs `git` against repositories you may not have written, so it is built to
treat repository content as untrusted:

- **A repository can't execute code just by being listed.** Every invocation neutralises
  the `.git/config` keys that would otherwise run commands on your machine —
  `core.fsmonitor`, `core.sshCommand`, `uploadpack.packObjectsHook`, `protocol.ext.allow`
  — and passes `--no-ext-diff --no-textconv` so `diff.external` and `textconv` drivers
  can't fire either. Git's own `safe.directory` does **not** cover this: it only guards
  repositories owned by *another* user.
- **No git operation can hang the app.** Every command runs under a hard timeout (30s for
  analysis, 120s for network operations), with output capped at 64 MiB and drained on
  dedicated threads so a full pipe can't deadlock.
- **`ssh://` destinations are validated, not interpolated.** OpenSSH has no `--` option
  terminator, so a host is accepted only if it can't be read as a flag.
- **It runs entirely locally.** No telemetry and no external service. The only network
  traffic is Git talking to your own remotes, plus `gh` if you have it installed.
- **It won't rewrite your history, because it can't.** The only write operations that
  exist are `pull`, `fetch`, `stash apply`, and `stash drop` (see
  [Scope](#scope-what-this-tool-does-not-do)).

See [SECURITY.md](SECURITY.md) for the threat model and how to report an issue.

## Scope: what this tool does *not* do

git-dashboard-tui is deliberately **observation-first**. There is no staging, committing,
branching, checkout, merge, rebase, cherry-pick, or push. When you want to change
something, press `t` to drop into `$SHELL` in that repository — or `c` in Settings to wire
up an external diff tool — and use whatever you already use.

If you want a terminal UI that *does* stage and commit, use
[lazygit](https://github.com/jesseduffield/lazygit) or [gitui](https://github.com/gitui-org/gitui);
they're excellent, and they solve a different problem than this does.

## What you get

| Screen | What it shows |
|---|---|
| **Home** | Every repository: branch, `↑ahead ↓behind`, dirty count, last commit, `[PR:n]`, `✓CI`/`✗CI`, and `⚠MERGE`/`⚠REBASE`/`⚠Nconflict` warnings |
| **Status** (`1`) | Overview KPIs, recent commits, and the working-tree file list |
| **Commits** (`2`) | Commit graph with git's own per-lane colouring, branch/tag badges, and base↔target comparison |
| **Branches / Tags** (`3` `4`) | Sorted lists with author and subject; compare any two tags |
| **Stash** (`5`) | Inspect, apply, or drop stash entries |
| **Contributors** (`6`) | Per-repository contributor stats over All / 1w / 1m / 3m |
| **Worktrees** (`7`) | Every worktree with path, branch, HEAD, and lock/prunable state — `Enter` opens a shell inside one |
| **Diff** | Split/unified diff with syntax highlighting, hunk navigation, and an optional per-line blame gutter |
| **Commit Search** (`S`) | Commit-message search across *all* registered repositories at once |
| **Global Members** (`M`) | Contributor totals aggregated across every repository, with alias merging |

### Cross-repo commit search

```text
 Commit Search   git-dashboard-tui
┌Commit Search "timeout" (3)─────────────────────────────────────────────────────────────────────────────────────────┐
│▸ [web-frontend    ] 83ae2c6  2026-08-26  Bob               Raise checkout timeout to 60s                           │
│  [web-frontend    ] bde61da  2026-08-26  Bob               Raise checkout timeout to 45s                           │
│  [web-frontend    ] ec9e778  2026-08-26  Bob               Add checkout timeout                                    │
└────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┘
j/k move  / new search  enter open commit  ? help  esc back  q quit
```

### Diff with the blame gutter (`b`)

Uncommitted lines are attributed to `Not Committed Yet` rather than misattributed to
whoever last touched the line:

```text
┌  Changed Files (1)──────┐┌  hunks (1/1)──────────┐┌▸ Diff    [f:full b:blame]────────────────────────┐
│▸ [M] src/invoice.rs     ││▸ #1   L10    /// Total││0000000 Not Committed…│ +/// Total including tax, │
│                         ││                       ││ce832db Carol         │  pub fn total(items: &[Ite│
│                         ││                       ││ce832db Carol         │      let sub = subtotal(it│
│                         ││                       ││ce832db Carol         │      sub + tax(sub, rate) │
└─────────────────────────┘└───────────────────────┘└──────────────────────────────────────────────────┘
hunk 1/1  n/p hunk  tab pane  [/] file  w ignore ws  f full file  b blame  t shell  ? help  esc back  q
```

## Installation

### One-line install (Linux & macOS)

```bash
curl -fsSL https://raw.githubusercontent.com/orapli/git-dashboard-tui/main/install.sh | sh
```

Detects your platform, downloads the matching binary, verifies its SHA-256 checksum
against the one published with the release, and installs to `~/.local/bin` (or
`/usr/local/bin` if that is already writable and on your `PATH`). It never uses `sudo`.

```bash
# Install somewhere specific, or pin a version
curl -fsSL https://raw.githubusercontent.com/orapli/git-dashboard-tui/main/install.sh | INSTALL_DIR=~/bin sh
curl -fsSL https://raw.githubusercontent.com/orapli/git-dashboard-tui/main/install.sh | VERSION=v0.2.1 sh
```

> Piping a script into a shell means trusting it. The script is short and dependency-free —
> read it first if you prefer: [`install.sh`](install.sh), or
> `curl -fsSL https://raw.githubusercontent.com/orapli/git-dashboard-tui/main/install.sh | less`

### Pre-built binary, by hand

One self-contained binary, nothing else to install. Pick your platform:

```bash
# Linux x86_64
curl -sSL https://github.com/orapli/git-dashboard-tui/releases/latest/download/git-dashboard-tui-x86_64-unknown-linux-gnu.tar.gz | tar xz

# Linux ARM64
curl -sSL https://github.com/orapli/git-dashboard-tui/releases/latest/download/git-dashboard-tui-aarch64-unknown-linux-gnu.tar.gz | tar xz

# macOS (Apple Silicon)
curl -sSL https://github.com/orapli/git-dashboard-tui/releases/latest/download/git-dashboard-tui-aarch64-apple-darwin.tar.gz | tar xz

# macOS (Intel)
curl -sSL https://github.com/orapli/git-dashboard-tui/releases/latest/download/git-dashboard-tui-x86_64-apple-darwin.tar.gz | tar xz
```

Then move it somewhere on your `PATH`:

```bash
chmod +x git-dashboard-tui
sudo mv git-dashboard-tui /usr/local/bin/    # or ~/.local/bin, or anywhere on PATH
```

**Windows (x64)**: download
[`git-dashboard-tui-x86_64-pc-windows-msvc.zip`](https://github.com/orapli/git-dashboard-tui/releases/latest)
and extract `git-dashboard-tui.exe`.

> On macOS, Gatekeeper may block an unsigned binary on first run. Allow it under
> **System Settings → Privacy & Security**, or run
> `xattr -d com.apple.quarantine git-dashboard-tui`.

All binaries are on the [Releases](https://github.com/orapli/git-dashboard-tui/releases) page.

### With cargo (builds from source, no clone needed)

```bash
# Requires Rust 1.85+ (2024 edition)
cargo install --git https://github.com/orapli/git-dashboard-tui --locked
```

### From a clone (for development)

```bash
git clone https://github.com/orapli/git-dashboard-tui
cd git-dashboard-tui
cargo install --path . --locked
```

### Requirements

| | |
|---|---|
| **Required** | `git` on your `PATH` |
| **Optional** | [`gh`](https://cli.github.com/), authenticated — enables the `[PR:n]` and `✓CI`/`✗CI` badges. Without it those columns are simply absent; nothing else changes. |
| **Optional** | An `ssh` client, for `ssh://` repositories (key-based auth only — `BatchMode=yes` means it never prompts) |
| **Tested on** | Linux, macOS, and Windows in CI on every push |

## Quick start

```bash
git-dashboard-tui
```

Then:

1. `a` — add one repository by path (`Tab` autocompletes)
2. `A` — or scan a whole folder and batch-import every Git repo under it
3. `Enter` — open a repository; `1`–`7` switch tabs
4. `t` — drop into `$SHELL` there when you want to actually change something
5. `?` — the full keybinding help, in-app and context-aware

## Keybindings

### Global

| Key | Action |
|---|---|
| `q` / `Ctrl+C` | Quit application |
| `Esc` / `h` / `←` | Back to previous screen / Clear filter |
| `?` | Toggle Help dialog (contextual return) |
| `/` | Live filter of the current view |
| `j` / `k` (or `↓` / `↑`) | Move selection |
| `g` / `G` | Jump to top / bottom of list |
| `t` | Open `$SHELL` in the current repository<br>(except the Contributors tab, where `t` toggles member status) |
| **Mouse wheel** | Scroll lists, commit logs, and diff views |
| **Mouse click** | Switch tabs or select items directly |
| **Click a column header** | Sort the Home list by that column; click again to reverse |

### 1. Home (multi-repo hub)

| Key | Action |
|---|---|
| `Enter` | Open selected repository |
| `a` / `A` | Add repository by path / Launch **Repository Finder** (scan folder) |
| `d` | Remove selected repository |
| `e` | Rename repository alias |
| `o` | Cycle sort order — name, branch, sync, dirty, updated, each ↑ and ↓. The sorted column is marked ▲ / ▼ in the header, and can also be set by clicking it. |
| `[` / `]` | Cycle repository group filter |
| `n` | Toggle **needs-attention** filter (failing CI / conflicts / interrupted op) |
| `S` | **Cross-repo commit message search** |
| `M` | Open **Global Cross-Repo Team Analytics** |
| `p` / `f` | `git pull` / `git fetch` selected repo |
| `P` / `F` | Bulk `git pull` / `git fetch` across all filtered repos |
| `r` | Reload all repository rows |
| `t` / `T` | Open `$SHELL` in selected repository |
| `s` | Open Settings |

### 2. Repository details (`1` – `7`)

| Tab | Key | Action |
|---|---|---|
| **Tabs** | `1` - `7` / `Tab` / `[` `]` | Switch between Status, Commits, Branches, Tags, Stash, Contributors, Worktrees |
| **(any tab)** | `p` / `f` | `git pull` / `git fetch` this repository |
| | `r` | Reload repository data |
| | `T` | Open `$SHELL` at repository root |
| **Status** (`1`) | `Enter` | Open working tree file diff |
| **Commits** (`2`) | `Space` | Select commit base/target for comparison |
| | `Enter` | Open commit diff / comparison |
| | `i` | Force open built-in TUI diff |
| **Branches** (`3`) | `Enter` | View interactive oneline branch log |
| **Tags** (`4`) | `Space` / `Enter` | Select tag range / Compare tags |
| **Stash** (`5`) | `Enter` / `a` / `d` | Inspect stash diff / Apply stash / Drop stash |
| **Contributors** (`6`) | `w` | Cycle time period (**All** / **1w** / **1m** / **3m**) |
| | `Space` / `t` | Toggle active/inactive member status |
| | `m` | Filter active members only |
| **Worktrees** (`7`) | `Enter` | **Launch `$SHELL` inside the selected worktree**<br>(plain `t` / `T` opens the repository root instead) |

### 3. Diff view

| Key | Action |
|---|---|
| `Tab` / `l` / `→` | Cycle focus forward: **Files** → **Hunks** → **Diff Content** |
| `Shift+Tab` / `h` / `←` | Cycle focus backward |
| `n` / `p` (or `N`) | Jump to Next / Previous change hunk |
| `[` / `]` | Jump to Previous / Next modified file |
| `g` / `G` | Jump to start / end of diff |
| `PageUp` / `PageDown` / `Space` | Scroll diff content by a page |
| `w` | Toggle **ignore whitespace** |
| `f` | Toggle **full-file context** |
| `b` | Toggle **blame gutter** (short hash + author per line) |
| `Enter` | Load selected file (Files pane) / jump to hunk (Hunks pane) |
| `t` / `T` | Open terminal at repository root |
| `Esc` | Return to repository view |

### 4. Settings (`s`)

| Key | Action |
|---|---|
| `Tab` / `1` / `2` | Switch between **Repositories** and **Members** tabs |
| `a` | Add repository / member (depending on tab) |
| `A` | Launch **Repository Finder** (Repositories tab) |
| `g` | Edit repository group (Repositories tab) |
| `e` | Rename repository / edit member aliases |
| `d` | Delete selected repository / member |
| `Space` / `t` | Toggle member active status (Members tab) |
| `c` | Set external diff tool (empty = built-in viewer) |
| `l` | Toggle language (English / 日本語) |
| `i` | Cycle **auto-refresh** interval (off / 30s / 1m / 5m) |
| `T` | Toggle **theme** (Catppuccin Mocha / Latte) |

### 5. Commit Search (`S` from Home)

| Key | Action |
|---|---|
| `/` | Start a new search |
| `Enter` | Open the selected commit's diff (jumps into its repository) |
| `Esc` | Back to Home |

### 6. Global Members (`M` from Home)

| Key | Action |
|---|---|
| `Tab` / `h` / `l` | Switch pane between member list and their repositories |
| `Enter` | Jump into the selected repository |
| `Space` / `t` | Toggle member active / inactive |
| `m` | Filter to active members only |
| `T` | Open `$SHELL` in the selected repository |
| `/` | Search members or repositories |

### 7. Repository Finder (`A` from Home)

| Key | Action |
|---|---|
| `Space` | Toggle selection of the highlighted repository |
| `a` | Select / deselect all |
| `Enter` | Import all selected repositories |
| `r` | Change the folder being scanned |
| `/` | Filter discovered repositories |
| `Esc` / `q` | Cancel |

## Configuration

Settings are plain JSON, written atomically, and shared with the `git-dashboard` GUI app:

- **macOS**: `~/Library/Application Support/com.git-dashboard.git-dashboard/`
- **Linux**: `~/.config/git-dashboard/`
- **Windows**: `%APPDATA%\git-dashboard\git-dashboard\config\`

| File | Contents |
|---|---|
| `config.json` | Registered repositories: name, path, group |
| `members.json` | Team members and the commit-author aliases that merge into them |
| `prefs.json` | Language, theme, diff toggles, sort order, auto-refresh interval, external diff command |
| `tech_rules.json` | Rules for detecting language/framework versions from manifest files |
| `cache/` | Per-repository cache of the Home row and the repository snapshot, so the dashboard opens populated instead of blank while the refresh runs. Safe to delete; it is rebuilt on the next refresh |

Most preferences are set from inside the app (`s` for Settings) rather than by editing
these files by hand.

### Using an external diff tool

Press `c` in Settings to hand diffs to another tool instead of the built-in viewer. The
command may contain `{path}`, `{range}`, `{base}`, `{target}`, and `{file}` placeholders;
leave it empty to use the built-in viewer. Programs are resolved from `PATH` or given as
an absolute path — a relative path is rejected on purpose, since the child process runs
with its working directory set to the repository being viewed.

### Remote repositories over SSH

Register a repository as `ssh://host/absolute/path` (the host may be any alias from your
`~/.ssh/config`). Status, log, diff, and contributor analysis run over one multiplexed SSH
connection per refresh. Authentication is key-based only — `BatchMode=yes` means it will
never sit waiting for a password prompt. Opening a local `$SHELL` is the one feature not
available for these.

## Project resources

- **[Changelog](CHANGELOG.md)** — what changed, by release.
- **[Security policy](SECURITY.md)** — threat model and how to report a vulnerability.
- **[Engineering wiki](openwiki/index.md)** — generated architecture notes: the
  [worker/queue model](openwiki/application/background-work.md), the
  [git execution engine](openwiki/git/engine.md), and more.
- **[Developer guide](CLAUDE.md)** — commands, module layout, and implementation rules.
- **[Issues](https://github.com/orapli/git-dashboard-tui/issues)** — bug reports and feature requests.

## Development

```bash
cargo fmt              # format
cargo clippy --all-targets -- -D warnings
cargo test             # unit + integration tests against real temporary repos
cargo run              # debug build
```

CI runs formatting, Clippy (`-D warnings`), and the full test suite on Linux, macOS, and
Windows for every push — a commit that fails `fmt` or `clippy` fails CI.

```
src/
├── app/        # State machine, key/mouse handling, background worker
├── git/        # Git command execution and output parsing
├── ui.rs       # Ratatui rendering
├── syntax.rs   # Lightweight syntax tokenizer
├── colors.rs   # Catppuccin palettes
└── config.rs   # Config load/save (always via write_atomic)
```

Start with the [engineering wiki](openwiki/index.md) before changing the worker model,
the git execution boundary, or the diff pipeline.

## License

[MIT](LICENSE)
