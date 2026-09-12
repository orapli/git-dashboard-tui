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

/// What the checked-out branch tracks.
///
/// A bare `has_upstream: bool` cannot express the difference that matters to
/// a reader of the Home list: `↑0 ↓0` on a branch with no upstream is not
/// "in sync", it is "nothing was ever compared". The `Unknown` variant exists
/// for rows restored from a cache written before this was recorded — guessing
/// `NoRemote` there would turn every cached row into a false "local only".
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum UpstreamState {
    /// Not recorded (a Home row cached by an older build).
    #[default]
    Unknown,
    /// No remote is configured at all — nothing to be ahead of or behind.
    NoRemote,
    /// A remote exists, but the current branch has no upstream set
    /// (typically a branch that was never pushed).
    NoUpstream,
    /// Tracking an upstream: the ahead/behind counts are meaningful.
    Tracking,
}

/// When the repository last talked to its remote, taken from the mtime of
/// `FETCH_HEAD`.
///
/// Ahead/behind counts are only as fresh as the last fetch, so this is the
/// age of the *remote* half of the Home row — quite different from when the
/// dashboard last read the working tree.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum LastFetch {
    /// Not recorded (a Home row cached by an older build).
    #[default]
    Unknown,
    /// No local `FETCH_HEAD` can be stat-ed — an `ssh://` repository lives on
    /// another machine, so the age is unavailable rather than "never".
    Unavailable,
    /// The repository has never fetched: no `FETCH_HEAD` exists.
    Never,
    /// Unix timestamp of the last fetch.
    At(i64),
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

/// The pull request opened from the repository's current branch, when there
/// is one. `gh pr list` already returns every open pull request, so this is
/// picked out of a reply the dashboard was fetching anyway — no extra call.
///
/// `#[serde(default)]`: a cache file written before a field existed still
/// loads, the missing field simply reading as its default.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Default)]
#[serde(default)]
pub struct BranchPr {
    pub number: usize,
    /// Sanitised and length-capped at parse time: a pull request title is
    /// text a stranger can write, and it is rendered into a terminal.
    pub title: String,
    pub url: String,
    pub is_draft: bool,
    /// GitHub's `reviewDecision`: `APPROVED`, `CHANGES_REQUESTED`,
    /// `REVIEW_REQUIRED`, or absent when the repository requires no review.
    pub review_decision: Option<String>,
}

impl BranchPr {
    fn decision_is(&self, expected: &str) -> bool {
        self.review_decision
            .as_deref()
            .is_some_and(|d| d.eq_ignore_ascii_case(expected))
    }

    /// A reviewer asked for changes — the one review outcome that puts the
    /// ball back in the author's court, so the only one wired to the
    /// "needs attention" filter.
    pub fn changes_requested(&self) -> bool {
        self.decision_is("CHANGES_REQUESTED")
    }

    pub fn approved(&self) -> bool {
        self.decision_is("APPROVED")
    }
}

/// What the cached `gh` lookups know about one repository.
///
/// `#[serde(default)]` on the struct is what lets a cache file written by an
/// older build load after fields are added here — every field added must be
/// defaultable, and adding one must never be a reason to discard a cache.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Default)]
#[serde(default)]
pub struct RemoteCiPrInfo {
    pub ci_state: GithubState,
    pub pr_state: GithubState,
    /// The branch the CI answer is *about* — the run list is queried scoped
    /// to the repository's current branch, so this is that branch, not
    /// whichever branch happened to push last.
    pub ci_branch: Option<String>,
    pub ci_fetched_at: i64,
    pub pr_fetched_at: i64,
    pub checked_at: i64,
    pub open_prs: Option<usize>,
    pub ci_status: Option<String>, // "success" | "failure" | "pending"
    pub last_run_url: Option<String>,
    /// The open pull request whose head branch is the current branch.
    ///
    /// Boxed to keep this struct pointer-sized in that field: it travels
    /// inside `HomeRow`, which travels inside the worker's `Msg` enum, and
    /// every other message would otherwise pay for it.
    pub branch_pr: Option<Box<BranchPr>>,
    /// Open pull requests in this repository waiting on the current user's
    /// review. `None` means the lookup has never succeeded.
    pub review_requests: Option<usize>,
    /// Outcome of the review-request lookup. Kept separate from `pr_state`
    /// because that lookup is the optional part of a refresh: when it fails
    /// the row loses one signal and nothing else changes.
    pub review_state: GithubState,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum GithubState {
    #[default]
    Unknown,
    Ready,
    NoRuns,
    Unauthenticated,
    Failed,
    Unavailable,
    Unsupported,
    /// No branch to scope a query to — the repository is on a detached HEAD.
    /// A distinct state rather than a silent fall-back to the
    /// repository-wide latest run, which would claim to describe a branch
    /// the user is not on.
    Detached,
    /// `gh` did not answer inside its timeout. Distinct from `Failed` so the
    /// panel can say "slow or unreachable" rather than "fetch failed", and
    /// so the retry can back off further than a plain failure does.
    TimedOut,
}

impl GithubState {
    /// Whether this is a complete answer rather than a missing one. A
    /// detached HEAD counts: "there is no branch to ask about" is as settled
    /// as "the branch has no runs", and neither is worth re-asking sooner
    /// than the refresh period or flagging as unverified.
    pub fn is_settled(self) -> bool {
        matches!(self, Self::Ready | Self::NoRuns | Self::Detached)
    }
}
