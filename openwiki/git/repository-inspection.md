---
type: domain-api
title: Repository inspection and enrichment
description: Summary, files, technology, worktree, and GitHub metadata derived from a registered repository.
tags: [git, repository-analysis, github, worktrees]
---

# Repository inspection and enrichment

`get_summary` in `src/git/status.rs` is the mandatory analysis behind Home rows and `RepoSnapshot`. It gets a display name, canonical local path or untouched SSH locator, branch/commit/file counts, remote/upstream/ahead-behind state, dirty count, contributor count, operation state, conflicts, and optional GitHub data. It batches thirteen Git reads so SSH analysis takes one connection.

## Summary invariants

`count_conflicts` recognizes Git porcelain's seven unmerged XY states. `GitOpState` is observation-only: merge, rebase, cherry-pick, and revert were started outside this application. Local rebase detection checks resolved worktree-aware `rebase-merge`/`rebase-apply` directories, because `REBASE_HEAD` can remain after a successful continue; SSH falls back to that imperfect pseudo-ref heuristic. Optional GitHub enrichment cannot fail the core summary.

`get_github_status` only runs when `remote -v` mentions `github.com`. It invokes `gh pr list` and `gh run list` separately, each with a two-second timeout, and returns `RemoteCiPrInfo` only when at least one datum exists. The dashboard’s attention filter treats failure, failed, cancelled, timed_out, and action_required as failure; successful/passing is good and other states are neither.

## Additional metadata APIs

- `get_files_report` parses `ls-tree --long` instead of stat-ing local files; it groups extensions and returns the ten largest blobs.
- `find_readme` tries fixed README names. Local reads are byte-capped then character-capped; SSH reads `HEAD:name` through Git.
- `detect_tech_info` loads or creates `tech_rules.json`, falls back to built-in Rails, Node.js, and Django rules, and reads matching manifest files with a 1 MiB cap. It is local-filesystem oriented; SSH technology detection cannot inspect a remote working tree.
- `get_worktrees` parses `git worktree list --porcelain` into `WorktreeInfo`, preserving bare, detached, locked, and prunable flags and unquoting paths/branches.

The `Summary`, `RemoteCiPrInfo`, `TechRule`, `FilesReport`, and `WorktreeInfo` schemas are in `src/git/types.rs`. Consumers are `worker::load_home_row`, `worker::load_repo`, and Home/Repo rendering described in [dashboard workflows](../workflows/dashboard-and-repositories.md).

## Tests and extension points

Parser tests in `src/git/tests.rs` cover porcelain conflict counting, worktree parsing, GitHub JSON parsing, capped reads, and tech rules. Integration tests create temporary repositories to verify summary dirty state, conflicts, rebase cleanup, and worktrees. Run `cargo test --lib git::tests` plus `cargo test --test integration_tests`. When adding a summary field, add it to `Summary`, batch command/parsing, Home or snapshot consumer, serialization default when cache-compatible, and a parser/integration assertion.
