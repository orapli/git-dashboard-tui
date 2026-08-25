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

/// A repository the "n" (needs attention) filter should surface: a failing
/// CI run, an unresolved conflict, or a merge/rebase/etc. left mid-operation.
/// Deliberately excludes plain uncommitted changes or being behind upstream —
/// those are normal working state, not something that needs a response.
pub fn needs_attention(row: &HomeRow) -> bool {
    row.op_state != GitOpState::None
        || row.conflicts > 0
        || row
            .ci_status
            .as_deref()
            .is_some_and(|s| classify_ci_status(s) == CiOutcome::Failure)
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

pub fn sort_repo_indices(
    idx: &mut [usize],
    repos: &[Repository],
    rows: &HashMap<usize, HomeRow>,
    sort_by: usize,
) {
    idx.sort_by(|&a, &b| {
        let date = |i: usize| rows.get(&i).map(|r| r.last_commit.as_str()).unwrap_or("");
        match sort_by {
            0 => repos[a]
                .name
                .to_lowercase()
                .cmp(&repos[b].name.to_lowercase()),
            1 => repos[b]
                .name
                .to_lowercase()
                .cmp(&repos[a].name.to_lowercase()),
            3 => {
                let da = date(a);
                let db = date(b);
                match (da.is_empty(), db.is_empty()) {
                    (true, false) => std::cmp::Ordering::Greater,
                    (false, true) => std::cmp::Ordering::Less,
                    _ => da.cmp(db),
                }
            }
            _ => {
                let da = date(a);
                let db = date(b);
                match (da.is_empty(), db.is_empty()) {
                    (true, false) => std::cmp::Ordering::Greater,
                    (false, true) => std::cmp::Ordering::Less,
                    _ => db.cmp(da),
                }
            }
        }
    });
}
