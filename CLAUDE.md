# CLAUDE.md (Git Dashboard TUI Developer Guide)

Handover notes for Claude Code, other AI assistants, and human developers.
For user-facing feature overview and setup, see [README.md](README.md).

## 🚀 Command Reference

* **Check / static analysis**: `cargo check` / `cargo clippy`
* **Debug build & run**: `cargo run`
* **Release build & run**: `cargo run --release`
* **Run tests**: `cargo test`

Verification steps after any change: `cargo fmt` → `cargo clippy --all-targets` (zero warnings)
→ `cargo test` → `cargo run`.
CI (`.github/workflows/ci.yml`) runs `fmt --check` / `clippy -D warnings` / `test`, so
**a commit that doesn't pass fmt and clippy will fail CI**.

## 📄 Document Map

| File | Content |
|---|---|
| `README.md` | User-facing: features, setup, keybindings |
| `CLAUDE.md` (this file) | Developer/AI-facing: commands, structure, implementation rules |

## 📂 Project Structure and Responsibilities

```
git-dashboard-tui/
├── Cargo.toml          # Project dependencies (ratatui, crossterm, serde, chrono, etc.)
├── locales/            # i18n locale definitions (en.yml, ja.yml)
├── src/
│   ├── main.rs         # Binary entry point (calls git_dashboard_tui::run)
│   ├── lib.rs          # Event loop, terminal setup, external diff runner
│   ├── app.rs          # Application state, keybindings, background job worker
│   ├── ui.rs           # Ratatui rendering (Home, Repo, Diff, Settings, Help, Log)
│   ├── colors.rs       # Color palette definitions (Catppuccin Mocha theme)
│   ├── config.rs       # Config file read/write (repositories, members, preferences)
│   ├── git.rs          # Git command execution and parsing
│   ├── i18n.rs         # i18n helper (rust-i18n)
│   └── syntax.rs       # Lightweight syntax tokenizer
```

Config files (config.json / members.json / prefs.json) are shared with `git-dashboard` under the OS config directory
(macOS: `~/Library/Application Support/com.git-dashboard.git-dashboard/`).

## 🎨 TUI Implementation Rules

* **Colors**: Styled via `Palette` (`colors.rs`). Consistent accent, subtext, and status colors.
* **Non-blocking operations**: Git commands are run asynchronously in worker threads via `Job` / `AsyncMessage` channels.
* **Keyboard navigation**: Standard vim-style (`j`/`k`, `/`, `enter`, `esc`, `q`) and number keys for tabs.
* **Config file writes must use `config::write_atomic`** (never `fs::write` directly).

<!-- OPENWIKI:START -->

## OpenWiki

See [AGENTS.md](AGENTS.md) for OpenWiki agent instructions.

<!-- OPENWIKI:END -->
