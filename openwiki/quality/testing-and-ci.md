---
type: quality-guide
title: Testing, CI, releases, and documentation automation
description: Focused Rust validation, real-Git integration coverage, cross-platform CI, release artifacts, and OpenWiki updates.
tags: [testing, ci, releases, automation]
---

# Testing, CI, releases, and documentation automation

## Test layers

- Module unit tests: `src/config.rs`, `src/colors.rs`, `src/git/tests.rs`, and `src/app/tests.rs` validate parsing, safety, pure helpers, state transitions, and persistence guards without a terminal.
- Integration tests: `tests/integration_tests.rs` builds temporary actual Git repositories and asserts summary/dirty state, branches/tags, graph ANSI isolation, diff/root-commit fallback, blame/activity, stash, Finder, worktrees, conflicts, and rebase cleanup. This is necessary because parser-only tests cannot prove the invoked Git version produces, for example, worktree porcelain, graph coloring, or conflict/rebase state as expected.
- Executor caveat: `src/git/tests.rs:test_run_with_timeout_kills_hung_process` is Unix-gated. The CI matrix still tests all targets on Windows, but that particular process-kill assertion is not a Windows coverage claim.

Use the narrowest command first: `cargo test --lib git::tests` for Git parser/executor work; `cargo test --lib app::tests` for state/worker work; `cargo test --test integration_tests` for public behavior that must work with Git. Full validation is:

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
```

## CI and release contract

`.github/workflows/ci.yml` runs on pushes and pull requests to `main`, on Ubuntu/macOS/Windows. Each matrix entry installs stable Rust plus clippy/rustfmt, then runs exactly formatting, clippy with warnings denied, and all-target tests. Preserve cross-platform behavior when changing process paths or shell code.

`.github/workflows/release.yml` triggers on `v*` tags and has `contents: write`. It builds release binaries for Linux x86_64, macOS x86_64/aarch64, and Windows x86_64; Unix artifacts are tarballs and Windows is zip, uploaded using `softprops/action-gh-release`.

## OpenWiki automation

`.github/workflows/openwiki-update.yml` runs manually and daily. It checks out full history because update mode compares to prior documentation history, uses Node 22, installs pinned `openwiki@0.3.3`, Mermaid, and jsdom, then invokes `openwiki code --update --print`. The subsequent PR is limited to `openwiki`, `AGENTS.md`, `CLAUDE.md`, and its workflow. It uses repository secrets for LangSmith-related environment variables; documentation must never expose their values.

When changing a workflow, validate YAML and retain its permissions/trigger/path constraints unless the product requirement changes. The application architecture and process safety rationale are in [architecture](../architecture/overview.md).
