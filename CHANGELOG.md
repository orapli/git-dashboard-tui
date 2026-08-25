# Changelog

All notable changes to this project are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0] - 2026-08-26

First release. Everything below shipped in it — the repository had no tagged
release before this, so the hardening and fixes listed here were applied
before any version was published rather than in response to a shipped bug.

### Added

- Pre-built binaries for **Linux (x86_64, ARM64)**, **macOS (Apple Silicon,
  Intel)**, and **Windows (x64)**. The arm64 Linux binary is built natively on
  an arm64 runner, and CI runs the full test suite on arm64 Linux too, so that
  target is tested rather than only built.

- **Cross-repository commit search** (`S` from Home). Searches commit messages across every
  registered repository at once — case-insensitive literal substring, not a regex — and
  `Enter` jumps straight into the matching commit's diff. Results are capped at 50 hits per
  repository and 300 overall, newest first.
- **"Needs attention" filter** (`n` on Home). Narrows the list to repositories with a failing
  CI run, unresolved conflicts, or an interrupted merge/rebase/cherry-pick/revert.
- **Interrupted-operation and conflict detection.** Home rows and the Status tab now show
  `⚠MERGE` / `⚠REBASE` / `⚠CHERRY-PICK` / `⚠REVERT` badges plus an unresolved-conflict count.
- **Blame gutter in the diff view** (`b`). Shows short hash and author per line. Uncommitted
  lines are attributed to `Not Committed Yet` instead of being misattributed to whoever last
  touched the line.
- **Ignore-whitespace (`w`) and full-file-context (`f`) diff toggles.** These preferences
  existed in `prefs.json` but had never been wired to anything.
- **Theme switching** (`T` in Settings) between Catppuccin Mocha and Latte. The `theme`
  preference was likewise previously dead configuration.
- **Optional periodic Home auto-refresh** (`i` in Settings): off / 30s / 1m / 5m. Off by
  default, because each cycle re-queries the GitHub API once per repository.
- Root commits can now be diffed — previously they rendered an empty file list with no
  explanation, since `<commit>^..<commit>` is unresolvable for a repository's first commit.

### Changed

- **Commit graph colouring now follows git's own per-lane colours** instead of colouring by
  glyph type, so concurrent branches stay visually distinct across rows. Handles all 12 lane
  colours git cycles through (6 hues × bold) and declines to guess on 256-colour/truecolor
  `log.graphColors` values rather than mapping them to an unrelated hue.
- Remote operations *and* cross-repository fan-out work (commit search, member aggregation)
  now run on a dedicated worker, so a slow `pull` or an unreachable SSH host can no longer
  block the diff and repository loads you're waiting on.
- Repository size and file counts come from `git ls-tree -r --long` rather than one
  `fs::metadata` call per tracked file — removing up to hundreds of thousands of syscalls per
  refresh on a monorepo, and making sizes work for `ssh://` repositories for the first time.
- Superseded Home refreshes are now dropped at the worker rather than analysed and discarded,
  and a completed pull/fetch refreshes only its own row instead of triggering a blanket
  refresh (a bulk pull across 50 repositories previously queued thousands of analyses).
- CI status classification is now shared by all three call sites and treats `cancelled`,
  `timed_out`, and `action_required` as needing attention, not just `failure`/`failed`.

### Fixed

- **A repository could execute code on your machine simply by being registered.** Every git
  invocation now neutralises `core.fsmonitor`, `core.sshCommand`, `uploadpack.packObjectsHook`,
  and `protocol.ext.allow`, and passes `--no-ext-diff --no-textconv` so `diff.external` and
  `textconv` drivers can't fire. Git's `safe.directory` does not cover this case.
- **An `ssh://` locator whose host began with `-` was passed to `ssh` as an option**
  (e.g. `-oProxyCommand=…`), which is arbitrary command execution. Hosts are now validated.
- **Repository content could forge the SSH batch delimiter**, shifting every subsequent
  command's output by one. The delimiter is now an unpredictable per-call nonce, and a
  boundary-count mismatch fails the batch instead of silently truncating.
- **The "hard" command timeout could hang forever** when a grandchild process kept a pipe
  open; output is now collected with a bounded grace period instead of an unbounded join.
- Several panics that would re-fire on every redraw and make the app unrecoverable: byte-index
  slicing of a non-ASCII `.git/HEAD` or worktree `HEAD`, an out-of-frame prompt dialog on very
  small terminals, and a multi-byte line reaching the conflict counter over SSH.
- **A corrupt config file no longer destroys your data.** A parse error previously read as an
  empty repository list, which the next save wrote back over the real one. Loading now reports
  the error and writes to that file are refused for the rest of the session.
- Deleting a repository no longer leaves `repo_index` pointing at a different one, which could
  write one repository's cached snapshot into another's cache file.
- An in-progress rebase no longer appears stuck forever: `REBASE_HEAD` is left behind by git
  after `rebase --continue` succeeds, so detection now checks `.git/rebase-merge`/`rebase-apply`.
- Cross-repo search distinguishes "no matches" from "the search failed" (unreachable host,
  missing `git`, timeout) instead of reporting both as an empty result.
- Column widths are now measured in terminal columns rather than characters, so CJK text no
  longer overflows and pushes neighbouring fields off the row.
- The syntax tokenizer no longer injects a zero-width space into Python triple-quoted strings.
- Attention badges render before PR/CI text so they survive column truncation.
- Blame gutter no longer breaks under line wrapping, and the current-hunk highlight extends
  across the whole row.
- Toggling ignore-whitespace or full-file context keeps your scroll position instead of
  jumping back to the first hunk.
- `cargo test` no longer overwrites the real (GUI-shared) `prefs.json`.

### Security

See [SECURITY.md](SECURITY.md) for the current threat model. The fixes above marked as
execution or forgery issues were found by an internal review of this codebase, not reported
externally; no released version was affected, as none of this had shipped.

### Documentation

- README rewritten with a task-oriented structure, real terminal output, an explicit scope
  statement, and corrected keybindings — `A`/`F` were both documented as opening the
  Repository Finder, but `F` is bulk-fetch; the Worktrees `t` binding and the Home sort order
  were also wrong, and the entire Settings/Search/Members/Finder key tables were missing.
- Added [`README.ja.md`](README.ja.md), [`SECURITY.md`](SECURITY.md), and this changelog.
- Added a generated [OpenWiki](openwiki/index.md) engineering index, refreshed by a
  manual-only GitHub Actions workflow.

### Added (initial feature set)

- Multi-repository Home dashboard: branch, ahead/behind, dirty count, last commit, and
  repository groups with filtering and bulk pull/fetch.
- Per-repository tabs: Status, Commits, Branches, Tags, Stash, Contributors, Worktrees.
- Commit graph visualisation with Catppuccin colours and branch/tag badges.
- Split/unified diff viewer with token-level syntax highlighting and hunk navigation.
- Full Git Worktree support, including opening a shell inside a worktree.
- GitHub integration via the `gh` CLI: open PR counts and CI status badges.
- Cross-repository contributor analytics with member alias merging and time-span filtering.
- Interactive Repository Finder for scanning a folder and batch-importing repositories.
- `ssh://` remote repository support, with commands batched over a single SSH connection.
- Shell jump (`t` / `Shift+T`), full mouse support, and bilingual UI (English / Japanese).
- Integration test suite running against real temporary Git repositories, plus CI on Linux,
  macOS, and Windows.

[Unreleased]: https://github.com/orapli/git-dashboard-tui/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/orapli/git-dashboard-tui/releases/tag/v0.1.0
