---
type: Configuration Reference
title: Persistent configuration and caches
description: User-owned JSON schemas, safe writes, migration behavior, editor and worktree preferences, and per-repository cache boundaries.
tags: [configuration, persistence, json, caching]
openwiki:
  roles: [operations, domain]
  change_kinds: [persistence, compatibility]
  source_paths: [src/config.rs, src/app/worker.rs]
  symbols: [Preferences, WorktreeNote, write_atomic]
  test_paths: [src/config.rs, src/app/tests.rs]
  invariants: [A failed configuration load disables writes to that file for the process so corrupt user data is not replaced by defaults.]
  validation_commands: [cargo test --lib config::tests]
---

# Persistent configuration and caches

`src/config.rs` owns user state. Production `get_config_dir` uses `directories::ProjectDirs` with qualifier/organization/application `com` / `git-dashboard` / `git-dashboard`, creates it on demand, and migrates local `config.json`, `members.json`, and `tech_rules.json` only when first creating the destination. Test builds redirect all config access into a process-local temporary directory.

## Schemas and files

| File | Schema and owner |
|---|---|
| `config.json` | `Vec<Repository>`: display name, path, optional group. |
| `members.json` | `Vec<Member>`: canonical name, aliases, active status. |
| `prefs.json` | `Preferences`: onboarding state, theme/language, UI values, editor command/wait, worktree notes/favorites, diff toggles/command, sort, auto-refresh, and recent comparisons. |
| `tech_rules.json` | `Vec<TechRule>` used by local technology detection. |
| `cache/repo-*.json` / `cache/home-*.json` / `cache/tui-*.json` | hashes of repository paths; repository inspection, Home-row, and TUI-snapshot cache formats respectively live in `app/worker.rs`. |

`load_json` differentiates a missing file (`Ok(None)`) from a read or JSON parse error. `App::new` records which of repositories, members, and preferences failed to load and disables writes to that individual file for the process, avoiding replacement of corrupt user data with defaults. `write_atomic` writes a sibling process-and-counter unique temporary file then renames it.

## Compatibility constraints

`Preferences` has serde defaults. `prefs.json` is byte-for-byte shared with a sibling GUI application outside this repository; defaults let either app read fields it does not understand, but saving will drop unknown fields. This is documented in code as an out-of-scope cross-repository schema limitation, not a merge-preserving format.

`repo_cache_path`, `home_cache_path`, and `tui_cache_path` hash the repository path. `HomeRow` cache JSON is tolerant of missing fields through serde defaults and ignored when invalid; it is written after Home refresh and deleted with the repository. `TuiCache` requires version 1; invalid JSON or version mismatch is ignored. Its intentional exclusion of working-tree files prevents stale local-change display. See [background work](../application/background-work.md) for cache load/save timing and [inspection](../git/repository-inspection.md) for `tech_rules.json` creation/fallback.

## Safe changes and tests

Add a persisted field with serde defaults when old JSON must load, update `Default`, save/load callers, and add a roundtrip/default test. Preserve atomic write behavior and per-file failed-load guard. `src/config.rs` tests language serde, corruption handling, and defaults; `src/app/tests.rs` covers app-level persistence paths. Run `cargo test --lib config::tests` and `cargo test --lib app::tests`.
