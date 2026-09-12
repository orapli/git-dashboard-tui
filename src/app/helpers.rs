use super::types::*;
use crate::config::Repository;
use crate::git::{DiffRowKind, FileDiff, GitOpState};
use std::collections::HashMap;

/// First 7 characters of a commit id.
///
/// Slicing `&s[..7]` panics unless byte 7 is a char boundary, and the strings
/// this is applied to are not guaranteed to be hex: a corrupt `.git/HEAD`, or
/// a `git worktree list --porcelain` record whose path contained a newline,
/// both yield arbitrary text. In the render path such a panic re-fires on
/// every redraw, so the app cannot recover.
pub fn short_hash(s: &str) -> String {
    s.chars().take(7).collect()
}

/// Coarse classification of a GitHub Actions run's `conclusion` (or, while
/// still running, `status`) string, shared by every place that needs to
/// decide "is this red, green, or neither" instead of three separate
/// hand-rolled `contains(...)` checks that could silently drift apart.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CiOutcome {
    Success,
    /// Includes states a user would want surfaced as "needs a look", not
    /// just a literal test failure: a cancelled or timed-out run usually
    /// means something needs re-running, and `action_required` means a
    /// human has to do something before the run can even proceed.
    Failure,
    /// In progress, skipped, neutral, or any conclusion not recognised
    /// above — rendered as neither clearly-good nor clearly-bad.
    Other,
}

pub fn classify_ci_status(status: &str) -> CiOutcome {
    if status.contains("success") || status.contains("passing") {
        CiOutcome::Success
    } else if status.contains("failure")
        || status.contains("failed")
        || status.contains("cancelled")
        || status.contains("timed_out")
        || status.contains("action_required")
    {
        CiOutcome::Failure
    } else {
        CiOutcome::Other
    }
}

/// Whether this row's CI answer is a failure *on the branch it is checked
/// out on*.
///
/// The run list is queried scoped to the current branch, so normally the
/// answer is about that branch by construction. A row restored from the
/// on-disk cache can still carry the answer for the branch that was checked
/// out when it was written, and another branch's red run is not this
/// repository's problem — until the next refresh replaces it.
pub fn ci_failed_on_branch(row: &HomeRow) -> bool {
    let other_branch = row
        .github
        .as_ref()
        .and_then(|g| g.ci_branch.as_deref())
        .is_some_and(|b| !row.branch.is_empty() && b != row.branch);
    !other_branch
        && row
            .ci_status
            .as_deref()
            .is_some_and(|s| classify_ci_status(s) == CiOutcome::Failure)
}

/// How many open pull requests in this repository are waiting on the current
/// user's review. 0 when the lookup has never succeeded.
pub fn review_requests(row: &HomeRow) -> usize {
    row.github
        .as_ref()
        .and_then(|g| g.review_requests)
        .unwrap_or(0)
}

/// A reviewer asked for changes on the pull request opened from the branch
/// this repository is on.
pub fn branch_pr_changes_requested(row: &HomeRow) -> bool {
    row.github
        .as_ref()
        .and_then(|g| g.branch_pr.as_deref())
        .is_some_and(crate::git::BranchPr::changes_requested)
}

/// A repository the "n" (needs attention) filter should surface. Every one of
/// these is something a person has to *do* something about, and every one has
/// a matching reason line in the selected-repository panel — a flag the panel
/// cannot explain would be worse than no flag:
///
/// * a merge/rebase/cherry-pick/revert left mid-operation,
/// * an unresolved conflict,
/// * a failing CI run on the branch this repository is on,
/// * a pull request waiting on this user's review — someone else is blocked,
/// * changes requested on this branch's own pull request — the review came
///   back and the ball is with the author.
///
/// Deliberately excluded: plain uncommitted changes, and being behind
/// upstream — nearly every repository is behind something, so counting it
/// would leave the filter selecting almost everything. The Home summary
/// counts sync deltas separately. A *draft* pull request is excluded too
/// (being a draft is a choice, not a problem), and so is an approved one:
/// merging is a write operation this dashboard does not do.
pub fn needs_attention(row: &HomeRow) -> bool {
    row.op_state != GitOpState::None
        || row.conflicts > 0
        || ci_failed_on_branch(row)
        || review_requests(row) > 0
        || branch_pr_changes_requested(row)
}

pub fn filter_repo_indices(
    repos: &[Repository],
    query: &str,
    group_filter: Option<&str>,
) -> Vec<usize> {
    let mut idx: Vec<usize> = (0..repos.len()).collect();
    if let Some(gf) = group_filter {
        idx.retain(|&i| {
            let g = repos[i].group.as_deref().unwrap_or("").trim();
            if gf.is_empty() {
                g.is_empty()
            } else {
                g.eq_ignore_ascii_case(gf)
            }
        });
    }
    let q = query.trim().to_lowercase();
    if !q.is_empty() {
        idx.retain(|&i| {
            let r = &repos[i];
            r.name.to_lowercase().contains(&q)
                || r.path.to_string_lossy().to_lowercase().contains(&q)
                || r.group.as_deref().unwrap_or("").to_lowercase().contains(&q)
        });
    }
    idx
}

pub fn move_index(current: usize, len: usize, delta: isize) -> usize {
    if len == 0 {
        return 0;
    }
    let next = current as isize + delta;
    next.clamp(0, (len - 1) as isize) as usize
}

/// Clamp a selection cursor after the list it points into changed length.
/// Every screen keeps its cursor as a bare index into a list it does not own,
/// so each one has to be re-clamped whenever that list shrinks; doing it by
/// hand at each site is how a cursor ends up past the end and silently
/// disables Enter/delete until the user moves it.
pub fn clamp_index(current: usize, len: usize) -> usize {
    move_index(current, len, 0)
}

pub fn is_change_kind(k: &DiffRowKind) -> bool {
    matches!(
        k,
        DiffRowKind::Added | DiffRowKind::Removed | DiffRowKind::Modified
    )
}

pub fn is_hunk_start(lines: &[DiffLine], i: usize) -> bool {
    is_change_kind(&lines[i].kind) && (i == 0 || !is_change_kind(&lines[i - 1].kind))
}

pub fn collect_hunks(lines: &[DiffLine]) -> Vec<Hunk> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        if !is_hunk_start(lines, i) {
            i += 1;
            continue;
        }
        let start = i;
        i += 1;
        while i < lines.len() && is_change_kind(&lines[i].kind) {
            i += 1;
        }
        let end = i;
        let preview = lines[start]
            .text
            .trim()
            .trim_start_matches(['+', '-', ' '])
            .chars()
            .take(36)
            .collect::<String>();
        let line_no = lines[start].new_no.or(lines[start].old_no).unwrap_or(0);
        out.push(Hunk {
            start,
            end,
            label: format!("#{:<3} L{:<5} {preview}", out.len() + 1, line_no),
        });
    }
    out
}

pub fn apply_hunks(diff: &mut DiffView) {
    diff.hunks = collect_hunks(&diff.lines);
    diff.hunk_idx = 0;
    if let Some(h) = diff.hunks.first() {
        diff.scroll = h.start;
    } else {
        diff.scroll = 0;
    }
}

pub fn sync_hunk_from_scroll(diff: &mut DiffView) {
    if let Some(i) = diff.hunks.iter().rposition(|h| h.start <= diff.scroll) {
        diff.hunk_idx = i;
    }
}

pub fn flatten_diff(diff: &FileDiff) -> Vec<DiffLine> {
    if diff.is_binary {
        return vec![DiffLine {
            kind: DiffRowKind::Context,
            text: "Binary file".to_string(),
            old_no: None,
            new_no: None,
        }];
    }
    let mut out = Vec::new();
    for row in &diff.rows {
        match row.kind {
            DiffRowKind::Context => {
                let t = row
                    .right_text
                    .as_deref()
                    .or(row.left_text.as_deref())
                    .unwrap_or("");
                out.push(DiffLine {
                    kind: DiffRowKind::Context,
                    text: format!(" {t}"),
                    old_no: row.left_no,
                    new_no: row.right_no,
                });
            }
            DiffRowKind::Removed => {
                out.push(DiffLine {
                    kind: DiffRowKind::Removed,
                    text: format!("-{}", row.left_text.as_deref().unwrap_or("")),
                    old_no: row.left_no,
                    new_no: None,
                });
            }
            DiffRowKind::Added => {
                out.push(DiffLine {
                    kind: DiffRowKind::Added,
                    text: format!("+{}", row.right_text.as_deref().unwrap_or("")),
                    old_no: None,
                    new_no: row.right_no,
                });
            }
            DiffRowKind::Modified => {
                if let Some(t) = &row.left_text {
                    out.push(DiffLine {
                        kind: DiffRowKind::Removed,
                        text: format!("-{t}"),
                        old_no: row.left_no,
                        new_no: None,
                    });
                }
                if let Some(t) = &row.right_text {
                    out.push(DiffLine {
                        kind: DiffRowKind::Added,
                        text: format!("+{t}"),
                        old_no: None,
                        new_no: row.right_no,
                    });
                }
            }
        }
    }
    if diff.truncated {
        out.push(DiffLine {
            kind: DiffRowKind::Context,
            text: "… truncated".to_string(),
            old_no: None,
            new_no: None,
        });
    }
    out
}

/// Sort modes for the Home table, stored in `Preferences::repo_sort`.
///
/// Values 0-3 are fixed by history: `prefs.json` is shared with the
/// `git-dashboard` GUI application, which knows only those four, so their
/// meaning must not change. New columns are appended instead.
pub const SORT_NAME_ASC: usize = 0;
pub const SORT_NAME_DESC: usize = 1;
pub const SORT_UPDATED_DESC: usize = 2;
pub const SORT_UPDATED_ASC: usize = 3;
pub const SORT_BRANCH_ASC: usize = 4;
pub const SORT_BRANCH_DESC: usize = 5;
pub const SORT_DIRTY_DESC: usize = 6;
pub const SORT_DIRTY_ASC: usize = 7;
pub const SORT_SYNC_DESC: usize = 8;
pub const SORT_SYNC_ASC: usize = 9;

/// Every sort mode, in the order `o` cycles through them.
pub const SORT_CYCLE: [usize; 10] = [
    SORT_NAME_ASC,
    SORT_NAME_DESC,
    SORT_UPDATED_DESC,
    SORT_UPDATED_ASC,
    SORT_BRANCH_ASC,
    SORT_BRANCH_DESC,
    SORT_DIRTY_DESC,
    SORT_DIRTY_ASC,
    SORT_SYNC_DESC,
    SORT_SYNC_ASC,
];

/// The two sort modes a Home table column toggles between, by column index
/// (Name, Branch, Sync, Dirty, Updated, Path). The first is the direction a
/// fresh click on that header selects — descending where "most" is the
/// interesting end (newest, most changes, most diverged), ascending for names.
/// `None` marks a column that isn't worth sorting by.
pub fn sort_modes_for_column(column: usize) -> Option<(usize, usize)> {
    match column {
        0 => Some((SORT_NAME_ASC, SORT_NAME_DESC)),
        1 => Some((SORT_BRANCH_ASC, SORT_BRANCH_DESC)),
        2 => Some((SORT_SYNC_DESC, SORT_SYNC_ASC)),
        3 => Some((SORT_DIRTY_DESC, SORT_DIRTY_ASC)),
        4 => Some((SORT_UPDATED_DESC, SORT_UPDATED_ASC)),
        _ => None, // Path sorts almost identically to Name
    }
}

/// The column a sort mode belongs to, for drawing the direction marker on the
/// right header. Inverse of [`sort_modes_for_column`].
pub fn column_for_sort_mode(sort_by: usize) -> Option<usize> {
    (0..6)
        .find(|&c| matches!(sort_modes_for_column(c), Some((a, b)) if a == sort_by || b == sort_by))
}

/// Whether a sort mode is the ascending one of its pair — the marker shown is
/// ▲ for ascending and ▼ for descending.
pub fn sort_is_ascending(sort_by: usize) -> bool {
    matches!(
        sort_by,
        SORT_NAME_ASC | SORT_UPDATED_ASC | SORT_BRANCH_ASC | SORT_DIRTY_ASC | SORT_SYNC_ASC
    )
}

pub fn sort_repo_indices(
    idx: &mut [usize],
    repos: &[Repository],
    rows: &HashMap<usize, HomeRow>,
    sort_by: usize,
) {
    // A repository whose row hasn't loaded yet has no value to compare, so it
    // sorts last in every mode rather than being treated as an empty string or
    // a zero, which would scatter unloaded rows through the list.
    fn missing_last<T: Ord>(
        a: Option<T>,
        b: Option<T>,
        cmp: impl Fn(T, T) -> std::cmp::Ordering,
    ) -> std::cmp::Ordering {
        match (a, b) {
            (Some(x), Some(y)) => cmp(x, y),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => std::cmp::Ordering::Equal,
        }
    }

    idx.sort_by(|&a, &b| {
        let date = |i: usize| {
            rows.get(&i)
                .map(|r| r.last_commit.as_str())
                .filter(|s| !s.is_empty())
        };
        let branch = |i: usize| {
            rows.get(&i)
                .map(|r| r.branch.to_lowercase())
                .filter(|s| !s.is_empty())
        };
        let dirty = |i: usize| rows.get(&i).map(|r| r.dirty);
        let sync = |i: usize| rows.get(&i).map(|r| r.ahead + r.behind);
        let name = |i: usize| repos[i].name.to_lowercase();

        match sort_by {
            SORT_NAME_ASC => name(a).cmp(&name(b)),
            SORT_NAME_DESC => name(b).cmp(&name(a)),
            SORT_UPDATED_ASC => missing_last(date(a), date(b), |x, y| x.cmp(y)),
            SORT_BRANCH_ASC => missing_last(branch(a), branch(b), |x, y| x.cmp(&y)),
            SORT_BRANCH_DESC => missing_last(branch(a), branch(b), |x, y| y.cmp(&x)),
            SORT_DIRTY_DESC => missing_last(dirty(a), dirty(b), |x, y| y.cmp(&x)),
            SORT_DIRTY_ASC => missing_last(dirty(a), dirty(b), |x, y| x.cmp(&y)),
            SORT_SYNC_DESC => missing_last(sync(a), sync(b), |x, y| y.cmp(&x)),
            SORT_SYNC_ASC => missing_last(sync(a), sync(b), |x, y| x.cmp(&y)),
            // SORT_UPDATED_DESC, and anything unrecognised (a value written by
            // a newer build, or the GUI) falls back to newest-first.
            _ => missing_last(date(a), date(b), |x, y| y.cmp(x)),
        }
    });
}

#[cfg(test)]
mod sort_tests {
    use super::*;

    fn repo(name: &str) -> Repository {
        Repository {
            name: name.to_string(),
            path: std::path::PathBuf::from(format!("/tmp/{name}")),
            group: None,
        }
    }

    struct Fixture {
        repos: Vec<Repository>,
        rows: HashMap<usize, HomeRow>,
    }

    impl Fixture {
        /// Three repositories, deliberately ordered so that no two sort modes
        /// produce the same answer — otherwise a test could pass against the
        /// wrong comparator.
        fn new() -> Self {
            let repos = vec![repo("charlie"), repo("alpha"), repo("bravo")];
            let mut rows = HashMap::new();
            rows.insert(
                0,
                HomeRow {
                    branch: "main".into(),
                    ahead: 0,
                    behind: 5,
                    dirty: 1,
                    last_commit: "2026-08-20".into(),
                    ..Default::default()
                },
            );
            rows.insert(
                1,
                HomeRow {
                    branch: "zebra".into(),
                    ahead: 1,
                    behind: 0,
                    dirty: 9,
                    last_commit: "2026-08-25".into(),
                    ..Default::default()
                },
            );
            rows.insert(
                2,
                HomeRow {
                    branch: "develop".into(),
                    ahead: 0,
                    behind: 0,
                    dirty: 0,
                    last_commit: "2026-08-10".into(),
                    ..Default::default()
                },
            );
            Self { repos, rows }
        }

        fn order(&self, mode: usize) -> Vec<&str> {
            let mut idx: Vec<usize> = (0..self.repos.len()).collect();
            sort_repo_indices(&mut idx, &self.repos, &self.rows, mode);
            idx.iter().map(|&i| self.repos[i].name.as_str()).collect()
        }
    }

    #[test]
    fn every_mode_orders_by_its_own_column() {
        let f = Fixture::new();
        assert_eq!(f.order(SORT_NAME_ASC), ["alpha", "bravo", "charlie"]);
        assert_eq!(f.order(SORT_NAME_DESC), ["charlie", "bravo", "alpha"]);
        // develop < main < zebra
        assert_eq!(f.order(SORT_BRANCH_ASC), ["bravo", "charlie", "alpha"]);
        assert_eq!(f.order(SORT_BRANCH_DESC), ["alpha", "charlie", "bravo"]);
        // dirty: alpha 9, charlie 1, bravo 0
        assert_eq!(f.order(SORT_DIRTY_DESC), ["alpha", "charlie", "bravo"]);
        assert_eq!(f.order(SORT_DIRTY_ASC), ["bravo", "charlie", "alpha"]);
        // ahead+behind: charlie 5, alpha 1, bravo 0
        assert_eq!(f.order(SORT_SYNC_DESC), ["charlie", "alpha", "bravo"]);
        assert_eq!(f.order(SORT_SYNC_ASC), ["bravo", "alpha", "charlie"]);
        assert_eq!(f.order(SORT_UPDATED_DESC), ["alpha", "charlie", "bravo"]);
        assert_eq!(f.order(SORT_UPDATED_ASC), ["bravo", "charlie", "alpha"]);
    }

    /// On the first launch after adding a repository its row has not loaded.
    /// Those rows must collect at the end instead of being compared as 0 or
    /// "", which would scatter them through the list and make the order churn
    /// as each row arrives.
    #[test]
    fn rows_that_have_not_loaded_sort_last_in_every_mode() {
        let mut f = Fixture::new();
        f.repos.push(repo("zzz-unloaded"));
        for &mode in SORT_CYCLE.iter() {
            if mode == SORT_NAME_ASC || mode == SORT_NAME_DESC {
                continue; // name comes from the repo list, never missing
            }
            assert_eq!(
                *f.order(mode).last().unwrap(),
                "zzz-unloaded",
                "mode {mode} put an unloaded row somewhere other than last"
            );
        }
    }

    /// `prefs.json` is shared with the sibling GUI, which knows only modes
    /// 0-3 and may also write a value this build has never heard of. An
    /// unknown mode must degrade to a sensible order, not panic or scramble.
    #[test]
    fn an_unrecognised_mode_falls_back_to_newest_first() {
        let f = Fixture::new();
        assert_eq!(f.order(9999), f.order(SORT_UPDATED_DESC));
    }

    #[test]
    fn a_column_maps_to_the_pair_of_modes_that_maps_back_to_it() {
        for column in 0..6 {
            let Some((primary, secondary)) = sort_modes_for_column(column) else {
                continue;
            };
            assert_eq!(column_for_sort_mode(primary), Some(column));
            assert_eq!(column_for_sort_mode(secondary), Some(column));
            assert_ne!(
                sort_is_ascending(primary),
                sort_is_ascending(secondary),
                "column {column}'s two modes should be opposite directions"
            );
        }
    }

    /// The `o` key cycles through modes; every one of them has to be a mode
    /// the header renderer can draw a marker for, or the cycle would pass
    /// through states the header cannot explain.
    #[test]
    fn every_cycled_mode_belongs_to_a_visible_column() {
        for &mode in SORT_CYCLE.iter() {
            assert!(
                column_for_sort_mode(mode).is_some(),
                "cycle mode {mode} maps to no column"
            );
        }
    }
}
