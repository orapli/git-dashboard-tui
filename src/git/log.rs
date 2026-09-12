use super::exec::{check_safe_ref, run_git_cmd, run_git_cmd_ansi};
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

/// The filter prefixes understood inside the one-line commit-search query.
/// Anything that is not one of these (`fix:` in a conventional-commit
/// subject, say) stays part of the message text, so an ordinary query keeps
/// behaving exactly as it did before filters existed.
pub const SEARCH_PREFIXES: &[&str] = &["author:", "path:", "since:", "until:"];

/// Why a typed query was refused. The variants carry the offending field
/// rather than a rendered sentence: these reach the user, and the app layer
/// owns the bilingual wording.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SearchQueryError {
    /// `author:` (or another prefix) with nothing after the colon.
    MissingValue(&'static str),
    /// A value starting with `-`. Even where the value is embedded in a
    /// single `--author=<v>` token and so could not be read as an option, it
    /// is refused uniformly: a pathspec *is* a separate argv token, and one
    /// rule is easier to keep right than four.
    LeadingDash(&'static str),
    /// A control character — never legitimate in an author, a path or a
    /// date, and a newline would forge a line in the batched remote script
    /// `exec.rs` builds for `ssh://` repositories.
    ControlChar(&'static str),
}

impl SearchQueryError {
    /// The query field the problem is in: one of [`SEARCH_PREFIXES`] without
    /// its colon, or `"query"` for the free message text.
    pub fn field(&self) -> &'static str {
        match self {
            SearchQueryError::MissingValue(f)
            | SearchQueryError::LeadingDash(f)
            | SearchQueryError::ControlChar(f) => f,
        }
    }
}

impl std::fmt::Display for SearchQueryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SearchQueryError::MissingValue(field) => write!(f, "{field}: needs a value"),
            SearchQueryError::LeadingDash(field) => {
                write!(f, "{field}: value cannot start with '-'")
            }
            SearchQueryError::ControlChar(field) => {
                write!(f, "{field}: value contains a control character")
            }
        }
    }
}

/// A parsed cross-repository commit search: free message text plus the
/// filters that map onto `git log --author=` / `--since=` / `--until=` and
/// `-- <pathspec>`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CommitSearchQuery {
    /// Message substring (case-insensitive, literal). Empty means "any
    /// message": the `--grep` is then left out rather than passed empty.
    pub text: String,
    /// Author patterns, OR'd by git when more than one is given. Filled from
    /// `author:` and later widened with the member aliases.
    pub authors: Vec<String>,
    /// Pathspecs, OR'd by git, passed after `--`.
    pub paths: Vec<String>,
    pub since: Option<String>,
    pub until: Option<String>,
}

/// One whitespace-separated piece of the query line. `quoted` records that
/// the piece *started* with a double quote, which is the escape hatch for
/// searching messages that literally contain a prefix: `"path:"` is message
/// text, while `path:"a b"` is a filter whose value contains a space.
struct Token {
    text: String,
    quoted: bool,
}

/// Split on whitespace, honouring double quotes. An unclosed quote runs to
/// the end of the line instead of erroring: this is a live prompt, and a
/// query the user is still typing should not be rejected mid-word.
fn tokenize(raw: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut started = false;
    let mut quoted = false;
    let mut in_quote = false;
    for ch in raw.chars() {
        if ch == '"' {
            if !started {
                started = true;
                quoted = true;
            }
            in_quote = !in_quote;
            continue;
        }
        if ch.is_whitespace() && !in_quote {
            if started {
                tokens.push(Token {
                    text: std::mem::take(&mut current),
                    quoted,
                });
                started = false;
                quoted = false;
            }
            continue;
        }
        started = true;
        current.push(ch);
    }
    if started {
        tokens.push(Token {
            text: current,
            quoted,
        });
    }
    tokens
}

fn check_filter_value(field: &'static str, value: &str) -> Result<(), SearchQueryError> {
    if value.is_empty() {
        return Err(SearchQueryError::MissingValue(field));
    }
    if value.starts_with('-') {
        return Err(SearchQueryError::LeadingDash(field));
    }
    if value.chars().any(char::is_control) {
        return Err(SearchQueryError::ControlChar(field));
    }
    Ok(())
}

impl CommitSearchQuery {
    /// Parse the query line. Grammar: whitespace-separated tokens where
    /// `author:`, `path:`, `since:` and `until:` introduce a filter and
    /// every other token joins the message text. `author:` and `path:` may
    /// repeat (git ORs them); a repeated `since:`/`until:` keeps the last
    /// one, since a commit cannot be after two different instants.
    pub fn parse(raw: &str) -> Result<CommitSearchQuery, SearchQueryError> {
        let mut q = CommitSearchQuery::default();
        let mut words: Vec<String> = Vec::new();
        for token in tokenize(raw) {
            let prefix = if token.quoted {
                None
            } else {
                SEARCH_PREFIXES.iter().find(|p| token.text.starts_with(**p))
            };
            let Some(prefix) = prefix else {
                words.push(token.text);
                continue;
            };
            let field = prefix.trim_end_matches(':');
            let value = token.text[prefix.len()..].to_string();
            check_filter_value(field, &value)?;
            match field {
                "author" => q.authors.push(value),
                "path" => q.paths.push(value),
                "since" => q.since = Some(value),
                _ => q.until = Some(value),
            }
        }
        q.text = words.join(" ");
        // The message text rides in a single `--grep=<text>` token, so a
        // leading `-` is harmless there and stays searchable; a control
        // character is refused for the same reason it is in a filter.
        if q.text.chars().any(char::is_control) {
            return Err(SearchQueryError::ControlChar("query"));
        }
        Ok(q)
    }

    /// True when there is nothing to ask git for at all.
    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
            && self.authors.is_empty()
            && self.paths.is_empty()
            && self.since.is_none()
            && self.until.is_none()
    }

    /// Re-check every value at the git boundary. The fields are public and
    /// the worker rewrites `authors` when it expands member aliases, so
    /// having gone through `parse` once is not a guarantee about what
    /// finally reaches the argv.
    pub fn check_safe(&self) -> Result<(), SearchQueryError> {
        for a in &self.authors {
            check_filter_value("author", a)?;
        }
        for p in &self.paths {
            check_filter_value("path", p)?;
        }
        if let Some(s) = &self.since {
            check_filter_value("since", s)?;
        }
        if let Some(u) = &self.until {
            check_filter_value("until", u)?;
        }
        if self.text.chars().any(char::is_control) {
            return Err(SearchQueryError::ControlChar("query"));
        }
        Ok(())
    }
}

/// Search commits (all refs, case-insensitive, literal — not a regex, so a
/// query like "fix(auth)" doesn't need escaping) for the most recent `limit`
/// matches. Used to search across many repositories at once from the Home
/// screen.
///
/// Every pattern is embedded in a single `--grep=`/`--author=`/`--since=`
/// argv token rather than passed as a separate value, so even a value
/// starting with `-` could never be read as its own flag; pathspecs, which
/// *are* separate tokens, are additionally guarded by `--` and by
/// [`CommitSearchQuery::check_safe`].
///
/// git's own semantics do the combining: several `--author` are OR'd with
/// each other and AND'd with the message pattern, and several pathspecs are
/// OR'd.
pub fn search_commits(
    repo_path: &Path,
    query: &CommitSearchQuery,
    limit: usize,
) -> Result<Vec<CommitHit>, String> {
    query.check_safe().map_err(|e| e.to_string())?;
    let limit_s = limit.to_string();
    let grep_arg = (!query.text.is_empty()).then(|| format!("--grep={}", query.text));
    let author_args: Vec<String> = query
        .authors
        .iter()
        .map(|a| format!("--author={a}"))
        .collect();
    let since_arg = query.since.as_ref().map(|s| format!("--since={s}"));
    let until_arg = query.until.as_ref().map(|u| format!("--until={u}"));

    let mut args: Vec<&str> = vec!["log", "--all", "-i", "-F"];
    if let Some(g) = &grep_arg {
        args.push(g);
    }
    args.extend(author_args.iter().map(String::as_str));
    if let Some(s) = &since_arg {
        args.push(s);
    }
    if let Some(u) = &until_arg {
        args.push(u);
    }
    args.extend([
        "-n",
        &limit_s,
        "--format=%h|||%an|||%ad|||%s",
        "--date=format:%Y-%m-%d",
        // Always present, even with no pathspec: it terminates options, so a
        // pathspec can never be read back as one.
        "--",
    ]);
    args.extend(query.paths.iter().map(String::as_str));
    let output = run_git_cmd(repo_path, &args)?;
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
    let output = run_git_cmd_ansi(
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
    let output = run_git_cmd_ansi(
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
    // A commit message is free text and may contain tabs; the preview pane
    // draws it as-is, and a raw tab would move the terminal's cursor rather
    // than indent.
    run_git_cmd(repo_path, &["show", "-s", hash, "--"]).map(|s| {
        s.lines()
            .map(|l| super::exec::expand_tabs(l, super::exec::TAB_WIDTH))
            .collect::<Vec<_>>()
            .join("\n")
    })
}

// Get branch commits in oneline format with graph and colors (git log --graph --oneline --decorate --color=always)
pub fn get_branch_oneline_log(
    repo_path: &Path,
    branch_name: &str,
    limit: usize,
) -> Result<String, String> {
    check_safe_ref(branch_name)?;
    let limit_s = limit.to_string();
    run_git_cmd_ansi(
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
