use super::exec::{check_safe_ref, check_safe_ref_opt, run_git_cmd};
use super::types::{ChangedFile, DiffRow, DiffRowKind, FileDiff};
use std::collections::HashMap;
use std::path::Path;

/// Sentinel `target` meaning "the working tree, diffed against HEAD" rather
/// than a real commit-ish. Distinct from [`super::log::BLAME_WORKING_TREE`],
/// which happens to share the same text but names a different sentinel (a
/// blame *revision*, not a diff *target*) — don't conflate the two just
/// because the strings match today.
pub const WORKING_TREE: &str = "WORKING_TREE";

pub fn build_diff_args<'a>(
    range: &'a [String],
    extra: &[&'a str],
    paths: &[&'a str],
) -> Vec<&'a str> {
    let mut args = Vec::with_capacity(3 + extra.len() + range.len() + 1 + paths.len());
    args.push("diff");
    args.extend_from_slice(extra);
    for r in range {
        args.push(r.as_str());
    }
    args.push("--");
    args.extend_from_slice(paths);
    args
}

// Get recent commits list

pub fn diff_range(base: Option<&str>, target: &str, three_dot: bool) -> Vec<String> {
    match base {
        // GitHub-compare style: changes on target since the merge-base
        Some(b) if three_dot => vec![format!("{b}...{target}")],
        Some(b) => vec![format!("{b}..{target}")],
        // Single-commit mode: diff against the parent
        None => vec![format!("{target}^..{target}")],
    }
}

/// Git's empty tree object, the conventional base for diffing a root commit.
const EMPTY_TREE: &str = "4b825dc642cb6eb9a060e54bf8d69288fbee4904";

/// Range to retry with when `<target>^..<target>` failed: a repository's first
/// commit has no parent, which otherwise renders as an empty diff with no
/// explanation. `None` when the original range had a real base and the failure
/// must be surfaced instead.
fn root_fallback_range(base: Option<&str>, target: &str) -> Option<Vec<String>> {
    if base.is_some() || target == WORKING_TREE {
        return None;
    }
    Some(vec![EMPTY_TREE.to_string(), target.to_string()])
}

/// Run a `git diff` invocation, retrying against the empty tree if the range
/// itself was unresolvable (see [`root_fallback_range`]).
fn run_diff(
    repo_path: &Path,
    base: Option<&str>,
    target: &str,
    range: &[String],
    extra: &[&str],
    paths: &[&str],
) -> Result<String, String> {
    let args = build_diff_args(range, extra, paths);
    match run_git_cmd(repo_path, &args) {
        Ok(out) => Ok(out),
        Err(e) => {
            let fallback = root_fallback_range(base, target).ok_or(e)?;
            let args = build_diff_args(&fallback, extra, paths);
            run_git_cmd(repo_path, &args)
        }
    }
}

pub fn unquote_path(path: &str) -> String {
    let s = path.trim();
    if s.starts_with('"') && s.ends_with('"') && s.len() >= 2 {
        let inner = &s[1..s.len() - 1];
        let mut result = Vec::new();
        let bytes = inner.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == b'\\' && i + 1 < bytes.len() {
                match bytes[i + 1] {
                    b'n' => {
                        result.push(b'\n');
                        i += 2;
                    }
                    b't' => {
                        result.push(b'\t');
                        i += 2;
                    }
                    b'r' => {
                        result.push(b'\r');
                        i += 2;
                    }
                    b'\\' => {
                        result.push(b'\\');
                        i += 2;
                    }
                    b'"' => {
                        result.push(b'"');
                        i += 2;
                    }
                    b'0'..=b'7' => {
                        let mut octal = 0u8;
                        let mut count = 0;
                        while i + 1 < bytes.len()
                            && (b'0'..=b'7').contains(&bytes[i + 1])
                            && count < 3
                        {
                            octal = (octal << 3) + (bytes[i + 1] - b'0');
                            i += 1;
                            count += 1;
                        }
                        result.push(octal);
                        i += 1;
                    }
                    c => {
                        result.push(c);
                        i += 2;
                    }
                }
            } else {
                result.push(bytes[i]);
                i += 1;
            }
        }
        String::from_utf8(result)
            .unwrap_or_else(|e| String::from_utf8_lossy(e.as_bytes()).into_owned())
    } else {
        s.to_string()
    }
}

pub fn normalize_numstat_path(path: &str) -> String {
    let trimmed = path.trim();
    if let Some((_, new_part)) = trimmed.split_once(" => ") {
        if let (Some(open), Some(close)) = (trimmed.find('{'), trimmed.find('}'))
            && open < close
        {
            let prefix = &trimmed[..open];
            let suffix = &trimmed[close + 1..];
            let middle = &trimmed[open + 1..close];
            if let Some((_, right)) = middle.split_once(" => ") {
                let full = format!("{prefix}{right}{suffix}");
                return unquote_path(&full);
            }
        }
        return unquote_path(new_part);
    }
    unquote_path(trimmed)
}

pub fn parse_changed_files(name_status: &str, numstat: &str) -> Vec<ChangedFile> {
    // Build numstat map: path -> (additions, deletions)
    let mut num_map: HashMap<String, (usize, usize)> = HashMap::new();
    for line in numstat.lines() {
        let parts: Vec<&str> = line.splitn(3, '\t').collect();
        if parts.len() < 3 {
            continue;
        }
        let add = parts[0].trim().parse::<usize>().unwrap_or(0);
        let del = parts[1].trim().parse::<usize>().unwrap_or(0);
        let raw_path = parts[2].trim();
        let path = normalize_numstat_path(raw_path);
        num_map.insert(path, (add, del));
    }

    let mut files = Vec::new();
    for line in name_status.lines() {
        let parts: Vec<&str> = line.splitn(3, '\t').collect();
        if parts.is_empty() {
            continue;
        }
        let status_raw = parts[0].trim();
        let status = if status_raw.starts_with('R') {
            "R".to_string()
        } else {
            status_raw.chars().next().unwrap_or('?').to_string()
        };

        let (path, old_path) = if status == "R" && parts.len() >= 3 {
            (
                unquote_path(parts[2].trim()),
                Some(unquote_path(parts[1].trim())),
            )
        } else if parts.len() >= 2 {
            (unquote_path(parts[1].trim()), None)
        } else {
            continue;
        };

        let (additions, deletions) = num_map.get(&path).copied().unwrap_or((0, 0));
        files.push(ChangedFile {
            status,
            path,
            old_path,
            additions,
            deletions,
        });
    }
    files
}

/// Returns the list of files changed between base (or parent) and target.
pub fn get_changed_files(
    repo_path: &Path,
    base: Option<&str>,
    target: &str,
    three_dot: bool,
) -> Result<Vec<ChangedFile>, String> {
    check_safe_ref_opt(base)?;
    check_safe_ref(target)?;

    let range = if target == WORKING_TREE {
        working_tree_range(repo_path)
    } else {
        diff_range(base, target, three_dot)
    };

    let extra = ["--name-status", "--find-renames"];
    let name_status = run_diff(repo_path, base, target, &range, &extra, &[])?;

    let extra = ["--numstat", "--find-renames"];
    let numstat = run_diff(repo_path, base, target, &range, &extra, &[])?;

    let mut files = parse_changed_files(&name_status, &numstat);
    if target == WORKING_TREE {
        for path in untracked_files(repo_path)? {
            files.push(ChangedFile {
                status: "?".into(),
                path,
                old_path: None,
                additions: 0,
                deletions: 0,
            });
        }
        files.sort_by(|a, b| a.path.cmp(&b.path));
    }
    Ok(files)
}

fn working_tree_range(path: &Path) -> Vec<String> {
    // An unborn branch still has staged and untracked work worth inspecting.
    vec![
        if run_git_cmd(path, &["rev-parse", "--verify", "HEAD"]).is_ok() {
            "HEAD".into()
        } else {
            EMPTY_TREE.into()
        },
    ]
}

fn untracked_files(path: &Path) -> Result<Vec<String>, String> {
    let output = run_git_cmd(
        path,
        &[
            "-c",
            "core.quotePath=true",
            "ls-files",
            "--others",
            "--exclude-standard",
        ],
    )?;
    Ok(output.lines().map(unquote_path).collect())
}

fn untracked_diff(path: &Path, file: &str, extra: &[&str]) -> Result<String, String> {
    let mut args = vec!["diff", "--no-index"];
    args.extend_from_slice(extra);
    args.extend_from_slice(&["--", "/dev/null", file]);
    let output = super::exec::run_with_timeout(
        super::exec::git_command_for(path, &args),
        super::exec::GIT_TIMEOUT,
    )?;
    // --no-index reports differences as exit 1, not an execution failure.
    if !output.status.success() && output.status.code() != Some(1) {
        return Err(super::exec::strip_control_sequences(
            &String::from_utf8_lossy(&output.stderr),
            false,
        ));
    }
    Ok(super::exec::strip_control_sequences(
        &String::from_utf8_lossy(&output.stdout),
        false,
    ))
}

/// Char-level similarity in [0, 1] between two lines: 2*LCS/(len_a+len_b).
/// Very long lines fall back to a cheap char-frequency ratio to stay fast.
pub fn line_similarity(a: &str, b: &str) -> f32 {
    let a: Vec<char> = a.trim().chars().collect();
    let b: Vec<char> = b.trim().chars().collect();
    if a.is_empty() && b.is_empty() {
        return 1.0;
    }
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    let (n, m) = (a.len(), b.len());
    let denom = (n + m) as f32;
    if n * m <= 40_000 {
        // LCS length with two rolling rows (O(min) memory)
        let mut prev = vec![0u32; m + 1];
        let mut cur = vec![0u32; m + 1];
        for i in 1..=n {
            for j in 1..=m {
                cur[j] = if a[i - 1] == b[j - 1] {
                    prev[j - 1] + 1
                } else {
                    prev[j].max(cur[j - 1])
                };
            }
            std::mem::swap(&mut prev, &mut cur);
        }
        2.0 * prev[m] as f32 / denom
    } else {
        let mut counts: HashMap<char, u32> = HashMap::new();
        for &c in &a {
            *counts.entry(c).or_insert(0) += 1;
        }
        let mut common = 0u32;
        for &c in &b {
            if let Some(k) = counts.get_mut(&c)
                && *k > 0
            {
                *k -= 1;
                common += 1;
            }
        }
        2.0 * common as f32 / denom
    }
}

/// Positionally paired +/- lines are shown side-by-side as Modified only when
/// at least half their characters match; below this they read better as a
/// plain removal plus addition (no forced intra-line comparison).
const PAIR_SIMILARITY_THRESHOLD: f32 = 0.5;

/// Parses a unified diff string into split-view DiffRows.
/// This is a pure function so it can be unit-tested.
pub fn parse_unified_diff(diff_text: &str) -> Vec<DiffRow> {
    let mut rows: Vec<DiffRow> = Vec::new();
    let mut left_no: usize = 0;
    let mut right_no: usize = 0;

    // Buffers for a consecutive block of +/- lines inside one hunk
    let mut removed_buf: Vec<String> = Vec::new();
    let mut added_buf: Vec<String> = Vec::new();
    let mut removed_start: usize = 0;
    let mut added_start: usize = 0;

    let flush = |rows: &mut Vec<DiffRow>,
                 removed: &mut Vec<String>,
                 added: &mut Vec<String>,
                 r_start: usize,
                 a_start: usize| {
        let pairs = removed.len().max(added.len());
        // Dissimilar pairs are buffered so consecutive ones come out as a block
        // of removals followed by a block of additions, not a zigzag.
        let mut pend_rem: Vec<DiffRow> = Vec::new();
        let mut pend_add: Vec<DiffRow> = Vec::new();
        for i in 0..pairs {
            let l = removed.get(i).cloned();
            let r = added.get(i).cloned();
            let ln = if i < removed.len() {
                Some(r_start + i)
            } else {
                None
            };
            let rn = if i < added.len() {
                Some(a_start + i)
            } else {
                None
            };
            let paired = match (&l, &r) {
                (Some(l), Some(r)) => line_similarity(l, r) >= PAIR_SIMILARITY_THRESHOLD,
                _ => false,
            };
            if paired {
                rows.append(&mut pend_rem);
                rows.append(&mut pend_add);
                rows.push(DiffRow {
                    kind: DiffRowKind::Modified,
                    left_no: ln,
                    left_text: l,
                    left_tokens: None,
                    right_no: rn,
                    right_text: r,
                    right_tokens: None,
                });
            } else {
                if l.is_some() {
                    pend_rem.push(DiffRow {
                        kind: DiffRowKind::Removed,
                        left_no: ln,
                        left_text: l,
                        left_tokens: None,
                        right_no: None,
                        right_text: None,
                        right_tokens: None,
                    });
                }
                if r.is_some() {
                    pend_add.push(DiffRow {
                        kind: DiffRowKind::Added,
                        left_no: None,
                        left_text: None,
                        left_tokens: None,
                        right_no: rn,
                        right_text: r,
                        right_tokens: None,
                    });
                }
            }
        }
        rows.append(&mut pend_rem);
        rows.append(&mut pend_add);
        removed.clear();
        added.clear();
    };

    for line in diff_text.lines() {
        if line.starts_with("@@") {
            // Flush pending +/- block before starting a new hunk
            flush(
                &mut rows,
                &mut removed_buf,
                &mut added_buf,
                removed_start,
                added_start,
            );

            // Parse @@ -a,b +c,d @@
            let parse_hunk = || -> Option<(usize, usize)> {
                let s = line.trim_start_matches('@').trim_start_matches(' ');
                let mut parts = s.split_whitespace();
                let old = parts.next()?;
                let new = parts.next()?;
                let old_start = old
                    .trim_start_matches('-')
                    .split(',')
                    .next()?
                    .parse::<usize>()
                    .ok()?;
                let new_start = new
                    .trim_start_matches('+')
                    .split(',')
                    .next()?
                    .parse::<usize>()
                    .ok()?;
                Some((old_start, new_start))
            };
            if let Some((ls, rs)) = parse_hunk() {
                left_no = ls;
                right_no = rs;
            }
            continue;
        }

        // Skip diff header lines
        if line.starts_with("diff ")
            || line.starts_with("index ")
            || line.starts_with("--- ")
            || line.starts_with("+++ ")
        {
            flush(
                &mut rows,
                &mut removed_buf,
                &mut added_buf,
                removed_start,
                added_start,
            );
            continue;
        }

        // "\ No newline at end of file" is metadata, not content; counting it
        // as a context line would shift every line number after it.
        if line.starts_with('\\') {
            continue;
        }

        if let Some(stripped) = line.strip_prefix('-') {
            if removed_buf.is_empty() {
                removed_start = left_no;
            }
            removed_buf.push(stripped.to_string());
            left_no += 1;
        } else if let Some(stripped) = line.strip_prefix('+') {
            if added_buf.is_empty() {
                added_start = right_no;
            }
            added_buf.push(stripped.to_string());
            right_no += 1;
        } else {
            // Context line — flush pending block first
            flush(
                &mut rows,
                &mut removed_buf,
                &mut added_buf,
                removed_start,
                added_start,
            );
            let text = if let Some(stripped) = line.strip_prefix(' ') {
                stripped.to_string()
            } else {
                line.to_string()
            };
            rows.push(DiffRow {
                kind: DiffRowKind::Context,
                left_no: Some(left_no),
                left_text: Some(text.clone()),
                left_tokens: None,
                right_no: Some(right_no),
                right_text: Some(text),
                right_tokens: None,
            });
            left_no += 1;
            right_no += 1;
        }
    }
    flush(
        &mut rows,
        &mut removed_buf,
        &mut added_buf,
        removed_start,
        added_start,
    );
    rows
}

pub const MAX_DIFF_LINES: usize = 2000;

/// Returns the split-view diff for a single file.
pub fn get_file_diff(
    repo_path: &Path,
    base: Option<&str>,
    target: &str,
    file_path: &str,
    ignore_whitespace: bool,
    full: bool,
    three_dot: bool,
) -> Result<FileDiff, String> {
    check_safe_ref_opt(base)?;
    check_safe_ref(target)?;

    let mut extra = vec!["--color=never"];
    if ignore_whitespace {
        extra.push("-w");
    }
    if full {
        extra.push("--unified=999999");
    }

    let range = if target == WORKING_TREE {
        working_tree_range(repo_path)
    } else {
        diff_range(base, target, three_dot)
    };

    let raw =
        if target == WORKING_TREE && untracked_files(repo_path)?.iter().any(|p| p == file_path) {
            untracked_diff(repo_path, file_path, &extra)?
        } else {
            run_diff(repo_path, base, target, &range, &extra, &[file_path])?
        };

    // Match only git's own marker line, not file content that happens to
    // contain the phrase (content lines are prefixed with +/-/space).
    if raw.lines().any(|l| l.starts_with("Binary files")) {
        return Ok(FileDiff {
            path: file_path.to_string(),
            is_binary: true,
            truncated: false,
            rows: vec![],
        });
    }

    let mut rows = parse_unified_diff(&raw);
    // Truncate before tokenizing: a 500k-row diff otherwise tokenizes every row
    // (two Strings and two token vectors each) only to keep the first 2000.
    let limit = if full { 5000 } else { MAX_DIFF_LINES };
    let truncated = rows.len() > limit;
    if truncated {
        rows.truncate(limit);
    }
    // Expand tabs before tokenizing, so the highlighter and the renderer agree
    // on every column. Doing it here rather than at each `DiffRow`
    // construction keeps it to one place that no future row kind can bypass.
    for row in &mut rows {
        if let Some(text) = row.left_text.take() {
            row.left_text = Some(super::exec::expand_tabs(&text, super::exec::TAB_WIDTH));
        }
        if let Some(text) = row.right_text.take() {
            row.right_text = Some(super::exec::expand_tabs(&text, super::exec::TAB_WIDTH));
        }
    }
    let ext = std::path::Path::new(file_path)
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("");
    for row in &mut rows {
        if let Some(ref text) = row.left_text {
            row.left_tokens = Some(crate::syntax::tokenize(text, ext));
        }
        if let Some(ref text) = row.right_text {
            row.right_tokens = Some(crate::syntax::tokenize(text, ext));
        }
    }
    Ok(FileDiff {
        path: file_path.to_string(),
        is_binary: false,
        truncated,
        rows,
    })
}

// Commit header + message only (-s suppresses the diffstat: the detail
// panel renders the changed files as a styled tree instead)
