# git-dashboard-tui

**Find your next task across repositories, from one terminal.**

[日本語](README.ja.md) · [User manual](https://orapli.github.io/git-dashboard-tui/manual.html) · [Keyboard and configuration reference](docs/reference.md)

See uncommitted changes, sync differences and failing CI together, inspect a diff,
and open your usual work tools. Includes cross-repository commit search,
contributor analysis and a Worktree workspace.

v0.5.0 makes refreshing a 30-repository dashboard about sixty times faster, stops the
table reporting confident answers it does not have, reads out as JSON for a script or a
shell prompt, and hands the file and line you are looking at to the tool you open next.

![Import → attention → diff → shell walkthrough](docs/img/quick-tour.svg)

This README and the published manual describe **v0.5.0**. The installer downloads the latest release.
This includes first-run guidance, Home freshness context, the `O` tool menu, the `W` Worktree workspace,
untracked-file diffs and cancellable background discovery. See the [changelog](CHANGELOG.md).

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
curl -fsSL https://raw.githubusercontent.com/orapli/git-dashboard-tui/main/install.sh | VERSION=v0.5.0 sh
```

> Piping a script into a shell means trusting it. The script is short and dependency-free —
> read it first if you prefer: [`install.sh`](install.sh), or
> `curl -fsSL https://raw.githubusercontent.com/orapli/git-dashboard-tui/main/install.sh | less`

For Windows x64, extract `git-dashboard-tui-x86_64-pc-windows-msvc.zip` from
[Releases](https://github.com/orapli/git-dashboard-tui/releases/latest) and place the executable
on PATH. Linux (x86_64 / ARM64) and macOS (Intel / Apple Silicon) archives are available there too.

To install development main, use **Rust 1.88 or later** and a native build environment:

```bash
cargo install --git https://github.com/orapli/git-dashboard-tui --branch main --locked
```

Required: `git` on PATH. GitHub PR/CI information needs authenticated `gh`;
SSH registration needs `ssh` with working key authentication.
See the [support matrix](https://orapli.github.io/git-dashboard-tui/manual.html#platforms).

## Quick start

```bash
git-dashboard-tui
```

1. Press `A`, enter a work folder, select with `Space`, and register with `Enter`. Use `a` for one repository.
2. On Home, `n` filters attention items. `Enter` opens details; select a Status file and press `Enter` for its diff.
3. `O` opens the editor/lazygit/GitUI menu; `t` opens a shell. Use Home `W` to work in a specific worktree.
4. `?` shows help for the current screen, `Esc` goes back, and `q` quits.

Viewing status is observation-first. Explicit `pull`, `fetch`, `stash apply` and `stash drop`
actions change repositories. `pull` may merge or rebase according to your Git configuration.
Use your usual tools for staging, committing and pushing.

## Learn more

| Task | Guide |
|---|---|
| Start work and inspect changes | [Task walkthroughs](https://orapli.github.io/git-dashboard-tui/manual.html#tasks) |
| Understand attention, unverified data, CI and caching | [Status and refresh](https://orapli.github.io/git-dashboard-tui/manual.html#freshness) |
| Configure VS Code, Neovim and other editors | [Editor examples](https://orapli.github.io/git-dashboard-tui/manual.html#editor-examples) |
| Resolve authentication, refresh and launch problems | [Troubleshooting](https://orapli.github.io/git-dashboard-tui/manual.html#troubleshooting) |
| Look up keys and configuration files | [Reference](docs/reference.md) |

## Development and resources

[Developer guide](CLAUDE.md) · [Documentation generation and checks](docs/README.md) ·
[Changelog](CHANGELOG.md) · [Security policy](SECURITY.md) ·
[Issues](https://github.com/orapli/git-dashboard-tui/issues)

The generated [OpenWiki](openwiki/index.md) is optional context. Source and tests are authoritative;
check its [generation metadata](openwiki/.last-update.json) for freshness.

## License

[MIT](LICENSE)
