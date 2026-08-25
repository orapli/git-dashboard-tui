---
type: workflow
title: Contributor analytics and cross-repository search
description: Alias-aware contribution aggregation and bounded commit-message search across registered repositories.
tags: [workflow, analytics, contributors, search]
---

# Contributor analytics and cross-repository search

This workflow intentionally runs on the secondary worker because it makes at least one Git call per registered repository. It is entered from Home as Global Members (`M`) or Commit Search (`S`), with `Job::LoadGlobalMembers` and `Job::SearchCommits` defined in `src/app/types.rs`.

## Contributors

`config::Member` provides a canonical name, aliases, and active flag. `process_contributor_log` resolves raw author name or email case-insensitively against this registry, aggregates counts/first/last timestamps/email set, calculates share of commits, then sorts by commit count. `get_contributors` asks Git for at most 50,000 author records and uses `TimeSpan` (`All`, one week/month/three months) as a Git `--since` filter.

`compute_global_members` first includes every registered member—even with zero commits—then folds each successfully read repository into `GlobalMember` and `MemberRepoContribution`. Contributions sort by count; members sort active first, then total commits and latest date. Failures in a repository are currently omitted from this aggregate rather than represented as a per-repository error.

`get_activity` and `build_activity` produce daily, hourly, and weekly buckets. The implementation drops author timestamps in the future or outside the selected window because Git’s `--since` uses a different date notion than `%at`.

## Commit search and navigation

`log::search_commits` does literal, case-insensitive matching across all refs. `search_commits_across` adds repository index/name context, retains up to 50 hits from each repository, sorts newest date first, caps the merged result at 300, and keeps failed repository names separate from legitimate zero matches. `Msg::CommitSearchLoaded` shows usable hits while surfacing those failures.

A hit carries both `repo_index` and `repo_name`; navigation verifies that the current repository list still has the same name at that index before opening its diff. This prevents an asynchronous result from targeting a repository removed or reordered after search started.

## Tests and changes

Use `src/git/tests.rs` for alias, time-bucket, and log parsing behavior; `src/app/tests.rs` for aggregation/search and stale navigation; `tests/integration_tests.rs` for contributor/blame activity through real Git. Run `cargo test --lib` during changes, followed by `cargo test --all-targets` for behavior involving repository fixtures. Adding a member field requires its config serde schema, Settings editing path, aggregation mapping, and persistence tests.
