# Keyboard and configuration reference

Version v0.4.1 adds mouse selection for Settings and Repository Finder,
plus title-bar navigation and result clicks.
The title bar provides buttons for Back, Home, and Navigate. Navigate opens Settings,
Worktrees, Global Members, or Commit Search; `Esc` closes its menu.

Covers v0.4.1. [User manual](manual.html) · [README](../README.md)

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
| **Mouse wheel** | Scroll lists, commit logs, diff views, and the help screen |
| **Mouse click** | Switch tabs or select items directly |
| **Click a column header** | Sort the Home list by that column; click again to reverse |
| **Click a list row** | Select it; click the selected row again to open it |
| **Settings row/tab click** | Select a repository or member, or switch the visible tab |
| **Finder row/checkbox click** | Select a row; click its `[ ]` area to toggle import selection |
| **Title bar navigation** | Click `Back`, `Home`, or `Navigate`; choose a destination from the popup |
| **Global Members member row** | Click to select a member; member rows do not open repositories |
| **Global Members repository / Commit Search hit** | Click once to select; click the selected row again to open it |
| **Click the `[ ]` marker** | In Commits and Tags, pick the compare base, then the target (same as `space`). Clicking a marked row again clears it |

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
| `C` | Open the selected repository’s latest GitHub CI run |
| `O` | Open the shell / editor / lazygit / GitUI menu |
| `W` | Browse local worktrees across repositories |
| `S` | **Cross-repo commit message search** |
| `M` | Open **Global Cross-Repo Team Analytics** |
| `p` / `f` | `git pull` / `git fetch` selected repo. While one runs, the repository's Sync cell shows a spinner and the operation name, and the title bar counts the jobs still going |
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

### 8. Cross-repository Worktrees (Home `W`)

| Key | Action |
|---|---|
| `/` | Search paths, branches and notes |
| `m` | Edit purpose note |
| `*` / `f` | Toggle favorite / filter favorites |
| `Enter` / `O` | Tools menu for the selected worktree |
| `t` | Shell in the selected worktree |
| `r` | Refresh in the background |
| `[` / `]` | Show previous / next collection error |

### 9. Tools menu (`O`)

| Key | Action |
|---|---|
| `t` / `e` / `l` / `g` | Shell / editor / lazygit / GitUI |
| `c` | Set editor command (default: `code`) |
| `w` | Wait for editor (default: off; enable for terminal editors) |
| `Esc` / `q` | Close menu |

See the [manual](manual.html#editor-examples) for command examples and support limits.

## Configuration

Settings are plain JSON, written atomically, and shared with the `git-dashboard` GUI app:

- **macOS**: `~/Library/Application Support/com.git-dashboard.git-dashboard/`
- **Linux**: `~/.config/git-dashboard/`
- **Windows**: `%APPDATA%\git-dashboard\git-dashboard\config\`

| File | Contents |
|---|---|
| `config.json` | Registered repositories: name, path, group |
| `members.json` | Team members and the commit-author aliases that merge into them |
| `prefs.json` | Language, theme, sidebar, diff toggles, recent comparisons, sort order, auto-refresh interval, external diff command, editor command/wait, onboarding dismissal, worktree notes/favorites |
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
never sit waiting for a password prompt. Local shell/editor/Git-client launch, GitHub PR/CI information and the cross-repository
Worktree workspace are unavailable for SSH entries. This restriction concerns repositories
registered as `ssh://…`, not a local clone whose origin uses SSH.


## Isolate configuration

Set `GIT_DASHBOARD_CONFIG_DIR` to an absolute directory. Configuration saves create missing directories; you can also create it before launch as below.

```bash
mkdir -p "$HOME/.gdt-demo-config"
GIT_DASHBOARD_CONFIG_DIR="$HOME/.gdt-demo-config" git-dashboard-tui
```

```powershell
New-Item -ItemType Directory -Force "$env:USERPROFILE/.gdt-demo-config" | Out-Null
$env:GIT_DASHBOARD_CONFIG_DIR = "$env:USERPROFILE/.gdt-demo-config"
git-dashboard-tui
```
