use super::exec::{check_safe_ref, run_git_cmd};
use super::types::{BlameEntry, CommitHit, CommitRef, CommitSummary};
use std::collections::HashMap;
use std::path::Path;

pub fn split_graph_and_hash(raw_hash_field: &str) -> (String, String) {
    let trimmed = raw_hash_field.trim_end();
    if let Some(pos) = trimmed.rfind(' ') {
        let graph = trimmed[..=pos].to_string();
        let hash = trimmed[pos + 1..].trim().to_string();
        if !hash.is_empty() {
            return (graph, hash);
        }
    }
    ("".to_string(), trimmed.trim().to_string())
}

pub fn parse_refs(raw: &str) -> Vec<CommitRef> {
    let mut refs = Vec::new();
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return refs;
    }
    for item in trimmed.split(',') {
        let item = item.trim();
        if item.is_empty() {
            continue;
        }
        if let Some(tag_name) = item.strip_prefix("tag: ") {
            refs.push(CommitRef {
                name: tag_name.trim().to_string(),
                is_head: false,
                is_tag: true,
                is_remote: false,
            });
        } else if item.starts_with("HEAD -> ") {
            let branch = item.trim_start_matches("HEAD -> ").trim();
            refs.push(CommitRef {
                name: branch.to_string(),
                is_head: true,
                is_tag: false,
                is_remote: false,
            });
        } else if item == "HEAD" {
            refs.push(CommitRef {
                name: "HEAD".to_string(),
                is_head: true,
                is_tag: false,
                is_remote: false,
            });
        } else if item.starts_with("origin/")
            || item.starts_with("upstream/")
            || item.starts_with("remotes/")
        {
            refs.push(CommitRef {
                name: item.to_string(),
                is_head: false,
                is_tag: false,
                is_remote: true,
            });
        } else {
            refs.push(CommitRef {
                name: item.to_string(),
                is_head: false,
                is_tag: false,
                is_remote: false,
            });
        }
    }
    refs
}

pub fn parse_commit_log(log_output: &str) -> Vec<CommitSummary> {
    let mut commits = Vec::new();
    for line in log_output.lines() {
        let parts: Vec<&str> = line.split("|||").collect();
        if parts.len() < 4 {
            continue;
        }
        let (graph, hash) = split_graph_and_hash(parts[0]);
        if hash.is_empty() {
            continue;
        }
        if parts.len() >= 5 {
            let refs = parse_refs(parts[1]);
            let message = parts[4..].join("|||").trim().to_string();
            commits.push(CommitSummary {
                hash,
                author: parts[2].trim().to_string(),
                date: parts[3].trim().to_string(),
                message,
                graph,
                refs,
            });
        } else {
            let message = parts[3..].join("|||").trim().to_string();
            commits.push(CommitSummary {
                hash,
                author: parts[1].trim().to_string(),
                date: parts[2].trim().to_string(),
                message,
                graph,
                refs: Vec::new(),
            });
        }
    }
    commits
}

/// Search commit messages (all refs, case-insensitive, literal substring —
/// not a regex, so a query like "fix(auth)" doesn't need escaping) for the
/// most recent `limit` matches. Used to search across many repositories at
/// once from the Home screen.
///
/// `query` is embedded in a single `--grep=<query>` argv token rather than
/// passed as a separate value, so even a query starting with `-` can never
/// be read as its own flag.
pub fn search_commits(
    repo_path: &Path,
    query: &str,
    limit: usize,
) -> Result<Vec<CommitHit>, String> {
    let limit_s = limit.to_string();
    let grep_arg = format!("--grep={query}");
    let output = run_git_cmd(
        repo_path,
        &[
            "log",
            "--all",
            "-i",
            "-F",
            &grep_arg,
            "-n",
            &limit_s,
            "--format=%h|||%an|||%ad|||%s",
            "--date=format:%Y-%m-%d",
        ],
    )?;
    Ok(output.lines().filter_map(parse_commit_hit).collect())
}

/// `splitn(4, ..)` keeps everything after the third `|||` intact in the
/// message field, in case the commit subject itself contains that sequence.
fn parse_commit_hit(line: &str) -> Option<CommitHit> {
    let parts: Vec<&str> = line.splitn(4, "|||").collect();
    if parts.len() < 4 {
        return None;
    }
    Some(CommitHit {
        hash: parts[0].trim().to_string(),
        author: parts[1].trim().to_string(),
        date: parts[2].trim().to_string(),
        message: parts[3].trim().to_string(),
    })
}

pub fn get_recent_commits(repo_path: &Path) -> Result<Vec<CommitSummary>, String> {
    let output = run_git_cmd(
        repo_path,
        &[
            "log",
            "--graph",
            // git's own per-lane coloring (log.graphColors, default
            // red/green/yellow/blue/magenta/cyan cycling by lane) — the UI
            // maps these onto the active theme rather than re-deriving lane
            // identity itself, so any topology git can lay out gets
            // lane-consistent color for free. `-c color.ui=never` (set on
            // every invocation, see exec::GIT_CONFIG_ARGS) is overridden by
            // this explicit flag, as git gives command-line flags priority.
            "--color=always",
            "--all",
            "-n",
            "50",
            "--format=%h|||%D|||%an|||%ad|||%s",
            "--date=format:%Y-%m-%d %H:%M",
        ],
    )?;
    Ok(parse_commit_log(&output))
}

pub fn get_commits_for_diff(repo_path: &Path) -> Result<Vec<CommitSummary>, String> {
    let output = run_git_cmd(
        repo_path,
        &[
            "log",
            "--graph",
            "--color=always",
            "--all",
            "-n",
            "200",
            "--format=%h|||%D|||%an|||%ad|||%s",
            "--date=format:%Y-%m-%d %H:%M",
        ],
    )?;
    Ok(parse_commit_log(&output))
}

pub fn get_commit_show(repo_path: &Path, hash: &str) -> Result<String, String> {
    check_safe_ref(hash)?;
    // `--` keeps a ref that looks like a path (or an option) from being read
    // as one; the ref itself is already checked above.
    run_git_cmd(repo_path, &["show", "-s", hash, "--"])
}

// Get branch commits in oneline format with graph and colors (git log --graph --oneline --decorate --color=always)
pub fn get_branch_oneline_log(
    repo_path: &Path,
    branch_name: &str,
    limit: usize,
) -> Result<String, String> {
    check_safe_ref(branch_name)?;
    let limit_s = limit.to_string();
    run_git_cmd(
        repo_path,
        &[
            "log",
            branch_name,
            "-n",
            &limit_s,
            "--graph",
            "--decorate",
            "--color=always",
            "--pretty=format:%C(auto)%h%C(reset)%C(auto)%d%C(reset) %s %C(dim)(%cr, %an)%C(reset)",
            "--",
        ],
    )
}

/// Sentinel meaning "blame the working tree", matching the `WORKING_TREE`
/// convention `diff.rs`/`status.rs` already use for the same case. Omitting
/// the revision entirely (rather than passing `HEAD`) is what makes git
/// attribute uncommitted lines to a synthetic "Not Committed Yet" commit
/// instead of silently blaming whatever HEAD last touched them.
pub const BLAME_WORKING_TREE: &str = "WORKING_TREE";

pub fn get_file_blame(
    repo_path: &Path,
    blame_ref: &str,
    file_path: &str,
) -> Result<Vec<BlameEntry>, String> {
    let is_worktree = blame_ref == BLAME_WORKING_TREE;
    if !is_worktree {
        check_safe_ref(blame_ref)?;
    }
    let mut args = vec!["blame", "--line-porcelain"];
    if !is_worktree {
        args.push(blame_ref);
    }
    args.push("--");
    args.push(file_path);
    let output_res = run_git_cmd(repo_path, &args);

    let output = match output_res {
        Ok(out) => out,
        Err(err) => {
            if let Some(fallback_ref) = (!is_worktree)
                .then_some(blame_ref)
                .and_then(|r| r.strip_suffix('^'))
            {
                check_safe_ref(fallback_ref)?;
                let fallback_args =
                    vec!["blame", "--line-porcelain", fallback_ref, "--", file_path];
                run_git_cmd(repo_path, &fallback_args)?
            } else {
                return Err(err);
            }
        }
    };

    #[derive(Clone)]
    struct CommitInfo {
        author: String,
        date: String,
        summary: String,
    }

    // Each entry clones its commit's author/date/summary rather than sharing
    // them, so an unbounded file could hold tens of thousands of duplicated
    // Strings in memory at once — MAX_GIT_OUTPUT already caps the raw text,
    // but a highly-repetitive-lines file can still parse into far more
    // *entries* than its raw byte size would suggest.
    const MAX_BLAME_LINES: usize = 20_000;

    let mut commit_cache: HashMap<String, CommitInfo> = HashMap::new();
    let mut entries = Vec::new();

    let mut current_hash = String::new();
    let mut temp_author = String::new();
    let mut temp_date = String::new();
    let mut temp_summary = String::new();

    for line in output.lines() {
        if entries.len() >= MAX_BLAME_LINES {
            break;
        }
        if line.starts_with('\t') {
            let info = commit_cache
                .entry(current_hash.clone())
                .or_insert_with(|| CommitInfo {
                    author: temp_author.clone(),
                    date: temp_date.clone(),
                    summary: temp_summary.clone(),
                });

            entries.push(BlameEntry {
                hash: current_hash.clone(),
                author: info.author.clone(),
                date: info.date.clone(),
                summary: info.summary.clone(),
            });
            continue;
        }

        let parts: Vec<&str> = line.splitn(2, ' ').collect();
        if parts.is_empty() {
            continue;
        }

        match parts[0] {
            "author" => {
                if parts.len() > 1 {
                    temp_author = parts[1].to_string();
                }
            }
            "author-time" => {
                if let Some(datetime) = parts
                    .get(1)
                    .and_then(|s| s.parse::<i64>().ok())
                    .and_then(|ts| chrono::DateTime::from_timestamp(ts, 0))
                {
                    temp_date = datetime.format("%Y-%m-%d").to_string();
                }
            }
            "summary" => {
                if parts.len() > 1 {
                    temp_summary = parts[1].to_string();
                }
            }
            _ => {
                if parts[0].len() == 40 && parts[0].chars().all(|c| c.is_ascii_hexdigit()) {
                    current_hash = parts[0].to_string();
                }
            }
        }
    }

    Ok(entries)
}
