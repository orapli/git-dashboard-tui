# Keyboard and configuration reference

Version v0.5.0 adds the command line and `--json`, commit-search filters, `y` to copy,
user-defined work-tool commands, branch-scoped CI and pull-request signals, and a Home
row that distinguishes unknown from fine.
The title bar provides buttons for Back, Home, and Navigate. Navigate opens Settings,
Worktrees, Global Members, or Commit Search; `Esc` closes its menu.

Covers v0.5.0. [User manual](manual.html) · [README](../README.md)

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
| `y` | Copy the identifier for the current selection — the commit hash, branch, tag, stash ref, author e-mail, file path, worktree path or repository path, depending on where you are.<br>Home, repository details, the diff view, the worktree view and the tools menu; elsewhere it does nothing |
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
| `n` | Toggle **needs-attention** filter — a failing CI run on this branch, an unresolved conflict, an interrupted operation, a registration that could not be read, a pull request waiting on your review, or changes requested on this branch’s pull request |
| `C` | Open the GitHub CI run for the selected repository’s current branch |
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
| `x` | Add, edit or delete user-defined work-tool commands |
| `l` | Toggle language (English / 日本語) |
| `i` | Cycle **auto-refresh** interval (off / 30s / 1m / 5m) |
| `T` | Toggle **theme** (Catppuccin Mocha / Latte) |

### 5. Commit Search (`S` from Home)

| Key | Action |
|---|---|
| `/` | Start a new search |
| `author:` / `path:` | Limit to an author or a path — `author:jane path:src/` |
| `since:` / `until:` | Limit to a date range — `since:2.weeks until:2024-01-01` |
| `Enter` | Open the selected commit's diff (jumps into its repository) |
| `Esc` | Back to Home |

Filters are whitespace-separated tokens in the same one-line prompt; every other word is
still message text, so `fix: auth` and `fix(auth):` keep working. Quote a value containing
spaces (`author:"Jane Doe"`), and quote the whole token (`"path:"`) to search for that
literal text instead. `author:` also matches the aliases `members.json` merges into that
person, so a canonical name finds their other identities. `author:` and `path:` may repeat
and are OR'd by git; a repeated `since:` / `until:` keeps the last one. A value may not
start with `-` or contain a control character, and a query that fails to parse leaves the
previous result on screen instead of blanking it. Results are capped at 50 commits per
repository and 300 overall — when a cap is hit, the heading says which one and how many
repositories reached it.

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
| `[` / `]` | Show the previous / next problem — first the repositories whose worktree list could not be read, then the individual rows whose directory could not be inspected |

A row whose directory could not be inspected reads `unknown` instead of `Dirty ?`, is
counted in the header's `unknown` total rather than absorbed into a healthy count, and
carries its own reason in the `[` / `]` inspector.

### 9. Tools menu (`O`)

| Key | Action |
|---|---|
| `t` / `e` / `l` / `g` | Shell / editor / lazygit / GitUI |
| `1` – `6` | Run the matching user-defined command |
| `c` | Set editor command (default: `code`) |
| `w` | Wait for editor (default: off; enable for terminal editors) |
| `x` | Add, edit or delete user-defined commands |
| `y` | Copy this path |
| `Esc` / `q` | Close menu |

See the [manual](manual.html#editor-examples) for command examples and support limits.

`y` copies the identifier for whatever is selected: the commit hash in Commits, the branch,
the tag, the stash ref, the contributor's email address (what `git log --author=` wants), the
absolute file path in Status and in the Diff view, and otherwise the repository or worktree
path. It is sent to the terminal as an OSC 52 sequence, which many terminals ignore by default
(tmux needs `set-clipboard on`), so the confirmation says the sequence was emitted rather than
claiming the clipboard was written. The payload is stripped of control characters and refused
above a size limit.

## Reading the Home row

| Cell | Meaning |
|---|---|
| `↑2 ↓1` | Commits ahead of / behind the upstream, as of the last fetch |
| `↑? ↓?` | There is nothing to compare against — no remote, or a branch that was never pushed; the panel names which. A row restored from a cache written before v0.5.0 shows it too, because that cache did not record the answer, and then the panel has nothing to add |
| `2M 5?` | Two tracked files changed and five untracked files. A row restored from an older cache shows one unsuffixed total instead of inventing an attribution |
| `0` | A clean working tree |
| `⚠ path missing` / `⚠ not a git repo` / `⚠ unreadable` | The registration could not be read at all. Every other cell shows `—`, the panel prints the full reason, and `n` surfaces the row |
| `…` | Not loaded yet |
| `~/…/repo` | A shortened path: the middle is elided, never the leaf, so the distinguishing end of the path survives |
| Spinner and an operation name | A pull, fetch or row refresh is running; it replaces the Sync cell, which is the value it is about to change |

The Branch cell carries the branch name, then what is happening to it. Everything after
the branch name is dropped whole — never cut in half — when the cell is too narrow, least
actionable first, so a `[PR:50]` can disappear but can never become a `[PR:5]`.

| Mark | Meaning |
|---|---|
| `⚠MERGE` / `⚠REBASE` / `⚠2conflict` | An operation left mid-flight, or unresolved conflicts. Kept whichever way the cell is squeezed |
| `#42` | The pull request opened from this branch |
| `✗rev` / `✓rev` | That pull request has changes requested / is approved. Nothing is shown while a review is merely outstanding, which is every open PR's default |
| `draft` | That pull request is a draft. First to be dropped: being a draft is a choice, not a task |
| `[REV:3]` | Three open pull requests in this repository are waiting on **your** review |
| `[PR:7]` | Seven open pull requests in total, `100+` above a hundred |
| `✓CI` / `✗CI` / `●CI` | The latest CI run on this branch succeeded / failed / is something else (running, cancelled, action required) |
| `—CI` / `⏱CI` / `?CI` | There is no CI answer: the repository is on a detached HEAD, the fetch timed out, or it failed. The mark is there because a blank space reads as "nothing wrong" |

Three marks use `?`, each in its own column and each meaning "not known": `↑? ↓?` in Sync,
`5?` in Dirty (the count of untracked files), and `?CI` in Branch. The em dash likewise
means "no value" in both places it appears — a failed row's other cells, and `—CI`.

The selected-repository panel shows two ages side by side, because they answer different
questions: **Local read (working tree)** is when the dashboard last ran git here, and
**Remote fetched (↑↓)** is the modification time of `FETCH_HEAD` — the age of the ahead/behind
counts, which only a fetch can refresh. It reads `never fetched`, `not recorded`, or
`unavailable (remote repo)` for an `ssh://` entry.

The CI line names the branch its answer is about, and its state is one of ready, no runs,
not fetched, unauthenticated, fetch failed, `gh` not installed, SSH unsupported,
no branch (detached HEAD) or timed out. The PR line shows the open pull-request count, this
branch's own pull request with its draft state and review decision, and how many pull requests
are waiting on your review.

## Command line

| Form | Effect |
|---|---|
| `git-dashboard-tui` | Start the dashboard |
| `git-dashboard-tui PATH` | Start focused on the repository at PATH |
| `git-dashboard-tui --json` | Print a status snapshot of the registered repositories and exit |
| `git-dashboard-tui PATH --json` | Snapshot of that one repository |
| `-h` / `--help` · `-V` / `--version` | Usage / version |
| `--` | End flag parsing, so a directory really named `--json` can still be opened |

`PATH` is shown next to your registered repositories, selected, and labelled
`(not registered)`; nothing is written to `config.json`, so running it in a scratch clone does
not grow your configuration. A path that is already registered selects the existing row instead
of adding a duplicate, and a path that is not a repository fails before the terminal is touched.
The cross-repository views still see everything you track.

Exit status is `0` on success, `2` for a command line that could not be parsed, and `1` when the
configuration or the requested path could not be read.

### The `--json` document

`--json` initialises no terminal, so it can be piped, and it makes **no network calls** — a
status line shelling out every few seconds must not hammer the GitHub API. CI and pull-request
fields therefore come from the cache the dashboard already wrote, and say when they are absent
or stale rather than emitting a plausible zero. Local git state is read fresh, with the same
commands and the same timeout the dashboard uses.

The document is an object, not a bare array, so it can carry a schema name and version and gain
fields later. `schema_version` is bumped only for a breaking change: adding a field is not one.

| Field | Meaning |
|---|---|
| `schema` | Always `git-dashboard-tui.status-snapshot` |
| `schema_version` | Currently `1` |
| `tool_version` | Version of the binary that produced the document |
| `generated_at` / `generated_at_unix` | RFC 3339 in UTC / epoch seconds |
| `repository_count` | Number of entries in `repositories` |

| Repository field | Meaning |
|---|---|
| `name` / `path` / `group` | As configured; `group` is `null` when unset |
| `registered` | `false` for a PATH operand that is not in `config.json` |
| `ok` / `error` | `false` with a one-line reason when the repository could not be read; every measured field below is then `null` |
| `branch` | The current branch, or `""` on a detached HEAD |
| `has_upstream` | Whether the branch tracks anything |
| `ahead` / `behind` | `null`, not `0`, when there is no upstream |
| `uncommitted` / `conflicts` | Porcelain entries including untracked files / unmerged paths |
| `operation` | `none`, `merge`, `rebase`, `cherry_pick` or `revert` |
| `last_commit` / `last_commit_unix` | Time of the HEAD commit |
| `github` | The cache-only CI and pull-request block below |

| `github` field | Meaning |
|---|---|
| `cached` | `false` when the dashboard has nothing cached for this repository; `note` then says why |
| `stale` | The cached values are old enough that the dashboard itself would re-fetch them |
| `age_seconds` | Age of the newest cached value |
| `ci_state` / `pr_state` | `not_fetched`, `ready`, `no_runs`, `unauthenticated`, `fetch_failed`, `gh_not_installed`, `unsupported`, `detached_head` or `fetch_timed_out` |
| `ci_status` | `success`, `failure` or `pending`, as reported by `gh` |
| `ci_branch` | The branch the CI answer is about |
| `last_run_url` | The run page `C` opens |
| `open_prs` | Open pull requests in the repository |
| `ci_fetched_at` / `pr_fetched_at` (and their `_unix` forms) | When each cached value was written |

```json
{
  "schema": "git-dashboard-tui.status-snapshot",
  "schema_version": 1,
  "tool_version": "0.5.0",
  "generated_at": "2026-09-12T09:15:04Z",
  "generated_at_unix": 1789204504,
  "repository_count": 1,
  "repositories": [
    {
      "name": "git-dashboard-tui",
      "path": "/home/you/work/git-dashboard-tui",
      "group": "OSS",
      "registered": true,
      "ok": true,
      "error": null,
      "branch": "main",
      "has_upstream": true,
      "ahead": 2,
      "behind": 0,
      "uncommitted": 3,
      "conflicts": 0,
      "operation": "none",
      "last_commit_unix": 1789202400,
      "last_commit": "2026-09-12T08:40:00Z",
      "github": {
        "cached": true,
        "note": null,
        "stale": false,
        "age_seconds": 42,
        "ci_state": "ready",
        "ci_status": "success",
        "ci_branch": "main",
        "ci_fetched_at": "2026-09-12T09:14:22Z",
        "ci_fetched_at_unix": 1789204462,
        "last_run_url": "https://github.com/orapli/git-dashboard-tui/actions/runs/1",
        "pr_state": "ready",
        "open_prs": 1,
        "pr_fetched_at": "2026-09-12T09:14:22Z",
        "pr_fetched_at_unix": 1789204462
      }
    }
  ]
}
```

## Configuration

Settings are plain JSON, written atomically, and shared with the `git-dashboard` GUI app:

- **macOS**: `~/Library/Application Support/com.git-dashboard.git-dashboard/`
- **Linux**: `~/.config/git-dashboard/`
- **Windows**: `%APPDATA%\git-dashboard\git-dashboard\config\`

| File | Contents |
|---|---|
| `config.json` | Registered repositories: name, path, group |
| `members.json` | Team members and the commit-author aliases that merge into them |
| `prefs.json` | Language, theme, sidebar, diff toggles, recent comparisons, sort order, auto-refresh interval, external diff command, editor command/wait, user-defined work-tool commands, onboarding dismissal, worktree notes/favorites |
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

### Custom work-tool commands

Press `x` in Settings, or `x` inside the `O` tool menu, to keep up to six commands of your own
alongside the shell, editor, lazygit and GitUI entries. Each has a label, a command line and a
wait flag (`w` — on for a program that takes over the terminal, off for a GUI). They appear in
the `O` menu as `1` – `6`.

A command may use `{path}` (the repository or worktree root, and the child's working directory),
`{file}` (absolute path of the file on screen), `{line}` (1-based line the diff is showing),
`{branch}` and `{hash}`. **A placeholder with no value in the current view cancels the launch and
names the one that is missing**, rather than substituting an empty string: a dropped argument
shifts the next one into its place, and `""` makes the tool open the wrong thing quietly. A
template that names no placeholder at all is handed the repository root as its last argument,
which is what the default `code` has always done.

Every substituted value is text from a repository the dashboard merely registered, so a value
containing a control character, or one that would start an argument with `-`, is rejected, and a
placeholder can never become the program name. Commands are split into a program and arguments
rather than run through a shell, so aliases, `~`, `$HOME`, pipes and redirections are not
expanded. As with the external diff tool, the program is resolved from `PATH` or given as an
absolute path. Unknown `{...}` text is left alone, so a template may contain braces of its own.

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
