---
type: Quality Guide
title: Testing, CI, releases, and documentation automation
description: Focused Rust validation, real-Git integration coverage, cross-platform CI, release artifacts, documentation gates, and manual OpenWiki updates.
tags: [testing, ci, releases, documentation]
openwiki:
  roles: [delivery, operations, testing]
  change_kinds: [release, documentation-automation]
  source_paths: [.github/workflows/ci.yml, .github/workflows/release.yml, .github/workflows/openwiki-update.yml, docs/check_docs.py]
  test_paths: [tests/integration_tests.rs, docs/test_check_docs.py]
  validation_commands: [cargo test --all-targets, python3 docs/check_docs.py]
---

# Testing, CI, releases, and documentation automation

## Test layers

- Module unit tests: `src/config.rs`, `src/colors.rs`, `src/git/tests.rs`, and `src/app/tests.rs` validate parsing, safety, pure helpers, state transitions, and persistence guards without a terminal.
- Integration tests: `tests/integration_tests.rs` builds temporary actual Git repositories and asserts summary/dirty state, branches/tags, graph ANSI isolation, diff/root-commit fallback, blame/activity, stash, Finder, worktrees, conflicts, rebase cleanup, terminal-control sanitization, and tab-indented diff display. This is necessary because parser-only tests cannot prove the invoked Git version produces, for example, worktree porcelain, graph coloring, or conflict/rebase state as expected.
- Executor caveat: `src/git/tests.rs:test_run_with_timeout_kills_hung_process` is Unix-gated. The CI matrix still tests all targets on Windows, but that particular process-kill assertion is not a Windows coverage claim.

Use the narrowest command first: `cargo test --lib git::tests` for Git parser/executor work; `cargo test --lib app::tests` for state/worker work; `cargo test --test integration_tests` for public behavior that must work with Git. Full validation is:

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
```

## CI and release contract

`.github/workflows/ci.yml` runs on pushes and pull requests to `main`, on Ubuntu x86_64/ARM64, macOS, and Windows. Each matrix entry installs stable Rust plus clippy/rustfmt, then runs formatting, clippy with warnings denied, and all-target tests. A separate Documentation job runs `python3 docs/check_docs.py` and its gate tests under Python 3.12. The MSRV job installs Rust 1.88.0 and runs `cargo check --locked --all-targets`. Preserve cross-platform behavior when changing process paths or shell code.

`.github/workflows/release.yml` triggers on `v*` tags and has `contents: write`. It builds release binaries for Linux x86_64/aarch64, macOS x86_64/aarch64, and Windows x86_64; Unix artifacts are tarballs and Windows is zip, uploaded using `softprops/action-gh-release`. The checksum job combines per-target SHA-256 files into the published `SHA256SUMS` consumed by `install.sh`. The installer downloads the matching Unix release, verifies it when checksums and a SHA-256 tool are available, runs `--version` or `--help` as a pre-install smoke check, and never invokes `sudo`; Windows users download the zip directly. Treat a release/installer change as a public distribution boundary and validate its artifact naming and checksum path, not only Rust compilation.

## Documentation and OpenWiki automation

`docs/check_docs.py` is the repository documentation gate. It checks generated manual output, local links and anchors, SVG assets, and English/Japanese language structure; `docs/test_check_docs.py` tests failure cases for that gate. For a documentation-only change, run `python3 docs/check_docs.py` with Python 3.11 or later; preview a changed generated manual in a browser when its source or screenshots change.

`.github/workflows/openwiki-update.yml` is **manual only** (`workflow_dispatch`). Its comments explain that the configured `openai-chatgpt` provider has no unattended browser-login equivalent; do not reintroduce a schedule until CI credentials and the referenced tracing/connector secrets are actually configured. The workflow checks out full history because update mode compares against the previously documented commit, uses Node 22, installs pinned `openwiki@0.3.3`, Mermaid, and jsdom, then invokes `openwiki code --update --print`. Its generated PR is limited to `openwiki`, `AGENTS.md`, `CLAUDE.md`, and the workflow itself. It references secret-backed LangSmith environment variables; documentation must never expose their values.

When changing a workflow, validate YAML and retain its permissions/trigger/path constraints unless the product requirement changes. The application architecture and process safety rationale are in [architecture](../architecture/overview.md).
