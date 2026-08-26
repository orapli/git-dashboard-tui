---
type: documentation-plan
title: Documentation impact plan
description: Evidence-backed maintenance plan for changes from c1d6a90f35fe12745f1e31d98fc90ddad5a11d82 through b91de57ddabbf772e47e6f1df6a0f5161496d635.
tags: [planning, maintenance]
openwiki:
  roles: [repository, workflow]
---

# Documentation impact plan

## Affected systems

| Source change | Documentation target and section | Disposition | Primary evidence |
|---|---|---|---|
| Home row cache, sortable headers, dynamic Home/tab widths | `workflows/dashboard-and-repositories.md` — registration/filtering/refresh; `presentation/ui-and-localization.md` — Home geometry | covered; update both | `src/app/mod.rs`, `src/app/helpers.rs`, `src/app/handlers.rs`, `src/app/types.rs`, `src/app/worker.rs`, `src/ui.rs`, `src/config.rs`, `src/app/tests.rs` |
| Compare base/target marker selection and persistent hint/help | `application/navigation.md`; `git/history-and-diffs.md`; presentation page | covered; update | `src/app/handlers.rs`, `src/app/mod.rs`, `src/app/types.rs`, `src/ui.rs`, `src/app/tests.rs` |
| Pull/fetch/refresh progress indicators and stable sort behavior | `application/background-work.md`; dashboard workflow; presentation page | covered; update | `src/app/mod.rs`, `src/app/types.rs`, `src/ui.rs`, `src/app/tests.rs` |
| Terminal safety against repository content | `architecture/overview.md`; `git/engine.md`; `git/history-and-diffs.md` | covered; update | `src/app/mod.rs`, `src/git/exec.rs`, `src/git/diff.rs`, `src/git/log.rs`, `tests/integration_tests.rs` |
| 0.2.0–0.3.0 releases, installer, checksums, ARM64 artifacts and Node 24 CI | `quality/testing-and-ci.md`; quickstart validation/install routing if needed | covered; update | `Cargo.toml`, `install.sh`, `.github/workflows/ci.yml`, `.github/workflows/release.yml`, `README.md` |

## Designed relationships

- Dashboard lifecycle -> queues asynchronous reloads and cache-backed detail opening -> [background work](application/background-work.md).
- Background work -> publishes loading state consumed by -> [terminal presentation](presentation/ui-and-localization.md).
- Navigation compare marker -> supplies range selection to -> [history and diff inspection](git/history-and-diffs.md).
- External terminal handoff -> must not execute repository-provided content and shares command hardening rationale with -> [Git execution engine](git/engine.md).
- Release automation -> distributes the application and is validated through -> [testing and CI](quality/testing-and-ci.md).

## Scope decisions

- Existing OpenWiki automation remains accurate except its Node runtime and pinned OpenWiki version; document only current workflow facts.
- Uncommitted `AGENTS.md` and `docs/` are outside the documented Git range and are not source evidence for this update.
- No area is evidence-blocked or deferred.

## Reconciliation after drafting

- Home cache/sort, mouse viewport, activity lifecycle, terminal-output safety, tab display, CLI startup, installer, release checksums, and CI matrix/action-runtime changes have substantive coverage in the planned canonical pages.
- The implementation and focused test locations are reachable from the revised quickstart routing table.
- No additional one-hop dependency exposed an undocumented independent system; no backlog entry is required.
