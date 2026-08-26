use crate::config::{Language, Member};
use crate::git::{
    BlameEntry, BranchInfo, ChangedFile, CommitSummary, Contributor, DiffRowKind, FileDiff,
    GitOpState, StashEntry, Summary, TagInfo, TimeSpan, WorktreeInfo,
};
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Screen {
    Home,
    Repo,
    Diff,
    Settings,
    GlobalMembers,
    RepoFinder,
    CommitSearch,
    Help,
    Log,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FoundRepo {
    pub path: PathBuf,
    pub name: String,
    pub branch: String,
    pub is_already_added: bool,
    pub is_selected: bool,
}

#[derive(Clone, Debug)]
pub struct RepoFinderState {
    pub scan_root: PathBuf,
    pub repos: Vec<FoundRepo>,
    pub selected_idx: usize,
    pub filter: String,
}

#[derive(Clone, Debug)]
pub struct MemberRepoContribution {
    pub repo_name: String,
    pub repo_path: PathBuf,
    pub repo_index: usize,
    pub commit_count: usize,
    pub first_commit: String,
    pub last_commit: String,
}

#[derive(Clone, Debug)]
pub struct GlobalMember {
    pub canonical_name: String,
    pub aliases: Vec<String>,
    pub is_active: bool,
    pub total_commits: usize,
    pub repo_count: usize,
    pub latest_commit_date: String,
    pub contributions: Vec<MemberRepoContribution>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RepoTab {
    Status,
    Commits,
    Branches,
    Tags,
    Stash,
    Contributors,
    Worktrees,
}

impl RepoTab {
    pub fn all() -> [RepoTab; 7] {
        [
            RepoTab::Status,
            RepoTab::Commits,
            RepoTab::Branches,
            RepoTab::Tags,
            RepoTab::Stash,
            RepoTab::Contributors,
            RepoTab::Worktrees,
        ]
    }

    pub fn next(self) -> Self {
        match self {
            Self::Status => Self::Commits,
            Self::Commits => Self::Branches,
            Self::Branches => Self::Tags,
            Self::Tags => Self::Stash,
            Self::Stash => Self::Contributors,
            Self::Contributors => Self::Worktrees,
            Self::Worktrees => Self::Status,
        }
    }

    pub fn prev(self) -> Self {
        match self {
            Self::Status => Self::Worktrees,
            Self::Commits => Self::Status,
            Self::Branches => Self::Commits,
            Self::Tags => Self::Branches,
            Self::Stash => Self::Tags,
            Self::Contributors => Self::Stash,
            Self::Worktrees => Self::Contributors,
        }
    }

    pub fn from_digit(d: char) -> Option<Self> {
        match d {
            '1' => Some(Self::Status),
            '2' => Some(Self::Commits),
            '3' => Some(Self::Branches),
            '4' => Some(Self::Tags),
            '5' => Some(Self::Stash),
            '6' => Some(Self::Contributors),
            '7' => Some(Self::Worktrees),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FocusPane {
    List,
    Hunks,
    Content,
}

impl FocusPane {
    pub fn next(self, has_hunks: bool) -> Self {
        match (self, has_hunks) {
            (Self::List, true) => Self::Hunks,
            (Self::List, false) => Self::Content,
            (Self::Hunks, _) => Self::Content,
            (Self::Content, _) => Self::List,
        }
    }

    pub fn prev(self, has_hunks: bool) -> Self {
        match (self, has_hunks) {
            (Self::List, _) => Self::Content,
            (Self::Hunks, _) => Self::List,
            (Self::Content, true) => Self::Hunks,
            (Self::Content, false) => Self::List,
        }
    }
}

/// `#[serde(default)]` so a cache file written by an older build — one that
/// predates a field added later — still loads instead of being discarded.
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct HomeRow {
    pub branch: String,
    pub ahead: usize,
    pub behind: usize,
    pub dirty: usize,
    pub last_commit: String,
    pub open_prs: Option<usize>,
    pub ci_status: Option<String>,
    pub op_state: GitOpState,
    pub conflicts: usize,
}

#[derive(Clone, Debug)]
pub struct RepoSnapshot {
    pub summary: Summary,
    pub commits: Vec<CommitSummary>,
    pub commits_err: Option<String>,
    pub branches: Vec<BranchInfo>,
    pub branches_err: Option<String>,
    pub tags: Vec<TagInfo>,
    pub tags_err: Option<String>,
    pub stashes: Vec<StashEntry>,
    pub stashes_err: Option<String>,
    pub working_files: Vec<ChangedFile>,
    pub working_err: Option<String>,
    pub contributors: Vec<Contributor>,
    pub contributors_err: Option<String>,
    pub worktrees: Vec<WorktreeInfo>,
    pub worktrees_err: Option<String>,
}

#[derive(Clone, Debug)]
pub struct LogView {
    pub title: String,
    pub body: String,
    pub scroll: usize,
}

#[derive(Clone, Debug)]
pub struct DiffLine {
    pub kind: DiffRowKind,
    pub text: String,
    pub old_no: Option<usize>,
    pub new_no: Option<usize>,
}

impl Default for DiffLine {
    fn default() -> Self {
        Self {
            kind: DiffRowKind::Context,
            text: String::new(),
            old_no: None,
            new_no: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hunk {
    pub start: usize,
    pub end: usize,
    pub label: String,
}

#[derive(Clone, Debug)]
pub struct DiffView {
    pub title: String,
    pub target: String,
    pub base: Option<String>,
    pub three_dot: bool,
    pub files: Vec<ChangedFile>,
    pub file_idx: usize,
    pub lines: Vec<DiffLine>,
    pub hunks: Vec<Hunk>,
    pub hunk_idx: usize,
    pub scroll: usize,
    pub loading: bool,
    pub header: Option<String>,
    pub error: Option<String>,
    /// Per-line blame for the file currently shown, indexed by new-file line
    /// number - 1 (i.e. `DiffLine::new_no`). `None` until requested — blame is
    /// a second git invocation per file, so it is opt-in via `b` rather than
    /// always fetched alongside the diff.
    pub blame: Option<Vec<BlameEntry>>,
    pub blame_loading: bool,
    /// Scroll position to restore once the in-flight reload completes,
    /// bypassing apply_hunks' usual "jump to the first hunk" behavior. Set
    /// when reloading only to change view density (ignore-whitespace,
    /// full-file context) rather than to look at a different file — for
    /// those, jumping back to line 1 defeats the point of the toggle.
    pub pending_scroll_restore: Option<usize>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum SettingsTab {
    #[default]
    Repositories,
    Members,
}

#[derive(Clone, Debug)]
pub enum Confirm {
    DeleteRepo(usize),
    DeleteMember(usize),
    DropStash { repo_idx: usize, stash_ref: String },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputKind {
    Filter,
    AddRepo,
    AddAlias,
    Rename,
    EditRepoGroup,
    AddMemberName,
    AddMemberAliases,
    EditMemberAliases,
    DiffCommand,
    FinderFilter,
    FinderScanPath,
    CommitSearchQuery,
}

/// One commit search result, tagged with which repository it came from
/// (`git::CommitHit` doesn't know about the repository list).
#[derive(Clone, Debug)]
pub struct CommitSearchHit {
    pub repo_index: usize,
    pub repo_name: String,
    pub hash: String,
    pub author: String,
    pub date: String,
    pub message: String,
}

#[derive(Clone, Debug, Default)]
pub struct CommitSearchState {
    pub query: String,
    pub hits: Vec<CommitSearchHit>,
    pub selected: usize,
    pub loading: bool,
}

#[derive(Clone, Debug)]
pub struct ExternalDiff {
    pub program: String,
    pub args: Vec<String>,
    pub cwd: PathBuf,
}

#[derive(Clone, Debug)]
pub struct CommitPreview {
    pub hash: String,
    pub header: String,
    pub files: Vec<ChangedFile>,
}

pub enum Job {
    LoadHome {
        generation: u64,
        index: usize,
        path: PathBuf,
        members: Vec<Member>,
    },
    LoadRepo {
        index: usize,
        path: PathBuf,
        members: Vec<Member>,
        time_span: TimeSpan,
    },
    LoadDiff {
        seq: u64,
        path: PathBuf,
        base: Option<String>,
        target: String,
        file: String,
        three_dot: bool,
        ignore_whitespace: bool,
        full: bool,
    },
    LoadBlame {
        seq: u64,
        path: PathBuf,
        blame_ref: String,
        file: String,
    },
    LoadCommitMeta {
        seq: u64,
        path: PathBuf,
        hash: String,
    },
    LoadFiles {
        seq: u64,
        path: PathBuf,
        base: Option<String>,
        target: String,
        three_dot: bool,
        preselect: Option<String>,
    },
    Pull {
        index: usize,
        path: PathBuf,
        lang: Language,
    },
    Fetch {
        index: usize,
        path: PathBuf,
        lang: Language,
    },
    StashApply {
        path: PathBuf,
        stash_ref: String,
    },
    StashDrop {
        path: PathBuf,
        stash_ref: String,
    },
    LoadBranchLog {
        path: PathBuf,
        branch: String,
    },
    LoadCommitPreview {
        seq: u64,
        path: PathBuf,
        hash: String,
    },
    LoadGlobalMembers {
        generation: u64,
        repos: Vec<(usize, String, PathBuf)>,
        members: Vec<Member>,
    },
    SearchCommits {
        seq: u64,
        query: String,
        repos: Vec<(usize, String, PathBuf)>,
    },
}

impl Job {
    /// Jobs that either talk to a remote (up to `GIT_NETWORK_TIMEOUT`) or run
    /// one git invocation *per registered repository* (member/commit-search
    /// aggregation). Both shapes can take far longer than a single-repo diff
    /// or status load, so they run on their own worker — otherwise a slow
    /// `pull`, or a cross-repo search that touches an unreachable SSH host,
    /// would stall every diff/repo load queued behind it on the shared one.
    pub fn is_secondary_worker(&self) -> bool {
        matches!(
            self,
            Job::Pull { .. }
                | Job::Fetch { .. }
                | Job::SearchCommits { .. }
                | Job::LoadGlobalMembers { .. }
        )
    }
}

pub enum Msg {
    HomeLoaded {
        generation: u64,
        index: usize,
        row: Result<HomeRow, String>,
    },
    RepoLoaded {
        index: usize,
        data: Box<Result<RepoSnapshot, String>>,
    },
    DiffLoaded {
        seq: u64,
        result: Result<FileDiff, String>,
    },
    CommitMeta {
        seq: u64,
        header: Result<String, String>,
        files: Result<Vec<ChangedFile>, String>,
    },
    FilesLoaded {
        seq: u64,
        files: Result<Vec<ChangedFile>, String>,
        preselect: Option<String>,
    },
    OpDone {
        ok: bool,
        text: String,
        /// Repository the operation ran against, so only that row is
        /// re-analysed. A blanket refresh re-queued every repository, and each
        /// of those refreshes queued another one per completing pull.
        repo_index: Option<usize>,
    },
    LogLoaded {
        title: String,
        body: Result<String, String>,
    },
    CommitPreviewLoaded {
        seq: u64,
        hash: String,
        header: Result<String, String>,
        files: Result<Vec<ChangedFile>, String>,
    },
    BlameLoaded {
        seq: u64,
        result: Result<Vec<BlameEntry>, String>,
    },
    CommitSearchLoaded {
        seq: u64,
        hits: Vec<CommitSearchHit>,
        /// Names of repos the search itself failed against (unreachable SSH
        /// host, `git` missing, timeout, ...) — distinct from a repo that
        /// was searched successfully and simply had no matches.
        failed_repos: Vec<String>,
    },
    GlobalMembersLoaded {
        generation: u64,
        list: Vec<GlobalMember>,
    },
}

/// Where the repository-detail item list was drawn, and how far it had
/// scrolled, as of the last frame.
///
/// A mouse click arrives as raw screen coordinates. Turning those back into
/// an item index needs the list's inner rectangle and its scroll offset, and
/// only the renderer knows either — the offset in particular is chosen by
/// ratatui while drawing, not by us. Recording it is the same approach
/// `home_offset` already uses for the Home table.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ListViewport {
    /// Screen y of the first item row (inside the border).
    pub y: u16,
    /// How many item rows are visible.
    pub height: u16,
    /// Screen x of the first item cell (inside the border).
    pub x: u16,
    /// Index, within the visible/filtered list, of the item drawn at `y`.
    pub offset: usize,
}

impl ListViewport {
    /// The index this screen row refers to, or `None` if the row is outside
    /// the list. Returns an index into the *filtered* list, the same space
    /// `list_selected` lives in.
    pub fn index_at(&self, row: u16) -> Option<usize> {
        if self.height == 0 || row < self.y || row >= self.y.saturating_add(self.height) {
            return None;
        }
        Some(self.offset + (row - self.y) as usize)
    }
}

/// A long-running operation attributed to one repository, so its Home row can
/// say what is happening to it rather than sitting there looking stale.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Activity {
    Refresh,
    Pull,
    Fetch,
}

impl Activity {
    /// The git verb, not a translated word: these name the command being run,
    /// and `pull`/`fetch` are what the user typed to start them.
    pub fn label(self) -> &'static str {
        match self {
            Activity::Refresh => "refresh",
            Activity::Pull => "pull",
            Activity::Fetch => "fetch",
        }
    }
}

/// Braille frames, one column wide each, so a spinner fits a narrow table cell.
pub const SPINNER_FRAMES: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

/// Milliseconds per frame. The event loop polls on a 100 ms timeout and
/// redraws each pass, so anything at or above that turns over every frame.
pub const SPINNER_INTERVAL_MS: u128 = 100;
