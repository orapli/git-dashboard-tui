use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct Summary {
    pub repo_name: String,
    pub repo_path: String,
    pub current_branch: String,
    pub total_commits: usize,
    pub total_contributors: usize,
    pub total_branches: usize,
    pub total_files: usize,
    pub total_size_bytes: u64,
    pub total_size_formatted: String,
    pub has_upstream: bool,
    pub ahead: usize,
    pub behind: usize,
    pub has_remote: bool,
    pub uncommitted_changes: usize,
    #[serde(default)]
    pub remote_ci_pr: Option<RemoteCiPrInfo>,
    /// A merge/rebase/cherry-pick/revert left mid-operation by work done
    /// outside the dashboard (this tool never starts one itself).
    #[serde(default)]
    pub op_state: GitOpState,
    /// Unresolved-conflict file count, from the same `status --porcelain`
    /// output already fetched for `uncommitted_changes`.
    #[serde(default)]
    pub conflicts: usize,
}

/// A git operation left unfinished — always caused by something outside this
/// dashboard (a shell, another tool), since the dashboard has no write
/// operations that could start one. Surfaced so a user who left the app open
/// notices it without having to jump to a shell to check.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum GitOpState {
    #[default]
    None,
    Merge,
    Rebase,
    CherryPick,
    Revert,
}

impl GitOpState {
    pub fn label(&self) -> &'static str {
        match self {
            GitOpState::None => "",
            GitOpState::Merge => "MERGE",
            GitOpState::Rebase => "REBASE",
            GitOpState::CherryPick => "CHERRY-PICK",
            GitOpState::Revert => "REVERT",
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Contributor {
    pub name: String,
    pub email: String,
    pub commit_count: usize,
    pub percentage: f64,
    pub first_commit: String,
    pub last_commit: String,
    pub is_member: bool,
    pub is_active: bool,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum TimeSpan {
    #[default]
    All,
    OneWeek,
    OneMonth,
    ThreeMonths,
}

impl TimeSpan {
    pub fn next(&self) -> Self {
        match self {
            Self::All => Self::OneWeek,
            Self::OneWeek => Self::OneMonth,
            Self::OneMonth => Self::ThreeMonths,
            Self::ThreeMonths => Self::All,
        }
    }

    pub fn label_en(&self) -> &'static str {
        match self {
            Self::All => "All time",
            Self::OneWeek => "1 Week",
            Self::OneMonth => "1 Month",
            Self::ThreeMonths => "3 Months",
        }
    }

    pub fn label_ja(&self) -> &'static str {
        match self {
            Self::All => "全期間",
            Self::OneWeek => "直近1週間",
            Self::OneMonth => "直近1ヶ月",
            Self::ThreeMonths => "直近3ヶ月",
        }
    }

    pub fn since_arg(&self) -> Option<&'static str> {
        match self {
            Self::All => None,
            Self::OneWeek => Some("1 week ago"),
            Self::OneMonth => Some("1 month ago"),
            Self::ThreeMonths => Some("3 months ago"),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct BranchInfo {
    pub name: String,
    pub is_remote: bool,
    pub author: String,
    pub date: String,
    pub date_unix: i64,
    pub message: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct DailyActivity {
    pub dates: Vec<String>,
    pub counts: Vec<usize>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct Activity {
    pub daily: DailyActivity,
    pub hourly: Vec<usize>,
    pub weekly: Vec<usize>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct FileExtInfo {
    pub ext: String,
    pub count: usize,
    pub percentage: f64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct FileInfo {
    pub path: String,
    pub size_bytes: u64,
    pub size_formatted: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct FilesReport {
    pub extensions: Vec<FileExtInfo>,
    pub largest_files: Vec<FileInfo>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct StashEntry {
    pub ref_name: String,
    pub date_relative: String,
    pub author: String,
    pub message: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct BlameEntry {
    pub hash: String,
    pub author: String,
    pub date: String,
    pub summary: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct CommitRef {
    pub name: String,
    pub is_head: bool,
    pub is_tag: bool,
    pub is_remote: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct CommitSummary {
    pub hash: String,
    pub author: String,
    pub date: String,
    pub message: String,
    pub graph: String,
    pub refs: Vec<CommitRef>,
}

/// A single hit from [`crate::git::search_commits`], without repository
/// context — the caller attaches that when aggregating across repositories.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommitHit {
    pub hash: String,
    pub author: String,
    pub date: String,
    pub message: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct SyncStatus {
    pub has_upstream: bool,
    pub ahead: usize,
    pub behind: usize,
    pub has_remote: bool,
}

// Git fetch repository

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct TagInfo {
    pub name: String,
    pub date: String,
    pub message: String,
    /// Short hash of the commit the tag points to (annotated tags are
    /// dereferenced). serde(default): older disk caches lack this field —
    /// such tags just miss their badge until the next refresh.
    #[serde(default)]
    pub hash: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct TechInfo {
    pub language: String,
    pub lang_version: String,
    pub framework: String,
    pub framework_version: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct VersionFileRule {
    pub file: String,
    pub regex: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct TechRule {
    pub name: String,
    pub language: String,
    pub lang_version_files: Vec<VersionFileRule>,
    pub framework: String,
    pub framework_version_files: Vec<VersionFileRule>,
}

#[derive(Clone, Debug)]
pub struct ChangedFile {
    pub status: String, // "M" | "A" | "D" | "R" | "?"
    pub path: String,   // display path (new path for renames)
    pub old_path: Option<String>,
    pub additions: usize,
    pub deletions: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub enum DiffRowKind {
    Context,
    Added,
    Removed,
    Modified, // left=removed, right=added
}

#[derive(Clone, Debug)]
pub struct DiffRow {
    pub kind: DiffRowKind,
    pub left_no: Option<usize>,
    pub left_text: Option<String>,
    pub left_tokens: Option<Vec<crate::syntax::Token>>,
    pub right_no: Option<usize>,
    pub right_text: Option<String>,
    pub right_tokens: Option<Vec<crate::syntax::Token>>,
}

#[derive(Clone, Debug)]
pub struct FileDiff {
    pub path: String,
    pub is_binary: bool,
    pub truncated: bool,
    pub rows: Vec<DiffRow>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct WorktreeInfo {
    pub path: String,
    pub head: String,
    pub branch: Option<String>,
    pub is_bare: bool,
    pub is_detached: bool,
    pub is_locked: bool,
    pub is_prunable: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Default)]
pub struct RemoteCiPrInfo {
    pub open_prs: Option<usize>,
    pub ci_status: Option<String>, // "success" | "failure" | "pending"
    pub last_run_url: Option<String>,
}
