# Changelog

All notable changes to this project are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

A responsiveness and trust release: refreshes stop blocking the interface, the Home
table stops reporting confident answers it does not have, and the dashboard becomes
readable from outside its own terminal.

### Added

- **A command line.** `git-dashboard-tui PATH` starts focused on that repository without
  registering it: it appears beside your registered repositories, selected and marked
  `(not registered)`, and nothing is written to `config.json` — running it in a scratch
  clone no longer quietly grows your configuration, and the cross-repository views still
  see everything you track. A path that is already registered selects the existing row
  instead of adding a duplicate, and a path that is not a repository fails before the
  terminal is touched.
- **`--json` prints a status snapshot and exits**, so a script, a shell prompt, a status
  line or an agent can read what the dashboard already knows. It initialises no terminal,
  so it can be piped, and it makes no network calls — something shelling out every few
  seconds must not hammer the GitHub API — so CI and pull-request fields come from the
  cache the dashboard wrote and state when they are absent or stale rather than emitting
  a plausible zero. A repository whose path has vanished is reported failed with its
  error, not as a healthy one, and `ahead`/`behind` are `null` rather than `0` when there
  is no upstream. The document is an object with a schema name
  (`git-dashboard-tui.status-snapshot`) and version, so it can be extended without
  breaking whoever is parsing it. Exit status is `2` for an unparseable command line and
  `1` for a configuration or path that could not be read.
- **Commit search understands `author:`, `path:`, `since:` and `until:`.** "What did this
  person change last week" and "who touched this path" — most of what a cross-repository
  search is for — were previously unanswerable, because the search matched commit
  messages by substring and nothing else. The filters are tokens in the same one-line
  prompt and every other word is still message text, so `fix: auth` and `fix(auth):` keep
  working; values with spaces can be quoted, and quoting a whole token searches for it
  literally. `author:` is widened through `members.json`, so a canonical name also finds
  that person's aliases, matching what the Contributors and Global Members views already
  do.
- **`y` copies the identifier for whatever is selected** — a commit hash, a branch, a tag,
  a stash ref, a contributor's email, a file or repository path — so it does not have to
  be retyped into the shell you just jumped to. It is sent as an OSC 52 sequence, which
  many terminals ignore by default, so the confirmation says the sequence was emitted
  rather than claiming the clipboard was written.
- **Up to six work-tool commands of your own**, added and edited with `x` from Settings or
  from the `O` menu itself, listed there as `1`–`6` and sharing the built-in entries'
  wait flag and not-on-PATH marker.
- **Command templates carry where you were.** They took only `{path}`, the repository
  root, so pressing `O` then `e` while reading line 120 of a file opened the repository
  and left you to navigate again. `{file}`, `{line}`, `{branch}` and `{hash}` are now
  substituted as well. A placeholder with no value in the current view refuses the launch
  and says which one is missing, rather than substituting an empty string — dropping an
  argument can shift the next one into its place, and `""` makes the tool open the wrong
  thing quietly. Every substituted value is untrusted repository text, so control
  characters and values that would supply an argument's leading dash are rejected, and a
  placeholder can never become the program name.
- **The help screen leads with the screen you pressed `?` on**, then the global keys, then
  everything else under a divider. It was one flat 62-line list, so on a 44-row terminal
  the keys for the screen you were actually on were usually below the fold, behind four
  sections you had not asked about. Five screens the flat list never mentioned at all —
  the worktree view, commit search, the repository finder, the branch log and the tools
  menu — are now in it, along with a dozen bindings that were only in `docs/reference.md`.
- **The pull-request data the dashboard was already fetching is now used**: the pull
  request opened from the current branch, with its draft state and review decision, and
  how many pull requests are waiting on your review. Both feed the `n` filter, and every
  reason has a readable line in the selected-repository panel. Being behind upstream is
  deliberately *not* an attention reason: nearly every repository is behind something, and
  counting it would make the filter select almost everything.
- **Planning documents**: an independent evaluation of v0.4.1 with its measured baseline
  and post-fix numbers, and a phased plan for Jujutsu-backed repositories (unstarted),
  both linked from `docs/README.md`.

### Changed

- **A refresh no longer blocks the interface, and is roughly sixty times faster.**
  Three things were wrong at once. Every git invocation was charged a fixed 25 ms
  `try_wait` poll — 25.8 ms per call against ~0.5 ms of real work, on ~17 calls per
  repository per refresh — which is now an exponential backoff from 200 µs. Every Home
  row ran the 13-command batch built for the *detail* screen, including
  `rev-list --count HEAD`, `ls-tree -r --long HEAD` and a 50,000-commit contributor log,
  none of whose results a row uses; rows now run a query that asks only for what they
  show. And all of it shared one worker thread with every interactive job, so opening a
  diff queued behind every repository still being analysed; Home refresh now has its own
  small pool and the interactive lane is its own.
  Measured by driving the real binary under a PTY, before and after, on the same
  fixtures: 30 repositories, cold refresh **13.83 s → 0.23 s**; pressing `r` and
  immediately opening a repository **14.49 s → 0.17 s**; 10 repositories including one
  with 20,000 commits **4.96 s → 0.21 s**. Those fixtures have no GitHub remotes, so the
  figures measure local git work only — a cold refresh of repositories with GitHub
  remotes is still bounded by `gh` calls.
- **CI is the current branch's, not the repository's.** The indicator asked for the newest
  workflow run anywhere in the repository, so a colleague's failing branch raised the
  warning on your row and your own failure was missed whenever someone else pushed more
  recently — and because `needs_attention` was wired to it, the `n` filter manufactured
  false "needs attention" rows. The query is now scoped to the current branch, and a
  detached HEAD is its own state rather than a silent fall-back that pretends a
  repository-wide run belongs to your branch. A new timed-out state is distinguished from
  a plain failure.
- **`gh` gets ten seconds instead of two, with tiered retries.** A 2 s budget for `gh`
  startup plus a TLS handshake plus an API round trip turned healthy setups into
  intermittent "fetch failed", retried every 60 s forever. Retries now depend on the
  cause: one minute after a plain failure, three after a timeout (the request already
  spent its whole budget on a link that has just proven slow), and the full refresh period
  when `gh` is missing or unauthenticated, since a minute cannot fix either.
- **Opening a repository lands on Status when the working tree is dirty, conflicted or
  mid-operation**, and on Commits otherwise. You usually opened it *because* of the
  conflict or the seven uncommitted files shown in its Home row, and neither is visible on
  Commits. The choice is made once, at open, so a load finishing later cannot move the tab
  under you.
- **The Home `Path` column shows a home-relative, middle-elided path** instead of filling
  with a long common prefix that carried no information at ordinary widths.

### Fixed

- **A branch with no upstream no longer reads as being in sync.** The cell always
  formatted the two counters, so a branch that was never pushed showed the same `↑0 ↓0` as
  a fully synced one. It now reads `↑? ↓?`, with the reason — no remote, or no upstream
  branch — spelled out in the panel.
- **A registration that cannot be read is reported as a failure.** A registered path that
  no longer existed showed `…` in every cell forever, indistinguishable from still
  loading, with an unattributed OS error in the footer; a registered directory that was
  not a git repository rendered as a perfectly healthy repository, because every git
  command failed into an empty string. The row now says which kind of broken, the footer
  names the repository, the panel prints the full reason and how to fix it, and `n`
  surfaces it.
- **Dirty no longer merges staged, unstaged and untracked into one number.** Two real
  changes next to five build artefacts read as `7`; they now read `2M 5?`, using the same
  letters as the Status tab.
- **The age of `ahead`/`behind` is now shown.** Those counts are only as fresh as the last
  fetch and nothing said when that was. The panel reports the `FETCH_HEAD` time as the age
  of the remote data, separately from the time the working tree was read — which is what
  the existing line actually meant.
- **Worktree rows whose directory could not be inspected are counted as unknown.** Such a
  row showed `Dirty ?` while the header still read `errors: 0`, so it was
  indistinguishable from a loading row, a broken one and an empty one. Row failures now
  carry a reason, join the `[` / `]` issue inspector, and are counted as unknown rather
  than absorbed into a healthy count.
- **A truncated commit search no longer looks complete.** The per-repository cap of 50 and
  the overall cap of 300 were silent — the heading showed a count, so a trimmed result was
  indistinguishable from a full one. The heading now names the cap that was hit and how
  many repositories hit it. A query that fails to parse leaves the previous result on
  screen instead of blanking it.
- **A panicking background job no longer takes a worker thread with it.** There was no
  respawn, so an index panic in a parser reached by one repository's unusual git output
  degraded the pool 8 → 7 → 1 across successive refreshes, invisibly, and the row being
  analysed kept its spinner forever. Jobs now run inside `catch_unwind`, so the pool keeps
  its width and the row reports a failure and stops spinning.
- **A long-running command no longer wakes the process thousands of times.** The poll cap
  chosen for the one-millisecond case also governed the two-minute case: a 60 s fetch over
  a slow link woke up about 30,000 times across as many as ten polling threads. The cap is
  now two-tier — tight for the first 100 ms, relaxed after — which keeps the latency win
  for short commands and restores the old wakeup count for long ones.
- **A stale Home row can no longer overwrite a fresher one.** Two loads for the same row
  could be in flight at once — pull a repository while the periodic refresh is already
  reloading that row — and on a thread pool they finish in arbitrary order, so a pre-pull
  row could land on top of the post-pull one and sit there until the next full refresh.
  Each load now carries an order number and an older result for that row is dropped.
- **A refresh spinner could run for the rest of the session.** An activity marker
  belonging to a superseded refresh was never removed, because those jobs are dropped
  without producing a message: press `r` with three repositories and delete one before the
  refresh lands, and the title bar claimed a job was still running. Refresh markers are now
  retired with the refresh that owns them.
- Strings taken from the GitHub API are stripped of control characters and length-capped
  before they can reach the terminal, as git output already was.

## [0.4.1] - 2026-09-10

### Added

- Add mouse navigation buttons and a keyboard-accessible destination popup to the title bar.
- Allow Global Members and Commit Search rows to select and open with repeated clicks.

### Fixed

- Enable Settings repository/member row selection and align tab clicks with the rendered layout.
- Enable Repository Finder row and checkbox clicks, preserving registered repositories.
- Respect filtered list lengths when scrolling and block background mouse operations during dialogs.

## [0.4.0] - 2026-09-05

A multi-repository workspace release with clearer status, tool launch and cancellable discovery.

### Added

- Cancellable, incremental repository discovery on a dedicated worker, including folder inputs from single-repository registration.
- First-run repository import flow and English/Japanese product walkthrough.
- Home attention reasons, separate unverified-data counts, GitHub freshness and CI-run context.
- `O` menu for shell, editor, lazygit and GitUI, with editor command/wait preferences.
- `W` cross-repository local Worktree workspace with persistent notes and favorites.
- Task-oriented bilingual manual, editor examples, platform/SSH matrix and troubleshooting.
- Documentation checks for generated output, local links/anchors, SVG assets and language structure.

### Fixed

- Include untracked files in Status and Diff, including repositories without a first commit; preserve ignored files and the Git index, and surface file-list errors.
- Prevent inherited `NO_COLOR` from removing application colors in documentation captures; regenerate affected English/Japanese screenshots and tours, and detect monochrome assets in CI.
- Refresh the Diff file list after returning from external tools, preserving selection and scroll when possible.
- Refresh repositories registered through symlink/noncanonical paths after working in their canonical directory.
- Separate GitHub caches when a repository's remote configuration changes.
- Resolve native Windows `.exe`/`.com` programs for the editor and Git-client menu.
- Create missing configuration directories during atomic saves.
- Keep Worktree collection errors visible beside healthy rows, with `[` / `]` navigation.

### Changed

- Show distinct Worktree directory names in the cross-repository table while keeping the full selected path in its detail panel.
- Shortened READMEs; moved complete keyboard/configuration tables to `docs/reference*.md`.
- Declared Rust 1.88 as the minimum supported version and added an MSRV CI check.
  The previous Rust 1.85 claim did not match current language features and locked dependencies.

## [0.3.1] - 2026-08-27

### Added

- **An illustrated user manual**, in English and Japanese:
  [manual.html](https://orapli.github.io/git-dashboard-tui/manual.html) /
  [manual.ja.html](https://orapli.github.io/git-dashboard-tui/manual.ja.html). Every screen
  is a real screenshot of the running program — colour is most of how this tool conveys
  state, and a text-only description of "green when clean, red when dirty" conveys none
  of it. Screenshots are captured by driving the binary in a pty and writing the terminal
  buffer out as SVG, so they cannot drift from what the program draws, and both languages
  are generated from one content structure so they cannot drift from each other.

### Security

- **Repository content could drive the terminal.** Nothing sanitised what git
  returned, and ratatui writes strings to the terminal as given — so a line committed
  to a file could contain `ESC[2J` to clear the screen, SGR sequences to repaint
  arbitrary regions, or `ESC]52;c;<base64>BEL`, which writes the system clipboard on
  terminals that support OSC 52. Confirmed against a real repository: the escapes were
  emitted byte for byte. This is the threat model the tool already assumes elsewhere —
  a registered repository is untrusted, which is why `core.fsmonitor` and friends are
  neutralised — but content was not covered, and it is attacker-chosen too: file text,
  commit messages, branch names, author names. Escape sequences are now removed at the
  single point where git output becomes a string, so no call site can bypass it; other
  control characters become a visible mark rather than vanishing silently.

### Fixed

- **Tab-indented files broke the diff layout.** A tab reaching the terminal moves the
  cursor to the terminal's own next tab stop, which the layout knows nothing about, so
  any Go, Make or C file drew over the pane beside it. Tabs are expanded to 4-column
  stops wherever file content is displayed.

## [0.3.0] - 2026-08-26

### Added

- **The footer hint bar wraps onto a second row** when the hints don't fit one.
  Home's hints need 154 columns in English and 141 in Japanese, and the bar was a
  single hard-truncated row — so on any ordinary terminal the last third of them
  simply did not appear. This is why `o` (sort) read as a removed feature: putting it
  back in the hints was not enough, because the tail of the bar was off-screen.
  Two rows cover Home down to a 71-column terminal; below that the bar ends in
  `… ?` rather than quietly dropping the remainder.
- **The help screen scrolls** (`j`/`k`, PageUp/PageDown, `g`/`G`, mouse wheel) and its
  title shows the visible range. It is 59 lines rendered into whatever height the
  terminal had: on 30 rows, 33 of them could not be reached by any means.
- **Running operations are now visible.** A `pull`, `fetch` or row refresh replaces the
  repository's Sync cell with a spinner and the operation's name — that cell holds the
  value the operation is about to change — and the title bar shows a spinner with the
  number of jobs still running, on every screen, since work started from Home keeps
  going while you move elsewhere. The status line and the "analyzing"/"fetching diff"/
  "searching" screens carry the same spinner, so a message that has been sitting there
  is visibly still live rather than possibly wedged. A completed pull hands straight
  over to the row reload it triggers, so the indicator does not blink off in between.
- **Pick comparison targets in Commits and Tags by clicking the `[ ]` marker**, not only
  with `space`. Clicking a marked row again clears it, so a mis-picked base is fixable
  by mouse. Clicking a row anywhere else selects it, and clicking the already-selected
  row opens it — until now a click anywhere in a repository's list did nothing at all.

### Fixed

- **Home column headers no longer truncate themselves.** Column widths were fixed at
  values chosen for the English headers, so `未コミット` (10 columns) was cut to
  `未コミッ` in an 8-column cell — a chopped word with no ellipsis to admit it. Each
  column is now at least as wide as its own header, in any language.
- **All seven repository tabs stay visible on a narrow terminal.** The full labels need
  87 columns in English and 95 in Japanese; at 80 the bar was clipped, and in Japanese
  `7 ワークツリー` vanished entirely, leaving no sign the tab existed. Shorter labels
  are used when the full ones don't fit, and bare numbers below that.
- **Clicking a tab selects the tab under the cursor.** The handler matched fixed column
  ranges derived from the English labels, so in Japanese — where every label is a
  different width — a click could land on a neighbouring tab. It now uses the bounds
  the renderer recorded.
- **The help screen was almost entirely English** under a Japanese UI — only the four
  section headings went through translation. Every line does now.
- **Two section headings rendered as raw translation keys** (`global_members`,
  `repo_detail`) in both languages: the keys were referenced but never defined in
  `locales/`, and the i18n layer falls back to printing the key itself.
- **The Settings config-file path is elided from the middle** rather than chopped at
  the frame edge with nothing to signal it. The tail names the directory, so both ends
  are kept.
- The Contributors panel title no longer carries a stray trailing space before its
  border when the active-members filter is off.
- **The Commits and Tags comparison headers were English-only**, even with the UI set to
  Japanese: `base=abc1234  (space to pick target)` never went through translation. They
  are now bilingual, and say that the `[ ]` marker is clickable — "space to mark" gave no
  hint that the thing at the start of the row was a control.

## [0.2.1] - 2026-08-26

No change to the application itself — the binaries are the 0.2.0 ones rebuilt.

### Changed

- Every GitHub Actions step now runs on a Node 24 runtime. Node 20 is deprecated and
  was only still working because the runner force-upgraded it; that fallback goes away.
  Bumped `actions/checkout`, `actions/upload-artifact`, `actions/download-artifact`,
  `actions/setup-node`, `softprops/action-gh-release`, and
  `peter-evans/create-pull-request`. `Swatinem/rust-cache` already ran on Node 24, and
  `dtolnay/rust-toolchain` is a composite action with no Node runtime.

## [0.2.0] - 2026-08-26

### Added

- **The Home dashboard now opens populated instead of blank.** Each repository's Home
  row is cached under `cache/` and redisplayed at startup while the refresh runs in the
  background. Measured against four local repositories, the first frame previously showed
  `…` placeholders for ~1.9 s; it is now filled in immediately. The dominant cost is the
  per-repository `gh` call (~1.2 s of a ~1.8 s refresh), which the cache does not remove —
  it only stops the user waiting on it to see anything. Cached rows are replaced as soon
  as the real ones arrive, and a repository's cache file is deleted when it is removed.
- **Sort the Home list by clicking a column header.** The sorted column is marked ▲ / ▼.
  Clicking the active column reverses it; each column starts in the direction that is
  useful first (names ascending, but most-changed and most-recent first).
- **Four more sort orders**: by branch, by ahead/behind count, by uncommitted-change
  count, and by name/date as before — all reachable from `o` as well as from the header.
  Rows that have not loaded yet sort last in every mode rather than being compared as
  `0` or `""`, which used to scatter them through the list as the refresh landed.
  Modes 0-3 keep their existing meaning so `prefs.json` stays compatible with the
  sibling GUI, and an unrecognised mode written by it falls back to newest-first.

## [0.1.0] - 2026-08-26

First release. Everything below shipped in it — the repository had no tagged
release before this, so the hardening and fixes listed here were applied
before any version was published rather than in response to a shipped bug.

### Added

- **One-line installer** (`install.sh`): `curl -fsSL .../install.sh | sh` detects the
  platform, downloads the matching release binary, verifies its SHA-256 against the
  checksum file published with the release, and installs without ever using `sudo`.
  `INSTALL_DIR` and `VERSION` override the defaults.
- **`SHA256SUMS` published with each release**, so downloads (by the installer or by
  hand) can be verified.
- **`--version` / `--help` flags.** The binary previously ignored all arguments and
  tried to start the TUI, which panics when stdout is not a terminal — so
  `git-dashboard-tui --version` failed in scripts and CI. Unknown arguments now exit
  with status 2 and a pointer to `--help` instead of launching the dashboard.
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

[Unreleased]: https://github.com/orapli/git-dashboard-tui/compare/v0.4.1...HEAD
[0.4.1]: https://github.com/orapli/git-dashboard-tui/compare/v0.4.0...v0.4.1
[0.4.0]: https://github.com/orapli/git-dashboard-tui/compare/v0.3.1...v0.4.0
[0.3.1]: https://github.com/orapli/git-dashboard-tui/compare/v0.3.0...v0.3.1
[0.3.0]: https://github.com/orapli/git-dashboard-tui/compare/v0.2.1...v0.3.0
[0.2.1]: https://github.com/orapli/git-dashboard-tui/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/orapli/git-dashboard-tui/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/orapli/git-dashboard-tui/releases/tag/v0.1.0
