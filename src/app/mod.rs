pub mod finder;
pub mod handlers;
pub mod helpers;
pub mod types;
pub mod worker;

pub use finder::*;
pub use helpers::*;
pub use types::*;
pub use worker::*;

use crate::config::{self, Language, Member, Repository};
use crate::git::{self, TimeSpan};
use crate::i18n;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};

/// Which config files could not be read. Saving over a file we failed to parse
/// would replace the user's data with the empty default we fell back to, so
/// each flag disables writes to that one file until the app is restarted.
#[derive(Default)]
struct ConfigLoadState {
    repos_failed: bool,
    members_failed: bool,
    prefs_failed: bool,
}

fn unwrap_config<T: Default>(
    result: Result<T, String>,
    failed: &mut bool,
    errors: &mut Vec<String>,
) -> T {
    match result {
        Ok(v) => v,
        Err(e) => {
            *failed = true;
            errors.push(e);
            T::default()
        }
    }
}

pub struct App {
    pub repos: Vec<Repository>,
    members: Vec<Member>,
    prefs: config::Preferences,
    config_state: ConfigLoadState,
    pub screen: Screen,
    pub home_filter: String,
    pub home_selected: usize,
    /// First table row currently on screen, recorded by the renderer so a
    /// mouse click can be mapped back to a repository once the list scrolls.
    pub home_offset: std::cell::Cell<usize>,
    /// Screen x-range of each Home table column header, recorded by the
    /// renderer. Click-to-sort hit-tests against these rather than recomputing
    /// the layout, so the two can never disagree about where a column is.
    pub home_col_bounds: std::cell::RefCell<Vec<(u16, u16)>>,
    /// Where the repository-detail item list is on screen and how far it has
    /// scrolled, recorded by the renderer. Same reason as `home_offset`: a
    /// click carries screen coordinates, and only the renderer knows what row
    /// they landed on.
    pub list_viewport: std::cell::Cell<ListViewport>,
    /// Screen x-range of each repository tab, recorded by the renderer. The
    /// click handler used fixed ranges computed from the English labels, so it
    /// mapped clicks to the wrong tab in Japanese — and would again whenever a
    /// label changed.
    pub tab_bounds: std::cell::RefCell<Vec<(u16, u16)>>,
    /// First visible help line. Clamped by the renderer, which is the only
    /// place that knows how tall the help box ended up.
    pub help_scroll: std::cell::Cell<usize>,
    pub home_rows: HashMap<usize, HomeRow>,
    pub repo_tab: RepoTab,
    pub repo_index: Option<usize>,
    pub repo_data: Option<RepoSnapshot>,
    pub repo_loading: bool,
    pub list_selected: usize,
    pub focus: FocusPane,
    pub diff: Option<DiffView>,
    pub settings_selected: usize,
    pub settings_tab: SettingsTab,
    pub settings_member_selected: usize,
    pub group_filter: Option<String>,
    pub contributor_time_span: TimeSpan,
    pub global_members: Vec<GlobalMember>,
    pub global_members_loading: bool,
    /// What is currently running against each repository, keyed by index.
    ///
    /// Keyed rather than counted on purpose: a stale or duplicated message
    /// can only remove an entry that is already gone, so the map cannot drift
    /// into a spinner that never stops — which a bare counter can.
    pub busy: HashMap<usize, Activity>,
    /// Fixed point for the spinner's phase. Driving it off elapsed time rather
    /// than a per-draw counter keeps the rate steady no matter how often the
    /// screen happens to be redrawn.
    started: std::time::Instant,
    global_gen: u64,
    pub global_member_selected: usize,
    pub global_member_repo_selected: usize,
    pub global_member_pane: FocusPane,
    pub global_member_filter: String,
    pub status: String,
    pub error: Option<String>,
    pub should_quit: bool,
    pub tag_base: Option<String>,
    pub tag_target: Option<String>,
    pub commit_base: Option<String>,
    pub commit_target: Option<String>,
    pub commit_preview: Option<CommitPreview>,
    pub list_filter: String,
    pub log: Option<LogView>,
    pub active_only: bool,
    /// Home filter: only repos with a failing CI run, an unresolved conflict,
    /// or a merge/rebase/etc. left mid-operation. Toggled with `n`.
    pub attention_only: bool,
    pub repo_finder: Option<RepoFinderState>,
    pub path_completions: Vec<String>,
    pub path_completion_idx: usize,
    input: Option<InputKind>,
    input_buf: String,
    confirm: Option<Confirm>,
    help_return: Option<Screen>,
    home_gen: Arc<AtomicU64>,
    pending_add_path: Option<PathBuf>,
    rename_idx: Option<usize>,
    editing_repo_group_idx: Option<usize>,
    pending_member_name: Option<String>,
    editing_member_idx: Option<usize>,
    pending_external: Option<ExternalDiff>,
    pending_terminal: Option<PathBuf>,
    preview_seq: u64,
    job_tx: Sender<Job>,
    bulk_tx: Sender<Job>,
    msg_rx: Receiver<Msg>,
    diff_seq: u64,
    last_auto_refresh: std::time::Instant,
    pub commit_search: Option<CommitSearchState>,
    search_seq: u64,
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

impl App {
    pub fn new() -> Self {
        let (job_tx, job_rx) = mpsc::channel::<Job>();
        let (bulk_tx, bulk_rx) = mpsc::channel::<Job>();
        let (msg_tx, msg_rx) = mpsc::channel::<Msg>();
        let home_gen = Arc::new(AtomicU64::new(0));
        spawn_worker(job_rx, msg_tx.clone(), Arc::clone(&home_gen));
        // Remote operations and cross-repo (one-git-call-per-repo)
        // aggregations get their own worker: a 120 s `pull`, or a commit
        // search that reaches an unresponsive SSH host, would otherwise
        // block every diff and single-repo load queued behind it.
        spawn_worker(bulk_rx, msg_tx, Arc::clone(&home_gen));

        let mut load_errors = Vec::new();
        let mut config_state = ConfigLoadState::default();
        let repos = unwrap_config(
            config::load_repositories(),
            &mut config_state.repos_failed,
            &mut load_errors,
        );
        let members = unwrap_config(
            config::load_members(),
            &mut config_state.members_failed,
            &mut load_errors,
        );
        let prefs = unwrap_config(
            config::load_preferences(),
            &mut config_state.prefs_failed,
            &mut load_errors,
        );
        let mut app = Self {
            repos,
            members,
            prefs,
            config_state,
            screen: Screen::Home,
            home_filter: String::new(),
            home_selected: 0,
            home_offset: std::cell::Cell::new(0),
            home_col_bounds: std::cell::RefCell::new(Vec::new()),
            list_viewport: std::cell::Cell::new(ListViewport::default()),
            tab_bounds: std::cell::RefCell::new(Vec::new()),
            help_scroll: std::cell::Cell::new(0),
            home_rows: HashMap::new(),
            repo_tab: RepoTab::Commits,
            repo_index: None,
            repo_data: None,
            repo_loading: false,
            list_selected: 0,
            focus: FocusPane::List,
            diff: None,
            settings_selected: 0,
            settings_tab: SettingsTab::Repositories,
            settings_member_selected: 0,
            group_filter: None,
            contributor_time_span: TimeSpan::All,
            global_members: Vec::new(),
            global_members_loading: false,
            busy: HashMap::new(),
            started: std::time::Instant::now(),
            global_gen: 0,
            global_member_selected: 0,
            global_member_repo_selected: 0,
            global_member_pane: FocusPane::List,
            global_member_filter: String::new(),
            status: String::new(),
            error: None,
            should_quit: false,
            tag_base: None,
            tag_target: None,
            commit_base: None,
            commit_target: None,
            commit_preview: None,
            list_filter: String::new(),
            log: None,
            active_only: false,
            attention_only: false,
            repo_finder: None,
            path_completions: Vec::new(),
            path_completion_idx: 0,
            input: None,
            input_buf: String::new(),
            confirm: None,
            help_return: None,
            home_gen,
            pending_add_path: None,
            rename_idx: None,
            editing_repo_group_idx: None,
            pending_member_name: None,
            editing_member_idx: None,
            pending_external: None,
            pending_terminal: None,
            preview_seq: 0,
            job_tx,
            bulk_tx,
            msg_rx,
            diff_seq: 0,
            last_auto_refresh: std::time::Instant::now(),
            commit_search: None,
            search_seq: 0,
        };
        if !load_errors.is_empty() {
            app.error = Some(load_errors.join(" / "));
        }
        // Populate from cache before the first refresh is even queued, so the
        // dashboard renders with real numbers immediately rather than a screen
        // of `…` while every repository is analysed one at a time.
        app.load_cached_home_rows();
        app.refresh_home();
        app
    }

    /// Fill `home_rows` from each repository's cached row. Purely local file
    /// reads — no git — so this stays fast even with many repositories.
    fn load_cached_home_rows(&mut self) {
        for (i, repo) in self.repos.iter().enumerate() {
            if let Some(row) = load_home_cache(&repo.path) {
                self.home_rows.insert(i, row);
            }
        }
    }

    pub fn members(&self) -> &[Member] {
        &self.members
    }

    /// The current spinner frame. Advances with wall-clock time, so every
    /// spinner on screen turns in step.
    pub fn spinner(&self) -> &'static str {
        let n = self.started.elapsed().as_millis() / SPINNER_INTERVAL_MS;
        SPINNER_FRAMES[(n as usize) % SPINNER_FRAMES.len()]
    }

    /// Wording for the title-bar indicator. Singular/plural in English, and
    /// a counter suffix in Japanese, which has neither.
    pub fn busy_label(&self, n: usize) -> String {
        match self.lang() {
            Language::Japanese => format!("実行中 {n} 件"),
            Language::English if n == 1 => "1 job running".to_string(),
            Language::English => format!("{n} jobs running"),
        }
    }

    /// What is running against this repository, if anything.
    pub fn activity(&self, repo_index: usize) -> Option<Activity> {
        self.busy.get(&repo_index).copied()
    }

    /// How many background operations are in flight, counting the per-repo
    /// ones and the screens that track their own load state.
    pub fn busy_count(&self) -> usize {
        let flags = [
            self.repo_loading,
            self.global_members_loading,
            self.diff.as_ref().is_some_and(|d| d.loading),
            self.diff.as_ref().is_some_and(|d| d.blame_loading),
            self.commit_search.as_ref().is_some_and(|s| s.loading),
        ];
        self.busy.len() + flags.iter().filter(|f| **f).count()
    }

    pub fn is_busy(&self) -> bool {
        self.busy_count() > 0
    }

    pub fn lang(&self) -> Language {
        self.prefs.language
    }

    pub fn t(&self, key: &str) -> String {
        i18n::t(self.prefs.language, key)
    }

    pub fn tt(&self, en: &str, ja: &str) -> String {
        match self.prefs.language {
            Language::Japanese => ja.to_string(),
            Language::English => en.to_string(),
        }
    }

    pub fn is_filtering(&self) -> bool {
        matches!(self.input, Some(InputKind::Filter))
    }

    pub fn is_adding_repo(&self) -> bool {
        matches!(
            self.input,
            Some(
                InputKind::AddRepo
                    | InputKind::AddAlias
                    | InputKind::FinderScanPath
                    | InputKind::Rename
                    | InputKind::EditRepoGroup
                    | InputKind::AddMemberName
                    | InputKind::AddMemberAliases
                    | InputKind::EditMemberAliases
                    | InputKind::DiffCommand
                    | InputKind::CommitSearchQuery
            )
        )
    }

    pub fn prompt_title(&self) -> String {
        match self.input {
            Some(InputKind::AddRepo) => self.tt(
                "Add repository  (~/path or /abs/path, Tab to complete)",
                "リポジトリ追加  (~/path または 絶対パス, Tabで補完)",
            ),
            Some(InputKind::AddAlias) => self.tt(
                "Display name / alias (Enter to use folder name)",
                "表示名 / 別名 (Enter でフォルダ名)",
            ),
            Some(InputKind::FinderScanPath) => self.tt(
                "Scan folder for Git repos (Tab to complete)",
                "Gitリポジトリを検索する親フォルダ (Tabで補完)",
            ),
            Some(InputKind::Rename) => self.tt("Rename alias", "別名を変更"),
            Some(InputKind::EditRepoGroup) => self.tt(
                "Repository group (e.g. Work, Personal, empty to clear)",
                "リポジトリのグループ (例: Work, Personal, 空で解除)",
            ),
            Some(InputKind::AddMemberName) => self.tt("New member name", "新規メンバー名"),
            Some(InputKind::AddMemberAliases) => self.tt(
                "Member aliases / Git commit authors (comma separated)",
                "メンバーの別名 / Gitコミット名 (カンマ区切り)",
            ),
            Some(InputKind::EditMemberAliases) => self.tt(
                "Edit member aliases / Git commit authors (comma separated)",
                "メンバーの別名を編集 / Gitコミット名 (カンマ区切り)",
            ),
            Some(InputKind::DiffCommand) => self.tt(
                "Diff tool command (e.g. 'code --wait --diff', empty for builtin)",
                "Diff ツールコマンド (例: 'code --wait --diff', 空で内蔵)",
            ),
            Some(InputKind::CommitSearchQuery) => self.tt(
                "Search commit messages across all repositories",
                "全リポジトリ横断でコミットメッセージを検索",
            ),
            _ => String::new(),
        }
    }

    pub fn input_buf(&self) -> &str {
        &self.input_buf
    }

    pub fn take_external(&mut self) -> Option<ExternalDiff> {
        self.pending_external.take()
    }

    pub fn take_terminal(&mut self) -> Option<PathBuf> {
        self.pending_terminal.take()
    }

    pub fn open_terminal_for_current_repo(&mut self) {
        let path = match self.screen {
            Screen::Home => {
                let filtered = self.filtered_home();
                filtered
                    .get(self.home_selected)
                    .and_then(|&i| self.repos.get(i))
                    .map(|r| r.path.clone())
            }
            Screen::Repo => self
                .repo_index
                .and_then(|i| self.repos.get(i))
                .map(|r| r.path.clone()),
            Screen::GlobalMembers => {
                let filtered = self.filtered_global_members();
                filtered
                    .get(self.global_member_selected)
                    .and_then(|&mi| self.global_members.get(mi))
                    .and_then(|m| m.contributions.get(self.global_member_repo_selected))
                    .map(|c| c.repo_path.clone())
            }
            Screen::Diff => self
                .repo_index
                .and_then(|i| self.repos.get(i))
                .map(|r| r.path.clone()),
            _ => None,
        };

        if let Some(p) = path {
            if git::parse_ssh_repo(&p).is_some() {
                self.error = Some(self.tt(
                    "Cannot open local terminal for remote SSH repository",
                    "リモートSSHリポジトリのローカルターミナルは開けません",
                ));
            } else if p.exists() {
                self.pending_terminal = Some(p);
            } else {
                self.error = Some(self.tt(
                    "Repository path does not exist",
                    "リポジトリのパスが存在しません",
                ));
            }
        }
    }

    pub fn diff_ignore_whitespace(&self) -> bool {
        self.prefs.diff_ignore_whitespace
    }

    pub fn diff_full_file(&self) -> bool {
        self.prefs.diff_full_file
    }

    pub fn diff_show_blame(&self) -> bool {
        self.prefs.diff_show_blame
    }

    pub fn theme_name(&self) -> &str {
        &self.prefs.theme
    }

    fn toggle_theme(&mut self) {
        self.prefs.theme = if self.prefs.theme == crate::colors::THEME_LATTE {
            crate::colors::THEME_MOCHA.to_string()
        } else {
            crate::colors::THEME_LATTE.to_string()
        };
        self.persist_prefs();
        self.status = format!("{} {}", self.tt("Theme:", "テーマ:"), self.prefs.theme);
    }

    pub fn diff_tool_label(&self) -> String {
        let c = self.prefs.diff_command.trim();
        if c.is_empty() {
            self.tt("builtin", "内蔵")
        } else {
            c.to_string()
        }
    }

    pub fn sort_mode(&self) -> usize {
        self.prefs.repo_sort
    }

    pub fn sort_label(&self) -> String {
        match self.prefs.repo_sort {
            SORT_NAME_ASC => self.tt("name ↑", "名前 ↑"),
            SORT_NAME_DESC => self.tt("name ↓", "名前 ↓"),
            SORT_UPDATED_DESC => self.tt("updated ↓", "更新 ↓"),
            SORT_UPDATED_ASC => self.tt("updated ↑", "更新 ↑"),
            SORT_BRANCH_ASC => self.tt("branch ↑", "ブランチ ↑"),
            SORT_BRANCH_DESC => self.tt("branch ↓", "ブランチ ↓"),
            SORT_DIRTY_DESC => self.tt("dirty ↓", "未コミット ↓"),
            SORT_DIRTY_ASC => self.tt("dirty ↑", "未コミット ↑"),
            SORT_SYNC_DESC => self.tt("sync ↓", "同期 ↓"),
            SORT_SYNC_ASC => self.tt("sync ↑", "同期 ↑"),
            _ => self.tt("updated ↓", "更新 ↓"),
        }
    }

    pub fn auto_refresh_label(&self) -> String {
        match self.prefs.auto_refresh_secs {
            0 => self.tt("off", "オフ"),
            30 => "30s".to_string(),
            60 => "1m".to_string(),
            300 => "5m".to_string(),
            n => format!("{n}s"),
        }
    }

    pub fn available_groups(&self) -> Vec<String> {
        let mut groups = Vec::new();
        for r in &self.repos {
            if let Some(g) = &r.group {
                let trimmed = g.trim();
                if !trimmed.is_empty()
                    && !groups.iter().any(|existing: &String| existing == trimmed)
                {
                    groups.push(trimmed.to_string());
                }
            }
        }
        groups.sort();
        groups
    }

    pub fn cycle_group_filter(&mut self, direction: isize) {
        let mut all_groups = vec!["All".to_string()];
        all_groups.extend(self.available_groups());
        if self
            .repos
            .iter()
            .any(|r| r.group.as_deref().unwrap_or("").trim().is_empty())
        {
            all_groups.push("Ungrouped".to_string());
        }
        let current_str = match &self.group_filter {
            None => "All",
            Some(g) if g.is_empty() => "Ungrouped",
            Some(g) => g.as_str(),
        };
        let cur_idx = all_groups
            .iter()
            .position(|g| g == current_str)
            .unwrap_or(0);
        let n = all_groups.len();
        let next_idx = if direction >= 0 {
            (cur_idx + 1) % n
        } else {
            (cur_idx + n - 1) % n
        };
        self.group_filter = match all_groups[next_idx].as_str() {
            "All" => None,
            "Ungrouped" => Some(String::new()),
            other => Some(other.to_string()),
        };
        self.home_selected = 0;
    }

    pub fn group_filter_label(&self) -> String {
        match &self.group_filter {
            None => self.tt("All", "すべて"),
            Some(g) if g.is_empty() => self.tt("Ungrouped", "未分類"),
            Some(g) => g.clone(),
        }
    }

    pub fn confirm_message(&self) -> Option<String> {
        match self.confirm {
            Some(Confirm::DeleteRepo(i)) => {
                let name = self.repos.get(i).map(|r| r.name.as_str()).unwrap_or("?");
                Some(format!("{} ({name}) [y/n]", self.t("confirm_delete_repo")))
            }
            Some(Confirm::DeleteMember(i)) => {
                let name = self
                    .members
                    .get(i)
                    .map(|m| m.canonical_name.as_str())
                    .unwrap_or("?");
                Some(format!(
                    "{} ({name}) [y/n]",
                    self.tt("Delete this member?", "このメンバーを削除しますか？")
                ))
            }
            Some(Confirm::DropStash { .. }) => Some(self.tt(
                "Drop this stash? [y/n]",
                "この stash を削除しますか？ [y/n]",
            )),
            None => None,
        }
    }

    pub fn filtered_home(&self) -> Vec<usize> {
        let mut idx =
            filter_repo_indices(&self.repos, &self.home_filter, self.group_filter.as_deref());
        sort_repo_indices(&mut idx, &self.repos, &self.home_rows, self.prefs.repo_sort);
        if self.attention_only {
            // A repo whose row hasn't loaded yet is left out rather than
            // guessed at — showing it would be a false positive or negative.
            idx.retain(|&i| self.home_rows.get(&i).is_some_and(needs_attention));
        }
        idx
    }

    pub fn current_list_len(&self) -> usize {
        self.visible_indices().len()
    }

    pub fn list_error(&self) -> Option<&str> {
        let data = self.repo_data.as_ref()?;
        match self.repo_tab {
            RepoTab::Status => data.working_err.as_deref(),
            RepoTab::Commits => data.commits_err.as_deref(),
            RepoTab::Branches => data.branches_err.as_deref(),
            RepoTab::Tags => data.tags_err.as_deref(),
            RepoTab::Stash => data.stashes_err.as_deref(),
            RepoTab::Contributors => data.contributors_err.as_deref(),
            RepoTab::Worktrees => data.worktrees_err.as_deref(),
        }
    }

    pub fn footer_hints(&self) -> Vec<(String, String)> {
        let pair = |k: &str, en: &str, ja: &str| (k.to_string(), self.tt(en, ja));
        if self.is_filtering() {
            return vec![
                pair("type", "filter", "絞込"),
                pair("enter", "apply", "確定"),
                pair("esc", "clear", "解除"),
            ];
        }
        match self.screen {
            Screen::Home => vec![
                pair("j/k", "move", "移動"),
                pair("enter", "open", "開く"),
                pair("[/]", "group", "グループ"),
                pair("/", "filter", "絞込"),
                pair("t", "shell", "シェル"),
                pair("P/F", "pull/fetch all", "一括P/F"),
                pair("M", "members", "横断メンバー"),
                pair("S", "search commits", "コミット検索"),
                pair("n", "needs attention", "要対応"),
                pair("o", "sort", "並替"),
                pair("s", "settings", "設定"),
                pair("?", "help", "ヘルプ"),
                pair("q", "quit", "終了"),
            ],
            Screen::Repo => {
                let mut h = vec![pair("1-7", "tabs", "タブ"), pair("j/k", "move", "移動")];
                h.extend(match self.repo_tab {
                    RepoTab::Status => vec![pair("enter", "diff", "diff")],
                    RepoTab::Commits => vec![
                        pair("space", "mark", "選択"),
                        pair("enter", "diff", "diff"),
                        pair("i", "builtin", "内蔵"),
                    ],
                    RepoTab::Branches => vec![pair("enter", "log", "ログ")],
                    RepoTab::Tags => vec![
                        pair("space", "mark", "選択"),
                        pair("enter", "compare", "比較"),
                    ],
                    RepoTab::Stash => vec![
                        pair("enter", "diff", "diff"),
                        pair("a/d", "apply/drop", "適用/削除"),
                    ],
                    RepoTab::Contributors => vec![
                        pair("space/t", "active", "在籍切替"),
                        pair("w", "period", "期間切替"),
                        pair("m", "filter active", "在籍のみ"),
                    ],
                    RepoTab::Worktrees => vec![pair("enter/t", "shell", "シェル起動")],
                });
                h.push(pair("t", "shell", "シェル"));
                h.push(pair("r", "reload", "再読込"));
                h.push(pair("?", "help", "ヘルプ"));
                h.push(pair("esc", "back", "戻る"));
                h.push(pair("q", "quit", "終了"));
                h
            }
            Screen::Diff => {
                let hunk = self
                    .diff
                    .as_ref()
                    .map(|d| {
                        if d.hunks.is_empty() {
                            "0/0".to_string()
                        } else {
                            format!("{}/{}", d.hunk_idx + 1, d.hunks.len())
                        }
                    })
                    .unwrap_or_else(|| "0/0".into());
                vec![
                    (format!("hunk {hunk}"), String::new()),
                    pair("n/p", "hunk", "hunk"),
                    pair("tab", "pane", "ペイン"),
                    pair("[/]", "file", "ファイル"),
                    pair("w", "ignore ws", "空白無視"),
                    pair("f", "full file", "全文表示"),
                    pair("b", "blame", "blame"),
                    pair("t", "shell", "シェル"),
                    pair("?", "help", "ヘルプ"),
                    pair("esc", "back", "戻る"),
                    pair("q", "quit", "終了"),
                ]
            }
            Screen::Settings => {
                let mut h = vec![
                    pair("tab/1/2", "tab", "タブ切替"),
                    pair("j/k", "move", "移動"),
                ];
                if self.settings_tab == SettingsTab::Repositories {
                    h.push(pair("a/A", "add/bulk", "追加/一括"));
                    h.push(pair("g", "group", "グループ"));
                    h.push(pair("d", "del", "削除"));
                } else {
                    h.push(pair("space/t", "active", "在籍切替"));
                    h.push(pair("a/e/d", "add/edit/del", "追加/編集/削除"));
                }
                h.push(pair("l", "lang", "言語"));
                h.push(pair("i", "auto-refresh", "自動更新"));
                h.push(pair("T", "theme", "テーマ"));
                h.push(pair("?", "help", "ヘルプ"));
                h.push(pair("esc", "back", "戻る"));
                h.push(pair("q", "quit", "終了"));
                h
            }
            Screen::GlobalMembers => vec![
                pair("j/k", "move", "移動"),
                pair("tab/h/l", "pane", "左右切替"),
                pair("enter", "open repo", "開く"),
                pair("space/t", "active", "在籍切替"),
                pair("m", "filter active", "在籍のみ"),
                pair("T", "shell", "シェル"),
                pair("?", "help", "ヘルプ"),
                pair("esc", "back", "戻る"),
                pair("q", "quit", "終了"),
            ],
            Screen::RepoFinder => {
                let n_selected = self
                    .repo_finder
                    .as_ref()
                    .map(|f| {
                        f.repos
                            .iter()
                            .filter(|r| r.is_selected && !r.is_already_added)
                            .count()
                    })
                    .unwrap_or(0);
                vec![
                    pair("j/k", "move", "移動"),
                    pair("space", "select", "選択"),
                    pair("a", "select all", "全選択"),
                    pair(
                        "enter",
                        &format!("import ({n_selected})"),
                        &format!("登録 ({n_selected})"),
                    ),
                    pair("/", "filter", "絞込"),
                    pair("r", "change path", "フォルダ変更"),
                    pair("?", "help", "ヘルプ"),
                    pair("esc", "cancel", "戻る"),
                ]
            }
            Screen::CommitSearch => vec![
                pair("j/k", "move", "移動"),
                pair("/", "new search", "再検索"),
                pair("enter", "open commit", "コミットを開く"),
                pair("?", "help", "ヘルプ"),
                pair("esc", "back", "戻る"),
                pair("q", "quit", "終了"),
            ],
            Screen::Help => vec![pair("esc/q/?", "back", "戻る")],
            Screen::Log => vec![
                pair("j/k", "scroll", "スクロール"),
                pair("?", "help", "ヘルプ"),
                pair("esc", "back", "戻る"),
                pair("q", "quit", "終了"),
            ],
        }
    }

    pub fn drain_messages(&mut self) {
        while let Ok(msg) = self.msg_rx.try_recv() {
            self.apply_msg(msg);
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) {
        if self.confirm.is_some() {
            self.handle_confirm(key);
            return;
        }
        if self.input.is_some() {
            self.handle_input(key);
            return;
        }
        if self.screen == Screen::Help {
            match key.code {
                KeyCode::Esc | KeyCode::Char('?') | KeyCode::Backspace | KeyCode::Char('q') => {
                    self.screen = self.help_return.take().unwrap_or(Screen::Home);
                    // Start from the top next time rather than wherever the
                    // last visit ended.
                    self.help_scroll.set(0);
                }
                KeyCode::Down | KeyCode::Char('j') => self.scroll_help_by(1),
                KeyCode::Up | KeyCode::Char('k') => self.scroll_help_by(-1),
                KeyCode::PageDown | KeyCode::Char(' ') => self.scroll_help_by(10),
                KeyCode::PageUp => self.scroll_help_by(-10),
                KeyCode::Char('g') => self.help_scroll.set(0),
                // The renderer clamps to the real content height, so asking
                // for more than exists is safe and lands on the last page.
                KeyCode::Char('G') => self.help_scroll.set(usize::MAX),
                _ => {}
            }
            return;
        }
        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL)
            || key.code == KeyCode::Char('q')
        {
            self.should_quit = true;
            return;
        }
        if key.code == KeyCode::Char('?') {
            self.help_return = Some(self.screen);
            self.screen = Screen::Help;
            return;
        }
        if key.code == KeyCode::Esc && self.error.is_some() {
            self.error = None;
            return;
        }
        match self.screen {
            Screen::Home => self.handle_home(key),
            Screen::Repo => self.handle_repo(key),
            Screen::Diff => self.handle_diff(key),
            Screen::Settings => self.handle_settings(key),
            Screen::GlobalMembers => self.handle_global_members(key),
            Screen::RepoFinder => self.handle_repo_finder(key),
            Screen::CommitSearch => self.handle_commit_search(key),
            Screen::Log => self.handle_log(key),
            Screen::Help => unreachable!(),
        }
    }

    pub fn visible_indices(&self) -> Vec<usize> {
        let Some(data) = self.repo_data.as_ref() else {
            return Vec::new();
        };
        let q = self.list_filter.trim().to_lowercase();
        let matches = |s: &str| q.is_empty() || s.to_lowercase().contains(&q);
        match self.repo_tab {
            RepoTab::Status => data
                .working_files
                .iter()
                .enumerate()
                .filter(|(_, f)| matches(&f.path))
                .map(|(i, _)| i)
                .collect(),
            RepoTab::Commits => data
                .commits
                .iter()
                .enumerate()
                .filter(|(_, c)| {
                    matches(&c.hash)
                        || matches(&c.author)
                        || matches(&c.message)
                        || matches(&c.date)
                })
                .map(|(i, _)| i)
                .collect(),
            RepoTab::Branches => data
                .branches
                .iter()
                .enumerate()
                .filter(|(_, b)| matches(&b.name) || matches(&b.message) || matches(&b.author))
                .map(|(i, _)| i)
                .collect(),
            RepoTab::Tags => data
                .tags
                .iter()
                .enumerate()
                .filter(|(_, t)| matches(&t.name) || matches(&t.message) || matches(&t.hash))
                .map(|(i, _)| i)
                .collect(),
            RepoTab::Stash => data
                .stashes
                .iter()
                .enumerate()
                .filter(|(_, s)| matches(&s.ref_name) || matches(&s.message) || matches(&s.author))
                .map(|(i, _)| i)
                .collect(),
            RepoTab::Contributors => data
                .contributors
                .iter()
                .enumerate()
                .filter(|(_, c)| {
                    if self.active_only && !(c.is_member && c.is_active) {
                        return false;
                    }
                    matches(&c.name) || matches(&c.email) || matches(&c.last_commit)
                })
                .map(|(i, _)| i)
                .collect(),
            RepoTab::Worktrees => data
                .worktrees
                .iter()
                .enumerate()
                .filter(|(_, w)| {
                    matches(&w.path)
                        || matches(&w.head)
                        || w.branch.as_ref().map(|b| matches(b)).unwrap_or(false)
                })
                .map(|(i, _)| i)
                .collect(),
        }
    }

    pub fn selected_item_index(&self) -> Option<usize> {
        self.visible_indices().get(self.list_selected).copied()
    }

    fn handle_confirm(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                let c = self.confirm.take();
                match c {
                    Some(Confirm::DeleteRepo(i)) => self.delete_repo(i),
                    Some(Confirm::DeleteMember(i)) => self.delete_member(i),
                    Some(Confirm::DropStash {
                        repo_idx,
                        stash_ref,
                    }) => self.drop_stash(repo_idx, stash_ref),
                    None => {}
                }
            }
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                self.confirm = None;
            }
            _ => {}
        }
    }

    fn handle_input(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                let kind = self.input.take();
                self.input_buf.clear();
                self.path_completions.clear();
                self.pending_member_name = None;
                self.editing_member_idx = None;
                self.editing_repo_group_idx = None;
                if matches!(kind, Some(InputKind::Filter)) {
                    if self.screen == Screen::Home {
                        self.home_filter.clear();
                        self.home_selected = 0;
                    } else if self.screen == Screen::GlobalMembers {
                        self.global_member_filter.clear();
                        self.global_member_selected = 0;
                        self.global_member_repo_selected = 0;
                    } else {
                        self.list_filter.clear();
                        self.list_selected = 0;
                    }
                } else if matches!(kind, Some(InputKind::FinderFilter))
                    && let Some(finder) = self.repo_finder.as_mut()
                {
                    finder.filter.clear();
                    finder.selected_idx = 0;
                }
            }
            KeyCode::Tab | KeyCode::BackTab => {
                if matches!(
                    self.input,
                    Some(InputKind::AddRepo | InputKind::FinderScanPath)
                ) {
                    if self.path_completions.is_empty() {
                        self.path_completions = complete_path(&self.input_buf);
                        self.path_completion_idx = 0;
                    } else if key.code == KeyCode::BackTab {
                        if self.path_completion_idx == 0 {
                            self.path_completion_idx =
                                self.path_completions.len().saturating_sub(1);
                        } else {
                            self.path_completion_idx -= 1;
                        }
                    } else {
                        self.path_completion_idx =
                            (self.path_completion_idx + 1) % self.path_completions.len();
                    }
                    if let Some(comp) = self.path_completions.get(self.path_completion_idx) {
                        self.input_buf = comp.clone();
                    }
                }
            }
            KeyCode::Enter => {
                let kind = self.input.take();
                self.path_completions.clear();
                let buf = std::mem::take(&mut self.input_buf);
                match kind {
                    Some(InputKind::Filter) => {
                        if self.screen == Screen::Home {
                            self.home_filter = buf;
                            self.home_selected = 0;
                        } else if self.screen == Screen::GlobalMembers {
                            self.global_member_filter = buf;
                            self.global_member_selected = 0;
                            self.global_member_repo_selected = 0;
                        } else {
                            self.list_filter = buf;
                            self.list_selected = 0;
                        }
                    }
                    Some(InputKind::FinderFilter) => {
                        if let Some(finder) = self.repo_finder.as_mut() {
                            finder.filter = buf;
                            finder.selected_idx = 0;
                        }
                    }
                    Some(InputKind::CommitSearchQuery) => {
                        self.run_commit_search(buf);
                    }
                    Some(InputKind::FinderScanPath) => {
                        let path = expand_user_path(&buf);
                        self.open_repo_finder(Some(path));
                    }
                    Some(InputKind::AddRepo) => self.add_repo_from_path(&buf),
                    Some(InputKind::AddAlias) => self.finish_add_repo(buf),
                    Some(InputKind::Rename) => self.rename_selected(buf),
                    Some(InputKind::EditRepoGroup) => self.finish_edit_repo_group(buf),
                    Some(InputKind::AddMemberName) => self.finish_add_member_name(buf),
                    Some(InputKind::AddMemberAliases) => self.finish_add_member_aliases(buf),
                    Some(InputKind::EditMemberAliases) => self.finish_edit_member_aliases(buf),
                    Some(InputKind::DiffCommand) => {
                        self.prefs.diff_command = buf.trim().to_string();
                        self.persist_prefs();
                        self.status = if self.prefs.diff_command.is_empty() {
                            self.tt("Using builtin diff.", "内蔵 diff を使います。")
                        } else {
                            format!(
                                "{} {}",
                                self.tt("Diff tool:", "Diff ツール:"),
                                self.prefs.diff_command
                            )
                        };
                    }
                    None => {}
                }
            }
            KeyCode::Backspace => {
                self.path_completions.clear();
                self.input_buf.pop();
                if matches!(self.input, Some(InputKind::Filter)) {
                    if self.screen == Screen::Home {
                        self.home_filter.clone_from(&self.input_buf);
                        self.home_selected = 0;
                    } else if self.screen == Screen::GlobalMembers {
                        self.global_member_filter.clone_from(&self.input_buf);
                        self.global_member_selected = 0;
                        self.global_member_repo_selected = 0;
                    } else {
                        self.list_filter.clone_from(&self.input_buf);
                        self.list_selected = 0;
                    }
                } else if matches!(self.input, Some(InputKind::FinderFilter))
                    && let Some(finder) = self.repo_finder.as_mut()
                {
                    finder.filter.clone_from(&self.input_buf);
                    finder.selected_idx = 0;
                }
            }
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.path_completions.clear();
                self.input_buf.push(c);
                if matches!(self.input, Some(InputKind::Filter)) {
                    if self.screen == Screen::Home {
                        self.home_filter.clone_from(&self.input_buf);
                        self.home_selected = 0;
                    } else if self.screen == Screen::GlobalMembers {
                        self.global_member_filter.clone_from(&self.input_buf);
                        self.global_member_selected = 0;
                        self.global_member_repo_selected = 0;
                    } else {
                        self.list_filter.clone_from(&self.input_buf);
                        self.list_selected = 0;
                    }
                } else if matches!(self.input, Some(InputKind::FinderFilter))
                    && let Some(finder) = self.repo_finder.as_mut()
                {
                    finder.filter.clone_from(&self.input_buf);
                    finder.selected_idx = 0;
                }
            }
            _ => {}
        }
    }

    fn handle_home(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc if !self.home_filter.is_empty() => {
                self.home_filter.clear();
                self.home_selected = 0;
            }
            KeyCode::Char('s') => {
                self.settings_selected = 0;
                self.settings_member_selected = 0;
                self.screen = Screen::Settings;
            }
            KeyCode::Char('M') => {
                self.open_global_members();
            }
            KeyCode::Char('S') => {
                self.input = Some(InputKind::CommitSearchQuery);
                self.input_buf = self
                    .commit_search
                    .as_ref()
                    .map(|s| s.query.clone())
                    .unwrap_or_default();
            }
            KeyCode::Char('[') => {
                self.cycle_group_filter(-1);
            }
            KeyCode::Char(']') => {
                self.cycle_group_filter(1);
            }
            KeyCode::Char('/') => {
                self.input = Some(InputKind::Filter);
                self.input_buf.clone_from(&self.home_filter);
            }
            KeyCode::Char('a') => self.begin_add_repo(),
            KeyCode::Char('A') => self.begin_bulk_add_repo(),
            KeyCode::Char('o') => self.cycle_sort(),
            KeyCode::Char('e') => self.begin_rename_home(),
            KeyCode::Char('d') => {
                if let Some(&idx) = self.filtered_home().get(self.home_selected) {
                    self.confirm = Some(Confirm::DeleteRepo(idx));
                }
            }
            KeyCode::Char('P') => self.bulk_pull(),
            KeyCode::Char('F') => self.bulk_fetch(),
            KeyCode::Char('p') => self.pull_selected_home(),
            KeyCode::Char('f') => self.fetch_selected_home(),
            KeyCode::Char('t') | KeyCode::Char('T') => self.open_terminal_for_current_repo(),
            KeyCode::Char('r') => self.refresh_home(),
            KeyCode::Char('n') => {
                self.attention_only = !self.attention_only;
                self.home_selected = 0;
                self.status = if self.attention_only {
                    self.tt(
                        "Showing only repos that need attention",
                        "要対応のリポジトリのみ表示",
                    )
                } else {
                    self.tt("Showing all repos", "すべてのリポジトリを表示")
                };
            }
            KeyCode::Char('g') => self.home_selected = 0,
            KeyCode::Char('G') => {
                let n = self.filtered_home().len();
                self.home_selected = n.saturating_sub(1);
            }
            KeyCode::Down | KeyCode::Char('j') => self.move_home(1),
            KeyCode::Up | KeyCode::Char('k') => self.move_home(-1),
            KeyCode::Enter | KeyCode::Char('l') | KeyCode::Right => self.open_selected_repo(),
            _ => {}
        }
    }

    fn handle_repo(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc | KeyCode::Char('h') | KeyCode::Backspace | KeyCode::Left => {
                self.screen = Screen::Home;
                self.repo_data = None;
                self.repo_index = None;
                self.list_filter.clear();
            }
            KeyCode::Tab | KeyCode::Char(']') => self.switch_tab(self.repo_tab.next()),
            KeyCode::BackTab | KeyCode::Char('[') => self.switch_tab(self.repo_tab.prev()),
            KeyCode::Char(d) if d.is_ascii_digit() => {
                if let Some(tab) = RepoTab::from_digit(d) {
                    self.switch_tab(tab);
                }
            }
            KeyCode::Char('/') => {
                self.input = Some(InputKind::Filter);
                self.input_buf.clone_from(&self.list_filter);
            }
            KeyCode::Char('r') => {
                if let Some(idx) = self.repo_index {
                    self.reload_repo(idx);
                }
            }
            KeyCode::Char('g') => {
                self.list_selected = 0;
                self.after_list_move();
            }
            KeyCode::Char('G') => {
                self.list_selected = self.current_list_len().saturating_sub(1);
                self.after_list_move();
            }
            KeyCode::Down | KeyCode::Char('j') => self.move_list(1),
            KeyCode::Up | KeyCode::Char('k') => self.move_list(-1),
            KeyCode::Char('p') => self.pull_current_repo(),
            KeyCode::Char('f') => self.fetch_current_repo(),
            KeyCode::Char('t') if self.repo_tab != RepoTab::Contributors => {
                self.open_terminal_for_current_repo();
            }
            KeyCode::Char('T') => {
                self.open_terminal_for_current_repo();
            }
            KeyCode::Char('m') if self.repo_tab == RepoTab::Contributors => {
                self.active_only = !self.active_only;
                self.list_selected = 0;
            }
            KeyCode::Char('w') | KeyCode::Char('W') if self.repo_tab == RepoTab::Contributors => {
                self.cycle_contributor_time_span();
            }
            KeyCode::Char(' ') | KeyCode::Char('t') if self.repo_tab == RepoTab::Contributors => {
                self.toggle_contributor_active();
            }
            KeyCode::Char('i') if self.repo_tab == RepoTab::Commits => {
                self.open_selected_commit(true);
            }
            KeyCode::Char(' ') if self.repo_tab == RepoTab::Tags => self.toggle_tag_marker(),
            KeyCode::Char(' ') if self.repo_tab == RepoTab::Commits => self.toggle_commit_marker(),
            KeyCode::Char('a') if self.repo_tab == RepoTab::Stash => self.apply_selected_stash(),
            KeyCode::Char('d') if self.repo_tab == RepoTab::Stash => {
                if let (Some(idx), Some(item)) = (self.repo_index, self.selected_item_index())
                    && let Some(data) = self.repo_data.as_ref()
                    && let Some(st) = data.stashes.get(item)
                {
                    self.confirm = Some(Confirm::DropStash {
                        repo_idx: idx,
                        stash_ref: st.ref_name.clone(),
                    });
                }
            }
            KeyCode::Enter | KeyCode::Char('l') | KeyCode::Right => self.activate_repo_item(),
            _ => {}
        }
    }

    fn handle_diff(&mut self, key: KeyEvent) {
        let has_hunks = self.diff.as_ref().is_some_and(|d| !d.hunks.is_empty());
        match key.code {
            KeyCode::Esc | KeyCode::Backspace => {
                self.diff = None;
                self.screen = Screen::Repo;
                self.focus = FocusPane::List;
            }
            KeyCode::Char('t') | KeyCode::Char('T') => {
                self.open_terminal_for_current_repo();
            }
            KeyCode::Tab | KeyCode::Char('l') | KeyCode::Right => {
                self.focus = self.focus.next(has_hunks);
            }
            KeyCode::BackTab | KeyCode::Char('h') | KeyCode::Left => {
                self.focus = self.focus.prev(has_hunks);
            }
            KeyCode::Char('[') => self.diff_change_file(-1),
            KeyCode::Char(']') => self.diff_change_file(1),
            KeyCode::Down | KeyCode::Char('j') => self.diff_move(1),
            KeyCode::Up | KeyCode::Char('k') => self.diff_move(-1),
            KeyCode::PageDown => self.diff_move(20),
            KeyCode::Char(' ') if self.focus == FocusPane::Content => self.diff_move(20),
            KeyCode::PageUp => self.diff_move(-20),
            KeyCode::Char('g') => self.diff_home_end(true),
            KeyCode::Char('G') => self.diff_home_end(false),
            KeyCode::Char('n') => self.next_hunk(1),
            KeyCode::Char('N') | KeyCode::Char('p') => self.next_hunk(-1),
            KeyCode::Char('w') => self.toggle_diff_ignore_whitespace(),
            KeyCode::Char('f') => self.toggle_diff_full_file(),
            KeyCode::Char('b') => self.toggle_diff_blame(),
            KeyCode::Enter if self.focus == FocusPane::List => self.load_selected_diff_file(),
            KeyCode::Enter if self.focus == FocusPane::Hunks => self.jump_current_hunk(),
            _ => {}
        }
    }

    fn handle_commit_search(&mut self, key: KeyEvent) {
        let len = self.commit_search.as_ref().map_or(0, |s| s.hits.len());
        match key.code {
            KeyCode::Esc | KeyCode::Char('h') | KeyCode::Backspace | KeyCode::Left => {
                self.screen = Screen::Home;
            }
            KeyCode::Char('/') => {
                self.input = Some(InputKind::CommitSearchQuery);
                self.input_buf = self
                    .commit_search
                    .as_ref()
                    .map(|s| s.query.clone())
                    .unwrap_or_default();
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if let Some(s) = self.commit_search.as_mut() {
                    s.selected = move_index(s.selected, len, 1);
                }
            }
            KeyCode::Up | KeyCode::Char('k') => {
                if let Some(s) = self.commit_search.as_mut() {
                    s.selected = move_index(s.selected, len, -1);
                }
            }
            KeyCode::Char('g') => {
                if let Some(s) = self.commit_search.as_mut() {
                    s.selected = 0;
                }
            }
            KeyCode::Char('G') => {
                if let Some(s) = self.commit_search.as_mut() {
                    s.selected = len.saturating_sub(1);
                }
            }
            KeyCode::Enter => self.jump_to_search_hit(),
            _ => {}
        }
    }

    /// Open the repository the selected search hit belongs to, straight to
    /// that commit's diff.
    fn jump_to_search_hit(&mut self) {
        let Some(hit) = self
            .commit_search
            .as_ref()
            .and_then(|s| s.hits.get(s.selected))
            .cloned()
        else {
            return;
        };
        // hit.repo_index was snapshotted when the search was dispatched; if
        // the repo list changed since (a repo removed, reordered, or
        // replaced) while the search was still running, that index could
        // now name a different repository. Confirm the name still matches
        // before jumping rather than silently opening the wrong one.
        match self.repos.get(hit.repo_index) {
            Some(r) if r.name == hit.repo_name => {}
            _ => {
                self.error = Some(self.tt(
                    "That repository is no longer at the same position — search again",
                    "そのリポジトリは一覧内の位置が変わりました。再検索してください",
                ));
                return;
            }
        }
        if !self.open_repo_state(hit.repo_index) {
            return;
        }
        // The commit diff is what the user pressed Enter to see, so it goes
        // to the worker first; the full repository load (branches, tags,
        // stashes, contributors, worktrees — needed only if they back out
        // to the Repo screen) follows behind it rather than blocking it.
        self.open_commit_diff(hit.repo_index, hit.hash, hit.message);
        if let Some(repo) = self.repos.get(hit.repo_index) {
            self.send_job(Job::LoadRepo {
                index: hit.repo_index,
                path: repo.path.clone(),
                members: self.members.clone(),
                time_span: self.contributor_time_span,
            });
        }
    }

    fn handle_log(&mut self, key: KeyEvent) {
        let Some(log) = self.log.as_mut() else {
            self.screen = Screen::Repo;
            return;
        };
        let max = log.body.lines().count().saturating_sub(1);
        match key.code {
            KeyCode::Esc | KeyCode::Char('h') | KeyCode::Backspace | KeyCode::Left => {
                self.log = None;
                self.screen = Screen::Repo;
            }
            KeyCode::Down | KeyCode::Char('j') => {
                log.scroll = (log.scroll + 1).min(max);
            }
            KeyCode::Up | KeyCode::Char('k') => {
                log.scroll = log.scroll.saturating_sub(1);
            }
            KeyCode::PageDown | KeyCode::Char(' ') => {
                log.scroll = (log.scroll + 20).min(max);
            }
            KeyCode::PageUp => {
                log.scroll = log.scroll.saturating_sub(20);
            }
            KeyCode::Char('g') => log.scroll = 0,
            KeyCode::Char('G') => log.scroll = max,
            _ => {}
        }
    }

    fn handle_settings(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc | KeyCode::Char('h') | KeyCode::Backspace => {
                self.screen = Screen::Home;
            }
            KeyCode::Tab | KeyCode::BackTab => {
                self.settings_tab = match self.settings_tab {
                    SettingsTab::Repositories => SettingsTab::Members,
                    SettingsTab::Members => SettingsTab::Repositories,
                };
            }
            KeyCode::Char('1') => {
                self.settings_tab = SettingsTab::Repositories;
            }
            KeyCode::Char('2') => {
                self.settings_tab = SettingsTab::Members;
            }
            KeyCode::Down | KeyCode::Char('j') => match self.settings_tab {
                SettingsTab::Repositories => {
                    let n = self.repos.len();
                    if n > 0 {
                        self.settings_selected = (self.settings_selected + 1).min(n - 1);
                    }
                }
                SettingsTab::Members => {
                    let n = self.members.len();
                    if n > 0 {
                        self.settings_member_selected =
                            (self.settings_member_selected + 1).min(n - 1);
                    }
                }
            },
            KeyCode::Up | KeyCode::Char('k') => match self.settings_tab {
                SettingsTab::Repositories => {
                    self.settings_selected = self.settings_selected.saturating_sub(1);
                }
                SettingsTab::Members => {
                    self.settings_member_selected = self.settings_member_selected.saturating_sub(1);
                }
            },
            KeyCode::Char('a') => match self.settings_tab {
                SettingsTab::Repositories => self.begin_add_repo(),
                SettingsTab::Members => self.begin_add_member(),
            },
            KeyCode::Char('A') if self.settings_tab == SettingsTab::Repositories => {
                self.begin_bulk_add_repo()
            }
            KeyCode::Char('g') if self.settings_tab == SettingsTab::Repositories => {
                if self.settings_selected < self.repos.len() {
                    self.begin_edit_repo_group(self.settings_selected);
                }
            }
            KeyCode::Char(' ') | KeyCode::Char('t')
                if self.settings_tab == SettingsTab::Members =>
            {
                self.toggle_member_active(self.settings_member_selected);
            }
            KeyCode::Char('d') => match self.settings_tab {
                SettingsTab::Repositories => {
                    if self.settings_selected < self.repos.len() {
                        self.confirm = Some(Confirm::DeleteRepo(self.settings_selected));
                    }
                }
                SettingsTab::Members => {
                    if self.settings_member_selected < self.members.len() {
                        self.confirm = Some(Confirm::DeleteMember(self.settings_member_selected));
                    }
                }
            },
            KeyCode::Char('e') => match self.settings_tab {
                SettingsTab::Repositories => {
                    if self.settings_selected < self.repos.len() {
                        self.begin_rename(self.settings_selected);
                    }
                }
                SettingsTab::Members => {
                    if self.settings_member_selected < self.members.len() {
                        self.begin_edit_member_aliases(self.settings_member_selected);
                    }
                }
            },
            KeyCode::Char('c') => {
                self.input = Some(InputKind::DiffCommand);
                self.input_buf.clone_from(&self.prefs.diff_command);
            }
            KeyCode::Char('l') => self.toggle_language(),
            KeyCode::Char('i') => self.cycle_auto_refresh(),
            KeyCode::Char('T') => self.toggle_theme(),
            _ => {}
        }
    }

    fn handle_global_members(&mut self, key: KeyEvent) {
        let vis = self.filtered_global_members();
        let cur_member = vis
            .get(self.global_member_selected)
            .and_then(|&i| self.global_members.get(i));
        let num_repos = cur_member.map(|m| m.contributions.len()).unwrap_or(0);

        match key.code {
            KeyCode::Esc | KeyCode::Char('h') | KeyCode::Backspace
                if self.global_member_pane == FocusPane::List =>
            {
                self.screen = Screen::Home;
            }
            KeyCode::Tab | KeyCode::BackTab | KeyCode::Char('l') | KeyCode::Right => {
                if self.global_member_pane == FocusPane::List && num_repos > 0 {
                    self.global_member_pane = FocusPane::Content;
                } else {
                    self.global_member_pane = FocusPane::List;
                }
            }
            KeyCode::Char('h') | KeyCode::Left => {
                self.global_member_pane = FocusPane::List;
            }
            KeyCode::Down | KeyCode::Char('j') => match self.global_member_pane {
                FocusPane::List => {
                    let n = vis.len();
                    if n > 0 {
                        self.global_member_selected = (self.global_member_selected + 1).min(n - 1);
                        self.global_member_repo_selected = 0;
                    }
                }
                _ => {
                    if num_repos > 0 {
                        self.global_member_repo_selected =
                            (self.global_member_repo_selected + 1).min(num_repos - 1);
                    }
                }
            },
            KeyCode::Up | KeyCode::Char('k') => match self.global_member_pane {
                FocusPane::List => {
                    self.global_member_selected = self.global_member_selected.saturating_sub(1);
                    self.global_member_repo_selected = 0;
                }
                _ => {
                    self.global_member_repo_selected =
                        self.global_member_repo_selected.saturating_sub(1);
                }
            },
            KeyCode::Char('/') => {
                self.input = Some(InputKind::Filter);
                self.input_buf.clone_from(&self.global_member_filter);
            }
            KeyCode::Char('m') => {
                self.active_only = !self.active_only;
                self.global_member_selected = 0;
                self.global_member_repo_selected = 0;
            }
            KeyCode::Char('T') => {
                self.open_terminal_for_current_repo();
            }
            KeyCode::Char(' ') | KeyCode::Char('t') => {
                if let Some(&mem_idx) = vis.get(self.global_member_selected)
                    && let Some(m) = self.global_members.get(mem_idx)
                {
                    let name = m.canonical_name.clone();
                    if let Some(idx) = self
                        .members
                        .iter()
                        .position(|em| em.canonical_name.eq_ignore_ascii_case(&name))
                    {
                        self.toggle_member_active(idx);
                    } else {
                        self.members.push(Member {
                            canonical_name: name,
                            aliases: m.aliases.clone(),
                            is_active: true,
                        });
                        self.persist_members();
                    }
                    self.request_global_members();
                }
            }
            KeyCode::Enter => {
                if let Some(m) = cur_member
                    && let Some(contrib) = m.contributions.get(self.global_member_repo_selected)
                {
                    let repo_idx = contrib.repo_index;
                    self.open_repo(repo_idx);
                }
            }
            _ => {}
        }
    }

    fn move_home(&mut self, delta: isize) {
        let n = self.filtered_home().len();
        self.home_selected = move_index(self.home_selected, n, delta);
    }

    fn move_list(&mut self, delta: isize) {
        let n = self.current_list_len();
        self.list_selected = move_index(self.list_selected, n, delta);
        self.after_list_move();
    }

    fn after_list_move(&mut self) {
        if self.repo_tab == RepoTab::Commits {
            self.request_commit_preview();
        }
    }

    fn switch_tab(&mut self, tab: RepoTab) {
        self.repo_tab = tab;
        self.list_selected = 0;
        self.list_filter.clear();
        self.after_list_move();
    }

    fn diff_move(&mut self, delta: isize) {
        let mut load = false;
        {
            let Some(diff) = self.diff.as_mut() else {
                return;
            };
            match self.focus {
                FocusPane::List => {
                    let old = diff.file_idx;
                    diff.file_idx = move_index(diff.file_idx, diff.files.len(), delta);
                    load = diff.file_idx != old;
                }
                FocusPane::Hunks => {
                    if diff.hunks.is_empty() {
                        return;
                    }
                    diff.hunk_idx = move_index(diff.hunk_idx, diff.hunks.len(), delta);
                    diff.scroll = diff.hunks[diff.hunk_idx].start;
                }
                FocusPane::Content => {
                    let max = diff.lines.len().saturating_sub(1);
                    diff.scroll = (diff.scroll as isize + delta).clamp(0, max as isize) as usize;
                    sync_hunk_from_scroll(diff);
                }
            }
        }
        if load {
            self.load_selected_diff_file();
        }
    }

    fn diff_change_file(&mut self, delta: isize) {
        let changed = {
            let Some(diff) = self.diff.as_mut() else {
                return;
            };
            let old = diff.file_idx;
            diff.file_idx = move_index(diff.file_idx, diff.files.len(), delta);
            diff.file_idx != old
        };
        if changed {
            self.load_selected_diff_file();
        }
    }

    fn diff_home_end(&mut self, home: bool) {
        let mut load = false;
        {
            let Some(diff) = self.diff.as_mut() else {
                return;
            };
            match self.focus {
                FocusPane::List => {
                    diff.file_idx = if home {
                        0
                    } else {
                        diff.files.len().saturating_sub(1)
                    };
                    load = true;
                }
                FocusPane::Hunks | FocusPane::Content => {
                    if diff.hunks.is_empty() {
                        diff.scroll = if home {
                            0
                        } else {
                            diff.lines.len().saturating_sub(1)
                        };
                    } else {
                        diff.hunk_idx = if home { 0 } else { diff.hunks.len() - 1 };
                        diff.scroll = diff.hunks[diff.hunk_idx].start;
                    }
                }
            }
        }
        if load {
            self.load_selected_diff_file();
        }
    }

    fn jump_current_hunk(&mut self) {
        let Some(diff) = self.diff.as_mut() else {
            return;
        };
        if let Some(h) = diff.hunks.get(diff.hunk_idx) {
            diff.scroll = h.start;
        }
    }

    fn next_hunk(&mut self, dir: isize) {
        let Some(diff) = self.diff.as_mut() else {
            return;
        };
        let n = diff.hunks.len();
        if n == 0 {
            return;
        }
        let next = (diff.hunk_idx as isize + dir).rem_euclid(n as isize) as usize;
        diff.hunk_idx = next;
        diff.scroll = diff.hunks[next].start;
    }

    /// Queue a job on the worker that owns its kind, surfacing the (terminal)
    /// case where that worker has died instead of dropping the job silently.
    fn send_job(&mut self, job: Job) {
        match &job {
            Job::LoadHome { index, .. } => {
                self.busy.insert(*index, Activity::Refresh);
            }
            Job::Pull { index, .. } => {
                self.busy.insert(*index, Activity::Pull);
            }
            Job::Fetch { index, .. } => {
                self.busy.insert(*index, Activity::Fetch);
            }
            // Everything else is tracked by the screen that owns it
            // (`repo_loading`, `diff.loading`, ...), already maintained.
            _ => {}
        }
        let tx = if job.is_secondary_worker() {
            &self.bulk_tx
        } else {
            &self.job_tx
        };
        if tx.send(job).is_err() {
            self.error = Some(self.tt(
                "Background worker stopped; restart the app.",
                "バックグラウンド処理が停止しました。アプリを再起動してください。",
            ));
        }
    }

    /// Persist repositories, refusing to write when config.json failed to load
    /// — saving then would replace the user's list with the empty default we
    /// fell back to. Callers that must roll back their in-memory change on
    /// failure use the `try_` form; the rest report through `self.error`.
    fn try_persist_repos(&self) -> Result<(), String> {
        if self.config_state.repos_failed {
            return Err(self.config_readonly_message("config.json"));
        }
        config::save_repositories(&self.repos)
    }

    fn try_persist_members(&self) -> Result<(), String> {
        if self.config_state.members_failed {
            return Err(self.config_readonly_message("members.json"));
        }
        config::save_members(&self.members)
    }

    fn persist_repos(&mut self) {
        if let Err(e) = self.try_persist_repos() {
            self.error = Some(e);
        }
    }

    fn persist_members(&mut self) {
        if let Err(e) = self.try_persist_members() {
            self.error = Some(e);
        }
    }

    fn persist_prefs(&mut self) {
        if self.config_state.prefs_failed {
            self.error = Some(self.config_readonly_message("prefs.json"));
            return;
        }
        if let Err(e) = config::save_preferences(&self.prefs) {
            self.error = Some(e);
        }
    }

    fn config_readonly_message(&self, file: &str) -> String {
        format!(
            "{file}: {}",
            self.tt(
                "could not be loaded, so it is not being overwritten",
                "を読み込めなかったため上書きしません"
            )
        )
    }

    fn home_generation(&self) -> u64 {
        self.home_gen.load(Ordering::Relaxed)
    }

    /// Re-analyse one repository's home row without bumping the generation,
    /// so the refreshes already in flight for the other rows stay valid.
    fn refresh_home_row(&mut self, index: usize) {
        let Some(repo) = self.repos.get(index) else {
            return;
        };
        let job = Job::LoadHome {
            generation: self.home_generation(),
            index,
            path: repo.path.clone(),
            members: self.members.clone(),
        };
        self.send_job(job);
    }

    fn refresh_home(&mut self) {
        self.status = self.t("analyzing");
        self.last_auto_refresh = std::time::Instant::now();
        let generation = self.home_gen.fetch_add(1, Ordering::Relaxed) + 1;
        let jobs: Vec<Job> = self
            .repos
            .iter()
            .enumerate()
            .map(|(index, repo)| Job::LoadHome {
                generation,
                index,
                path: repo.path.clone(),
                members: self.members.clone(),
            })
            .collect();
        for job in jobs {
            self.send_job(job);
        }
        if self.repos.is_empty() {
            self.status = self.tt(
                "No repositories. Press a to add one.",
                "リポジトリがありません。a で追加。",
            );
        }
    }

    fn open_selected_repo(&mut self) {
        let Some(&idx) = self.filtered_home().get(self.home_selected) else {
            return;
        };
        self.open_repo(idx);
    }

    /// State half of [`Self::open_repo`], without sending `Job::LoadRepo`.
    /// Split out for `jump_to_search_hit`, which needs this same setup but
    /// wants the commit-diff job (what the user actually pressed Enter to
    /// see) queued on the worker *before* the full repository load.
    fn open_repo_state(&mut self, idx: usize) -> bool {
        let Some(repo) = self.repos.get(idx) else {
            return false;
        };
        self.repo_index = Some(idx);
        self.repo_data = None;
        self.repo_loading = true;
        self.repo_tab = RepoTab::Commits;
        self.list_selected = 0;
        self.list_filter.clear();
        self.tag_base = None;
        self.tag_target = None;
        self.commit_base = None;
        self.commit_target = None;
        self.commit_preview = None;
        self.screen = Screen::Repo;
        if let Some(cached) = load_tui_cache(&repo.path) {
            self.repo_data = Some(cached);
            self.repo_loading = true;
        }
        self.status = self.t("analyzing_repo_data");
        true
    }

    fn open_repo(&mut self, idx: usize) {
        if !self.open_repo_state(idx) {
            return;
        }
        let Some(repo) = self.repos.get(idx) else {
            return;
        };
        self.send_job(Job::LoadRepo {
            index: idx,
            path: repo.path.clone(),
            members: self.members.clone(),
            time_span: self.contributor_time_span,
        });
    }

    fn reload_repo(&mut self, idx: usize) {
        let Some(repo) = self.repos.get(idx) else {
            return;
        };
        self.repo_loading = true;
        self.status = self.t("analyzing_repo_data");
        self.send_job(Job::LoadRepo {
            index: idx,
            path: repo.path.clone(),
            members: self.members.clone(),
            time_span: self.contributor_time_span,
        });
    }

    fn activate_repo_item(&mut self) {
        let Some(idx) = self.repo_index else {
            return;
        };
        let item = self.selected_item_index();
        let data = self.repo_data.as_ref();
        match self.repo_tab {
            RepoTab::Status => {
                let path = item.and_then(|i| data?.working_files.get(i).map(|f| f.path.clone()));
                if let Some(path) = path
                    && !self.launch_external(None, "WORKING_TREE")
                {
                    self.open_diff(
                        idx,
                        None,
                        "WORKING_TREE".to_string(),
                        false,
                        Some(path),
                        self.tt("Working tree", "作業ツリー"),
                    );
                }
            }
            RepoTab::Commits => {
                if let (Some(base), Some(target)) =
                    (self.commit_base.clone(), self.commit_target.clone())
                    && !self.launch_external(Some(&base), &target)
                {
                    self.open_diff(
                        idx,
                        Some(base.clone()),
                        target.clone(),
                        true,
                        None,
                        format!("{base}...{target}"),
                    );
                } else if self.commit_base.is_none() || self.commit_target.is_none() {
                    self.open_selected_commit(false);
                }
            }
            RepoTab::Tags => {
                if let (Some(base), Some(target)) = (self.tag_base.clone(), self.tag_target.clone())
                    && !self.launch_external(Some(&base), &target)
                {
                    self.open_diff(
                        idx,
                        Some(base.clone()),
                        target.clone(),
                        true,
                        None,
                        format!("{base}...{target}"),
                    );
                }
            }
            RepoTab::Branches => {
                let name = item.and_then(|i| data?.branches.get(i).map(|b| b.name.clone()));
                let path = self.repos.get(idx).map(|r| r.path.clone());
                if let (Some(name), Some(path)) = (name, path) {
                    self.status = self.tt("Loading log...", "ログを読み込み中...");
                    self.send_job(Job::LoadBranchLog { path, branch: name });
                }
            }
            RepoTab::Stash => {
                let stash = item.and_then(|i| {
                    data?
                        .stashes
                        .get(i)
                        .map(|st| (st.ref_name.clone(), st.message.clone()))
                });
                if let Some((stash_ref, message)) = stash {
                    self.open_commit_diff(idx, stash_ref, message);
                }
            }
            RepoTab::Contributors => {}
            RepoTab::Worktrees => {
                let wt_path = item.and_then(|i| data?.worktrees.get(i).map(|w| w.path.clone()));
                if let Some(path) = wt_path {
                    self.pending_terminal = Some(PathBuf::from(path));
                }
            }
        }
    }

    fn open_selected_commit(&mut self, force_builtin: bool) {
        let Some(idx) = self.repo_index else {
            return;
        };
        let item = self.selected_item_index();
        let commit = item.and_then(|i| {
            self.repo_data
                .as_ref()?
                .commits
                .get(i)
                .map(|c| (c.hash.clone(), c.message.clone()))
        });
        let Some((hash, message)) = commit else {
            return;
        };
        if !force_builtin && self.launch_external(None, &hash) {
            return;
        }
        self.open_commit_diff(idx, hash, message);
    }

    fn open_commit_diff(&mut self, repo_idx: usize, hash: String, message: String) {
        let Some(repo) = self.repos.get(repo_idx) else {
            return;
        };
        self.diff_seq += 1;
        let seq = self.diff_seq;
        self.diff = Some(DiffView {
            title: format!("{hash}  {message}"),
            target: hash.clone(),
            base: None,
            three_dot: false,
            files: Vec::new(),
            file_idx: 0,
            lines: Vec::new(),
            hunks: Vec::new(),
            hunk_idx: 0,
            scroll: 0,
            loading: true,
            header: None,
            error: None,
            blame: None,
            blame_loading: false,
            pending_scroll_restore: None,
        });
        self.screen = Screen::Diff;
        self.focus = FocusPane::List;
        self.send_job(Job::LoadCommitMeta {
            seq,
            path: repo.path.clone(),
            hash,
        });
    }

    fn open_diff(
        &mut self,
        repo_idx: usize,
        base: Option<String>,
        target: String,
        three_dot: bool,
        preselect_file: Option<String>,
        title: String,
    ) {
        let Some(repo) = self.repos.get(repo_idx) else {
            return;
        };
        self.diff_seq += 1;
        let seq = self.diff_seq;
        self.diff = Some(DiffView {
            title,
            target: target.clone(),
            base: base.clone(),
            three_dot,
            files: Vec::new(),
            file_idx: 0,
            lines: Vec::new(),
            hunks: Vec::new(),
            hunk_idx: 0,
            scroll: 0,
            loading: true,
            header: None,
            error: None,
            blame: None,
            blame_loading: false,
            pending_scroll_restore: None,
        });
        self.screen = Screen::Diff;
        self.focus = FocusPane::List;
        self.send_job(Job::LoadFiles {
            seq,
            path: repo.path.clone(),
            base,
            target,
            three_dot,
            preselect: preselect_file,
        });
    }

    fn load_selected_diff_file(&mut self) {
        let Some(repo_idx) = self.repo_index else {
            return;
        };
        let Some(repo) = self.repos.get(repo_idx) else {
            return;
        };
        let Some(diff) = self.diff.as_mut() else {
            return;
        };
        let Some(file) = diff.files.get(diff.file_idx) else {
            diff.loading = false;
            diff.lines.clear();
            diff.hunks.clear();
            diff.hunk_idx = 0;
            return;
        };
        self.diff_seq += 1;
        let seq = self.diff_seq;
        diff.loading = true;
        diff.scroll = 0;
        diff.blame = None;
        // Cleared unconditionally: reload_diff_preserving_scroll sets this
        // *after* calling this function, so a stale value from an earlier
        // toggle never leaks into an unrelated file's load.
        diff.pending_scroll_restore = None;
        let job = Job::LoadDiff {
            seq,
            path: repo.path.clone(),
            base: diff.base.clone(),
            target: diff.target.clone(),
            file: file.path.clone(),
            three_dot: diff.three_dot,
            ignore_whitespace: self.prefs.diff_ignore_whitespace,
            full: self.prefs.diff_full_file,
        };
        self.send_job(job);
        // Same seq as the diff job: a stale reply is dropped by the seq check
        // in both Msg handlers, so the two loads racing is not observable.
        if self.prefs.diff_show_blame {
            self.request_blame(seq);
        }
    }

    /// Fetch per-line blame for the file currently shown in the diff view.
    /// Opt-in (`b` in the diff screen) rather than always fetched alongside
    /// the diff, since it is a second git invocation per file. Returns
    /// whether a request was actually launched, so a caller that just
    /// turned the preference on can tell the difference from a silent no-op
    /// (no repo open, or no file selected yet) instead of leaving the title
    /// claiming "blame ON" with no gutter and no explanation.
    fn request_blame(&mut self, seq: u64) -> bool {
        let Some(repo_idx) = self.repo_index else {
            return false;
        };
        let Some(repo) = self.repos.get(repo_idx) else {
            return false;
        };
        let Some(diff) = self.diff.as_mut() else {
            return false;
        };
        let Some(file) = diff.files.get(diff.file_idx) else {
            return false;
        };
        diff.blame_loading = true;
        let blame_ref = if diff.target == git::WORKING_TREE {
            git::BLAME_WORKING_TREE.to_string()
        } else {
            diff.target.clone()
        };
        let job = Job::LoadBlame {
            seq,
            path: repo.path.clone(),
            blame_ref,
            file: file.path.clone(),
        };
        self.send_job(job);
        true
    }

    /// Reload the current file's diff without losing the user's place in
    /// it — used for toggles that only change view density (whitespace,
    /// full-file context), as opposed to switching to a different file,
    /// where jumping to the first hunk is the expected behavior.
    fn reload_diff_preserving_scroll(&mut self) {
        let saved_scroll = self.diff.as_ref().map(|d| d.scroll);
        self.load_selected_diff_file();
        // Set *after*: load_selected_diff_file unconditionally clears this
        // field so a stale restore from an earlier toggle can't leak into an
        // unrelated file's load.
        if let (Some(saved), Some(diff)) = (saved_scroll, self.diff.as_mut()) {
            diff.pending_scroll_restore = Some(saved);
        }
    }

    fn toggle_diff_ignore_whitespace(&mut self) {
        self.prefs.diff_ignore_whitespace = !self.prefs.diff_ignore_whitespace;
        self.persist_prefs();
        self.reload_diff_preserving_scroll();
    }

    fn toggle_diff_full_file(&mut self) {
        self.prefs.diff_full_file = !self.prefs.diff_full_file;
        self.persist_prefs();
        self.reload_diff_preserving_scroll();
    }

    fn toggle_diff_blame(&mut self) {
        self.prefs.diff_show_blame = !self.prefs.diff_show_blame;
        self.persist_prefs();
        if self.prefs.diff_show_blame {
            if !self.request_blame(self.diff_seq) {
                // No repo/file open to blame yet — leaving the preference on
                // would show "blame ON" in the title with no gutter and no
                // explanation, indistinguishable from a silent failure.
                self.prefs.diff_show_blame = false;
                self.persist_prefs();
                self.status = self.tt(
                    "No file selected to blame yet",
                    "blame対象のファイルが選択されていません",
                );
            }
        } else if let Some(diff) = self.diff.as_mut() {
            diff.blame = None;
            diff.blame_loading = false;
        }
    }

    /// Cycle one name through base -> target -> unmarked.
    ///
    /// Shared by tags and commits so the two can't drift: the rule that an
    /// already-marked entry clears rather than re-marking is what makes a
    /// second press (or click) undo a mistake instead of doing nothing.
    /// Move the help viewport. Saturating rather than wrapping: the renderer
    /// clamps the upper end against the real content height, which is the only
    /// place that knows it.
    pub(crate) fn scroll_help_by(&self, delta: isize) {
        let cur = self.help_scroll.get();
        let next = if delta < 0 {
            cur.saturating_sub(delta.unsigned_abs())
        } else {
            cur.saturating_add(delta as usize)
        };
        self.help_scroll.set(next);
    }

    fn toggle_marker(base: &mut Option<String>, target: &mut Option<String>, name: String) {
        if base.as_deref() == Some(name.as_str()) {
            *base = None;
        } else if target.as_deref() == Some(name.as_str()) {
            *target = None;
        } else if base.is_none() {
            *base = Some(name);
        } else {
            *target = Some(name);
        }
    }

    /// Mark the item at `index` (an index into the visible/filtered list) as
    /// the comparison base or target, if the current tab has such a thing.
    /// Returns whether anything was marked, so a click can fall through to
    /// plain selection on tabs that don't compare.
    pub fn toggle_marker_at(&mut self, index: usize) -> bool {
        let Some(data) = self.repo_data.as_ref() else {
            return false;
        };
        let Some(&item) = self.visible_indices().get(index) else {
            return false;
        };
        match self.repo_tab {
            RepoTab::Commits => {
                let Some(hash) = data.commits.get(item).map(|c| c.hash.clone()) else {
                    return false;
                };
                Self::toggle_marker(&mut self.commit_base, &mut self.commit_target, hash);
                true
            }
            RepoTab::Tags => {
                let Some(name) = data.tags.get(item).map(|t| t.name.clone()) else {
                    return false;
                };
                Self::toggle_marker(&mut self.tag_base, &mut self.tag_target, name);
                true
            }
            _ => false,
        }
    }

    fn toggle_tag_marker(&mut self) {
        let Some(data) = self.repo_data.as_ref() else {
            return;
        };
        let Some(i) = self.selected_item_index() else {
            return;
        };
        let Some(tag) = data.tags.get(i) else {
            return;
        };
        let name = tag.name.clone();
        Self::toggle_marker(&mut self.tag_base, &mut self.tag_target, name);
    }

    fn toggle_commit_marker(&mut self) {
        let Some(data) = self.repo_data.as_ref() else {
            return;
        };
        let Some(i) = self.selected_item_index() else {
            return;
        };
        let Some(c) = data.commits.get(i) else {
            return;
        };
        let name = c.hash.clone();
        Self::toggle_marker(&mut self.commit_base, &mut self.commit_target, name);
    }

    fn cycle_sort(&mut self) {
        let pos = SORT_CYCLE
            .iter()
            .position(|&m| m == self.prefs.repo_sort)
            .unwrap_or(0);
        self.set_sort(SORT_CYCLE[(pos + 1) % SORT_CYCLE.len()]);
    }

    /// Which Home table column a screen x-coordinate falls in, using the
    /// bounds the renderer recorded. `None` before the first draw, or for an
    /// x past the last column.
    /// Which repository tab a screen x-coordinate falls in, from the bounds
    /// the renderer recorded. `None` before the first draw or past the last tab.
    pub fn repo_tab_at(&self, x: u16) -> Option<RepoTab> {
        let idx = self
            .tab_bounds
            .borrow()
            .iter()
            .position(|&(start, end)| x >= start && x < end)?;
        RepoTab::all().get(idx).copied()
    }

    pub fn home_column_at(&self, x: u16) -> Option<usize> {
        self.home_col_bounds
            .borrow()
            .iter()
            .position(|&(start, end)| x >= start && x < end)
    }

    /// Sort by the given Home table column, toggling direction if that column
    /// is already the active one — the behaviour a clickable table header is
    /// expected to have.
    pub fn sort_by_column(&mut self, column: usize) {
        let Some((primary, secondary)) = sort_modes_for_column(column) else {
            return;
        };
        let next = if self.prefs.repo_sort == primary {
            secondary
        } else {
            primary
        };
        self.set_sort(next);
    }

    /// Start these tests from a known sort order. `App::new()` reads the
    /// persisted `repo_sort`, and tests share one config directory, so a
    /// concurrently-running test could otherwise decide which direction the
    /// first header click produces.
    #[cfg(test)]
    pub fn set_sort_for_test(&mut self, mode: usize) {
        self.prefs.repo_sort = mode;
    }

    fn set_sort(&mut self, mode: usize) {
        self.prefs.repo_sort = mode;
        self.persist_prefs();
        self.home_selected = 0;
        self.status = format!("{} {}", self.tt("Sort:", "ソート:"), self.sort_label());
    }

    /// Off / 30s / 1m / 5m, in that order.
    const AUTO_REFRESH_OPTIONS: [u64; 4] = [0, 30, 60, 300];

    fn cycle_auto_refresh(&mut self) {
        let cur = Self::AUTO_REFRESH_OPTIONS
            .iter()
            .position(|&s| s == self.prefs.auto_refresh_secs)
            .unwrap_or(0);
        self.prefs.auto_refresh_secs =
            Self::AUTO_REFRESH_OPTIONS[(cur + 1) % Self::AUTO_REFRESH_OPTIONS.len()];
        self.persist_prefs();
        self.last_auto_refresh = std::time::Instant::now();
        self.status = format!(
            "{} {}",
            self.tt("Auto-refresh:", "自動更新:"),
            self.auto_refresh_label()
        );
    }

    /// Called on every event-loop tick. Home is the only screen that refreshes
    /// itself: it is where a user leaves the app open to watch many
    /// repositories, and refresh_home()'s per-repository jobs are already
    /// deduplicated by generation (see worker::spawn_worker), so a periodic
    /// call here cannot pile up work behind a slow repository.
    pub fn maybe_auto_refresh(&mut self) {
        if self.prefs.auto_refresh_secs == 0 || self.screen != Screen::Home {
            return;
        }
        if self.last_auto_refresh.elapsed()
            >= std::time::Duration::from_secs(self.prefs.auto_refresh_secs)
        {
            self.refresh_home();
        }
    }

    fn begin_rename_home(&mut self) {
        if let Some(&idx) = self.filtered_home().get(self.home_selected) {
            self.begin_rename(idx);
        }
    }

    fn begin_rename(&mut self, idx: usize) {
        let Some(repo) = self.repos.get(idx) else {
            return;
        };
        self.rename_idx = Some(idx);
        self.input = Some(InputKind::Rename);
        self.input_buf.clone_from(&repo.name);
    }

    fn rename_selected(&mut self, name: String) {
        let Some(idx) = self.rename_idx.take() else {
            return;
        };
        let name = name.trim();
        if name.is_empty() || idx >= self.repos.len() {
            return;
        }
        self.repos[idx].name = name.to_string();
        if let Err(e) = self.try_persist_repos() {
            self.error = Some(e);
        }
    }

    fn finish_add_repo(&mut self, alias: String) {
        let Some(path) = self.pending_add_path.take() else {
            return;
        };
        let default_name = path
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| path.display().to_string());
        let name = {
            let t = alias.trim();
            if t.is_empty() {
                default_name
            } else {
                t.to_string()
            }
        };
        self.repos.push(Repository {
            name,
            path: path.clone(),
            group: None,
        });
        if let Err(e) = self.try_persist_repos() {
            self.error = Some(e);
            self.repos.pop();
            return;
        }
        let index = self.repos.len() - 1;
        self.send_job(Job::LoadHome {
            generation: self.home_generation(),
            index,
            path,
            members: self.members.clone(),
        });
        self.status = self.t("added_success");
        if self.screen == Screen::Settings {
            self.settings_selected = index;
        }
    }

    fn request_commit_preview(&mut self) {
        let Some(idx) = self.repo_index else {
            return;
        };
        let hash = self.selected_item_index().and_then(|i| {
            self.repo_data
                .as_ref()?
                .commits
                .get(i)
                .map(|c| c.hash.clone())
        });
        let Some(hash) = hash else {
            self.commit_preview = None;
            return;
        };
        if self.commit_preview.as_ref().is_some_and(|p| p.hash == hash) {
            return;
        }
        let Some(repo) = self.repos.get(idx) else {
            return;
        };
        self.preview_seq += 1;
        let seq = self.preview_seq;
        self.send_job(Job::LoadCommitPreview {
            seq,
            path: repo.path.clone(),
            hash,
        });
    }

    fn launch_external(&mut self, base: Option<&str>, target: &str) -> bool {
        let cmd = self.prefs.diff_command.trim();
        if cmd.is_empty() {
            return false;
        }
        let Some(idx) = self.repo_index else {
            return false;
        };
        let Some(repo) = self.repos.get(idx) else {
            return false;
        };
        match resolve_diff_command(cmd, &repo.path, base, target) {
            Ok(ext) => {
                self.pending_external = Some(ext);
                true
            }
            Err(e) => {
                self.error = Some(e);
                false
            }
        }
    }

    fn begin_add_repo(&mut self) {
        self.input = Some(InputKind::AddRepo);
        self.input_buf = "./".to_string();
        self.path_completions.clear();
        self.status = self.tt(
            "Enter repository path, then Enter (Tab to complete)",
            "リポジトリのパスを入力して Enter (Tabで補完)",
        );
    }

    fn begin_bulk_add_repo(&mut self) {
        self.open_repo_finder(None);
    }

    pub fn filtered_finder_repos(&self) -> Vec<usize> {
        let Some(finder) = &self.repo_finder else {
            return Vec::new();
        };
        let needle = finder.filter.trim().to_lowercase();
        finder
            .repos
            .iter()
            .enumerate()
            .filter(|(_, r)| {
                if needle.is_empty() {
                    return true;
                }
                r.name.to_lowercase().contains(&needle)
                    || r.path.to_string_lossy().to_lowercase().contains(&needle)
                    || r.branch.to_lowercase().contains(&needle)
            })
            .map(|(i, _)| i)
            .collect()
    }

    pub fn open_repo_finder(&mut self, root: Option<PathBuf>) {
        let scan_root =
            root.unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
        let found_paths = git::find_git_repos(&scan_root, 4);
        let mut repos = Vec::new();
        for path in found_paths {
            let is_already_added = self.repos.iter().any(|r| r.path == path);
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .filter(|n| !n.is_empty())
                .unwrap_or_else(|| path.display().to_string());
            let branch = git::read_file_capped(&path.join(".git/HEAD"), 4096)
                .and_then(|h| {
                    let trimmed = h.trim();
                    if let Some(b) = trimmed.strip_prefix("ref: refs/heads/") {
                        Some(b.to_string())
                    } else if trimmed.is_empty() {
                        None
                    } else {
                        Some(short_hash(trimmed))
                    }
                })
                .unwrap_or_else(|| "HEAD".into());
            repos.push(FoundRepo {
                path,
                name,
                branch,
                is_already_added,
                is_selected: !is_already_added,
            });
        }
        self.repo_finder = Some(RepoFinderState {
            scan_root,
            repos,
            selected_idx: 0,
            filter: String::new(),
        });
        self.screen = Screen::RepoFinder;
    }

    fn handle_repo_finder(&mut self, key: KeyEvent) {
        let vis = self.filtered_finder_repos();
        let num_vis = vis.len();
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => {
                if let Some(finder) = self.repo_finder.as_mut()
                    && !finder.filter.is_empty()
                {
                    finder.filter.clear();
                    finder.selected_idx = 0;
                    return;
                }
                self.screen = Screen::Home;
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if let Some(finder) = self.repo_finder.as_mut()
                    && num_vis > 0
                {
                    finder.selected_idx = (finder.selected_idx + 1).min(num_vis - 1);
                }
            }
            KeyCode::Up | KeyCode::Char('k') => {
                if let Some(finder) = self.repo_finder.as_mut() {
                    finder.selected_idx = finder.selected_idx.saturating_sub(1);
                }
            }
            KeyCode::Char('g') => {
                if let Some(finder) = self.repo_finder.as_mut() {
                    finder.selected_idx = 0;
                }
            }
            KeyCode::Char('G') => {
                if let Some(finder) = self.repo_finder.as_mut() {
                    finder.selected_idx = num_vis.saturating_sub(1);
                }
            }
            KeyCode::Char(' ') => {
                if let Some(finder) = self.repo_finder.as_mut()
                    && let Some(&raw_idx) = vis.get(finder.selected_idx)
                    && let Some(item) = finder.repos.get_mut(raw_idx)
                    && !item.is_already_added
                {
                    item.is_selected = !item.is_selected;
                }
            }
            KeyCode::Char('a') => {
                if let Some(finder) = self.repo_finder.as_mut() {
                    let all_selected = finder
                        .repos
                        .iter()
                        .filter(|r| !r.is_already_added)
                        .all(|r| r.is_selected);
                    for r in finder.repos.iter_mut() {
                        if !r.is_already_added {
                            r.is_selected = !all_selected;
                        }
                    }
                }
            }
            KeyCode::Char('/') => {
                self.input = Some(InputKind::FinderFilter);
                if let Some(finder) = &self.repo_finder {
                    self.input_buf.clone_from(&finder.filter);
                }
            }
            KeyCode::Char('r') => {
                self.input = Some(InputKind::FinderScanPath);
                if let Some(finder) = &self.repo_finder {
                    self.input_buf = finder.scan_root.to_string_lossy().to_string();
                } else {
                    self.input_buf = "./".to_string();
                }
                self.status = self.tt(
                    "Enter directory path to scan, then Enter (Tab to complete)",
                    "スキャンする親ディレクトリを入力して Enter (Tabで補完)",
                );
            }
            KeyCode::Enter => {
                self.import_finder_selected();
            }
            _ => {}
        }
    }

    fn import_finder_selected(&mut self) {
        let Some(finder) = self.repo_finder.take() else {
            self.screen = Screen::Home;
            return;
        };
        let mut added = 0usize;
        for item in finder.repos {
            if item.is_selected && !item.is_already_added {
                self.repos.push(Repository {
                    name: item.name,
                    path: item.path.clone(),
                    group: None,
                });
                let index = self.repos.len() - 1;
                self.send_job(Job::LoadHome {
                    generation: self.home_generation(),
                    index,
                    path: item.path,
                    members: self.members.clone(),
                });
                added += 1;
            }
        }
        if added > 0 {
            self.persist_repos();
            self.status = match self.lang() {
                Language::English => format!("Imported {added} repository(ies)"),
                Language::Japanese => format!("{added}件のリポジトリを登録しました"),
            };
        }
        self.screen = Screen::Home;
    }

    fn bulk_add_repos_from_path(&mut self, path_str: &str) {
        let s = path_str.trim();
        if s.is_empty() {
            return;
        }
        let path = expand_user_path(s);
        let found = git::find_git_repos(&path, 3);
        if found.is_empty() {
            self.error = Some(format!(
                "{}: {}",
                self.tt(
                    "No git repositories found in",
                    "Git リポジトリが見つかりませんでした"
                ),
                path.display()
            ));
            return;
        }
        let mut added = 0usize;
        let mut skipped = 0usize;
        for repo_path in found {
            if self.repos.iter().any(|r| r.path == repo_path) {
                skipped += 1;
                continue;
            }
            let name = repo_path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .filter(|n| !n.is_empty())
                .unwrap_or_else(|| repo_path.display().to_string());
            self.repos.push(Repository {
                name,
                path: repo_path.clone(),
                group: None,
            });
            let index = self.repos.len() - 1;
            self.send_job(Job::LoadHome {
                generation: self.home_generation(),
                index,
                path: repo_path,
                members: self.members.clone(),
            });
            added += 1;
        }
        if added > 0
            && let Err(e) = self.try_persist_repos()
        {
            self.error = Some(e);
            return;
        }
        self.status = match self.lang() {
            Language::English => format!("Added {added} repository(ies) (skipped {skipped})"),
            Language::Japanese => {
                format!("{added}件のリポジトリを追加しました (スキップ {skipped}件)")
            }
        };
    }

    fn add_repo_from_path(&mut self, path_str: &str) {
        if path_str.trim().is_empty() {
            return;
        }
        let path = expand_user_path(path_str);
        if !git::is_git_repo(&path) {
            if path.is_dir() {
                let found = git::find_git_repos(&path, 3);
                if !found.is_empty() {
                    self.bulk_add_repos_from_path(path_str);
                    return;
                }
            }
            self.error = Some(format!(
                "{}: {}",
                self.tt("Not a git repository", "Git リポジトリではありません"),
                path.display()
            ));
            return;
        }
        if self.repos.iter().any(|r| r.path == path) {
            self.error = Some(self.t("already_registered"));
            return;
        }
        let default_name = path
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| path.display().to_string());
        self.pending_add_path = Some(path);
        self.input = Some(InputKind::AddAlias);
        self.input_buf = default_name;
    }

    fn toggle_contributor_active(&mut self) {
        let Some(item_idx) = self.selected_item_index() else {
            return;
        };
        let Some(data) = self.repo_data.as_ref() else {
            return;
        };
        let Some(c) = data.contributors.get(item_idx) else {
            return;
        };
        let c_name = c.name.clone();
        let c_email = c.email.clone();

        let existing_idx = self.members.iter().position(|m| {
            m.canonical_name.eq_ignore_ascii_case(&c_name)
                || m.aliases.iter().any(|a| {
                    a.eq_ignore_ascii_case(&c_name)
                        || (!c_email.is_empty() && a.eq_ignore_ascii_case(&c_email))
                })
        });

        let (name, is_active) = if let Some(idx) = existing_idx {
            self.members[idx].is_active = !self.members[idx].is_active;
            (
                self.members[idx].canonical_name.clone(),
                self.members[idx].is_active,
            )
        } else {
            let mut aliases = Vec::new();
            if !c_name.is_empty() {
                aliases.push(c_name.clone());
            }
            if !c_email.is_empty() && !aliases.contains(&c_email) {
                aliases.push(c_email.clone());
            }
            self.members.push(Member {
                canonical_name: c_name.clone(),
                aliases,
                is_active: true,
            });
            (c_name, true)
        };

        self.persist_members();

        self.status = if is_active {
            format!("{name}: {}", self.tt("Active", "在籍中に設定"))
        } else {
            format!("{name}: {}", self.tt("Inactive", "非在籍に設定"))
        };

        self.refresh_contributors_status();
    }

    fn refresh_contributors_status(&mut self) {
        let Some(data) = self.repo_data.as_mut() else {
            return;
        };
        for c in &mut data.contributors {
            let mut found = false;
            let mut active = false;
            for m in &self.members {
                if m.canonical_name.eq_ignore_ascii_case(&c.name)
                    || m.aliases.iter().any(|a| {
                        a.eq_ignore_ascii_case(&c.name)
                            || (!c.email.is_empty() && a.eq_ignore_ascii_case(&c.email))
                    })
                {
                    found = true;
                    active = m.is_active;
                    break;
                }
            }
            c.is_member = found;
            c.is_active = active;
        }
    }

    fn toggle_member_active(&mut self, idx: usize) {
        if idx >= self.members.len() {
            return;
        }
        self.members[idx].is_active = !self.members[idx].is_active;
        let is_active = self.members[idx].is_active;
        let name = self.members[idx].canonical_name.clone();
        self.persist_members();
        self.status = if is_active {
            format!("{name}: {}", self.tt("Active", "在籍中に設定"))
        } else {
            format!("{name}: {}", self.tt("Inactive", "非在籍に設定"))
        };
        self.refresh_contributors_status();
    }

    fn delete_member(&mut self, idx: usize) {
        if idx >= self.members.len() {
            return;
        }
        let removed = self.members.remove(idx);
        if let Err(e) = self.try_persist_members() {
            self.members.insert(idx, removed);
            self.error = Some(e);
            return;
        }
        self.settings_member_selected =
            clamp_index(self.settings_member_selected, self.members.len());
        self.status = format!(
            "{}: {}",
            self.tt("Deleted member", "メンバーを削除しました"),
            removed.canonical_name
        );
        self.refresh_contributors_status();
    }

    fn begin_add_member(&mut self) {
        self.input = Some(InputKind::AddMemberName);
        self.input_buf.clear();
        self.status = self.tt("Enter new member name", "新規メンバー名を入力して Enter");
    }

    fn finish_add_member_name(&mut self, name: String) {
        let name = name.trim().to_string();
        if name.is_empty() {
            return;
        }
        self.pending_member_name = Some(name.clone());
        self.input = Some(InputKind::AddMemberAliases);
        self.input_buf = name;
        self.status = self.tt(
            "Enter aliases (comma-separated), then Enter",
            "別名を入力（カンマ区切り）して Enter",
        );
    }

    fn finish_add_member_aliases(&mut self, aliases_str: String) {
        let Some(name) = self.pending_member_name.take() else {
            return;
        };
        let aliases: Vec<String> = aliases_str
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        self.members.push(Member {
            canonical_name: name.clone(),
            aliases,
            is_active: true,
        });
        if let Err(e) = self.try_persist_members() {
            self.error = Some(e);
            self.members.pop();
            return;
        }
        self.settings_member_selected = self.members.len() - 1;
        self.status = format!(
            "{}: {name}",
            self.tt("Registered member", "メンバーを登録しました")
        );
        self.refresh_contributors_status();
    }

    fn begin_edit_member_aliases(&mut self, idx: usize) {
        let Some(m) = self.members.get(idx) else {
            return;
        };
        self.editing_member_idx = Some(idx);
        self.input = Some(InputKind::EditMemberAliases);
        self.input_buf = m.aliases.join(", ");
        self.status = self.tt(
            "Edit aliases (comma-separated), then Enter",
            "別名を編集（カンマ区切り）して Enter",
        );
    }

    fn finish_edit_member_aliases(&mut self, aliases_str: String) {
        let Some(idx) = self.editing_member_idx.take() else {
            return;
        };
        if idx >= self.members.len() {
            return;
        }
        let aliases: Vec<String> = aliases_str
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        self.members[idx].aliases = aliases;
        self.persist_members();
        self.status = format!(
            "{}: {}",
            self.tt("Updated aliases for", "別名を更新しました"),
            self.members[idx].canonical_name
        );
        self.refresh_contributors_status();
    }

    fn begin_edit_repo_group(&mut self, idx: usize) {
        let Some(r) = self.repos.get(idx) else {
            return;
        };
        self.editing_repo_group_idx = Some(idx);
        self.input = Some(InputKind::EditRepoGroup);
        self.input_buf = r.group.clone().unwrap_or_default();
        self.status = self.tt(
            "Enter group name (empty to clear), then Enter",
            "グループ名を入力（空で解除）して Enter",
        );
    }

    fn finish_edit_repo_group(&mut self, group_str: String) {
        let Some(idx) = self.editing_repo_group_idx.take() else {
            return;
        };
        if idx >= self.repos.len() {
            return;
        }
        let trimmed = group_str.trim().to_string();
        self.repos[idx].group = if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        };
        self.persist_repos();
        self.status = self.tt(
            "Repository group updated",
            "リポジトリのグループを更新しました",
        );
    }

    fn bulk_pull(&mut self) {
        let targets = self.filtered_home();
        if targets.is_empty() {
            return;
        }
        let count = targets.len();
        let lang = self.prefs.language;
        // Each pull refreshes its own row when it finishes, so no LoadHome is
        // queued up front — that only produced results the pull invalidated.
        let jobs: Vec<Job> = targets
            .iter()
            .filter_map(|&idx| {
                self.repos.get(idx).map(|r| Job::Pull {
                    index: idx,
                    path: r.path.clone(),
                    lang,
                })
            })
            .collect();
        for job in jobs {
            self.send_job(job);
        }
        self.status = format!(
            "{} ({count} repos)",
            self.tt(
                "Started bulk pull in background",
                "一括 pull をバックグラウンドで開始しました"
            )
        );
    }

    fn bulk_fetch(&mut self) {
        let targets = self.filtered_home();
        if targets.is_empty() {
            return;
        }
        let count = targets.len();
        let lang = self.prefs.language;
        // Each fetch refreshes its own row when it finishes, so no LoadHome is
        // queued up front — that only produced results the fetch invalidated.
        let jobs: Vec<Job> = targets
            .iter()
            .filter_map(|&idx| {
                self.repos.get(idx).map(|r| Job::Fetch {
                    index: idx,
                    path: r.path.clone(),
                    lang,
                })
            })
            .collect();
        for job in jobs {
            self.send_job(job);
        }
        self.status = format!(
            "{} ({count} repos)",
            self.tt(
                "Started bulk fetch in background",
                "一括 fetch をバックグラウンドで開始しました"
            )
        );
    }

    fn cycle_contributor_time_span(&mut self) {
        self.contributor_time_span = self.contributor_time_span.next();
        self.status = format!(
            "{}: {}",
            self.tt("Time span", "集計期間"),
            match self.prefs.language {
                Language::English => self.contributor_time_span.label_en(),
                Language::Japanese => self.contributor_time_span.label_ja(),
            }
        );
        if let Some(idx) = self.repo_index {
            self.reload_repo(idx);
        }
    }

    /// Search commit messages across every repository (`S` on Home). Runs on
    /// the worker like the cross-repo member aggregation, since it is one
    /// `git log` per repository.
    fn run_commit_search(&mut self, query: String) {
        let query = query.trim().to_string();
        if query.is_empty() {
            self.commit_search = None;
            return;
        }
        self.search_seq += 1;
        let seq = self.search_seq;
        self.commit_search = Some(CommitSearchState {
            query: query.clone(),
            hits: Vec::new(),
            selected: 0,
            loading: true,
        });
        self.screen = Screen::CommitSearch;
        let repos = self
            .repos
            .iter()
            .enumerate()
            .map(|(i, r)| (i, r.name.clone(), r.path.clone()))
            .collect();
        let job = Job::SearchCommits { seq, query, repos };
        self.send_job(job);
    }

    pub fn open_global_members(&mut self) {
        self.global_member_selected = 0;
        self.global_member_repo_selected = 0;
        self.global_member_pane = FocusPane::List;
        self.global_member_filter.clear();
        self.screen = Screen::GlobalMembers;
        self.request_global_members();
    }

    /// Queue the cross-repository contributor aggregation. It walks `git log`
    /// in every repository, which used to run inline on the UI thread.
    pub fn request_global_members(&mut self) {
        let repos = self
            .repos
            .iter()
            .enumerate()
            .map(|(i, r)| (i, r.name.clone(), r.path.clone()))
            .collect();
        let generation = self.global_gen.saturating_add(1);
        self.global_gen = generation;
        self.global_members_loading = true;
        self.status = self.tt("Aggregating members...", "メンバーを集計しています...");
        let job = Job::LoadGlobalMembers {
            generation,
            repos,
            members: self.members.clone(),
        };
        self.send_job(job);
    }

    pub fn filtered_global_members(&self) -> Vec<usize> {
        let q = self.global_member_filter.trim().to_lowercase();
        self.global_members
            .iter()
            .enumerate()
            .filter(|(_, m)| {
                if self.active_only && !m.is_active {
                    return false;
                }
                if q.is_empty() {
                    return true;
                }
                m.canonical_name.to_lowercase().contains(&q)
                    || m.aliases.iter().any(|a| a.to_lowercase().contains(&q))
                    || m.contributions
                        .iter()
                        .any(|c| c.repo_name.to_lowercase().contains(&q))
            })
            .map(|(i, _)| i)
            .collect()
    }

    fn delete_repo(&mut self, i: usize) {
        if i >= self.repos.len() {
            return;
        }
        let removed = self.repos.remove(i);
        if let Err(e) = self.try_persist_repos() {
            self.repos.insert(i, removed);
            self.error = Some(e);
            return;
        }
        // Drop the on-disk caches too, so removing a repository doesn't leave
        // files behind that would be served if it is ever re-added.
        let _ = std::fs::remove_file(config::home_cache_path(&removed.path));
        let _ = std::fs::remove_file(config::tui_cache_path(&removed.path));
        self.home_rows.remove(&i);
        let shifted: HashMap<_, _> = self
            .home_rows
            .drain()
            .filter_map(|(k, v)| {
                if k == i {
                    None
                } else if k > i {
                    Some((k - 1, v))
                } else {
                    Some((k, v))
                }
            })
            .collect();
        self.home_rows = shifted;
        // repo_index addresses self.repos by position, so leaving it alone
        // would silently repoint the open repository at a different one — and
        // an in-flight RepoLoaded for the old index would then be written into
        // the new repository's on-disk cache.
        match self.repo_index {
            Some(idx) if idx == i => {
                self.repo_index = None;
                self.repo_data = None;
                self.repo_loading = false;
                if matches!(self.screen, Screen::Repo | Screen::Diff) {
                    self.screen = Screen::Home;
                }
            }
            Some(idx) if idx > i => self.repo_index = Some(idx - 1),
            _ => {}
        }
        self.home_selected = clamp_index(self.home_selected, self.filtered_home().len());
        if self.settings_selected >= self.repos.len() && !self.repos.is_empty() {
            self.settings_selected = self.repos.len() - 1;
        }
        self.status = self.tt("Repository removed.", "リポジトリを削除しました。");
        self.refresh_home();
    }

    fn pull_selected_home(&mut self) {
        if let Some(&idx) = self.filtered_home().get(self.home_selected) {
            self.pull_repo(idx);
        }
    }

    fn fetch_selected_home(&mut self) {
        if let Some(&idx) = self.filtered_home().get(self.home_selected) {
            self.fetch_repo(idx);
        }
    }

    fn pull_current_repo(&mut self) {
        if let Some(idx) = self.repo_index {
            self.pull_repo(idx);
        }
    }

    fn fetch_current_repo(&mut self) {
        if let Some(idx) = self.repo_index {
            self.fetch_repo(idx);
        }
    }

    fn pull_repo(&mut self, idx: usize) {
        let Some(repo) = self.repos.get(idx) else {
            return;
        };
        let job = Job::Pull {
            index: idx,
            path: repo.path.clone(),
            lang: self.prefs.language,
        };
        self.status = self.tt("Pulling...", "Pull しています...");
        self.send_job(job);
    }

    fn fetch_repo(&mut self, idx: usize) {
        let Some(repo) = self.repos.get(idx) else {
            return;
        };
        let job = Job::Fetch {
            index: idx,
            path: repo.path.clone(),
            lang: self.prefs.language,
        };
        self.status = self.tt("Fetching...", "Fetch しています...");
        self.send_job(job);
    }

    fn apply_selected_stash(&mut self) {
        let Some(idx) = self.repo_index else {
            return;
        };
        let Some(data) = self.repo_data.as_ref() else {
            return;
        };
        let stash_ref = self
            .selected_item_index()
            .and_then(|i| data.stashes.get(i).map(|st| st.ref_name.clone()));
        let Some(stash_ref) = stash_ref else {
            return;
        };
        let Some(repo) = self.repos.get(idx) else {
            return;
        };
        self.status = self.t("apply_stash");
        self.send_job(Job::StashApply {
            path: repo.path.clone(),
            stash_ref,
        });
    }

    fn drop_stash(&mut self, repo_idx: usize, stash_ref: String) {
        let Some(repo) = self.repos.get(repo_idx) else {
            return;
        };
        self.send_job(Job::StashDrop {
            path: repo.path.clone(),
            stash_ref,
        });
    }

    #[cfg(test)]
    pub fn set_language_for_test(&mut self, lang: config::Language) {
        self.prefs.language = lang;
    }

    fn toggle_language(&mut self) {
        self.prefs.language = match self.prefs.language {
            Language::English => Language::Japanese,
            Language::Japanese => Language::English,
        };
        self.persist_prefs();
    }

    #[cfg(test)]
    pub fn apply_msg_for_test(&mut self, msg: Msg) {
        self.apply_msg(msg);
    }

    fn apply_msg(&mut self, msg: Msg) {
        // Clear the per-repo marker first: a superseded result still means
        // that job is no longer running, and returning early below would
        // otherwise leave the row spinning forever.
        match &msg {
            Msg::HomeLoaded { index, .. } => {
                self.busy.remove(index);
            }
            Msg::OpDone {
                repo_index: Some(index),
                ..
            } => {
                self.busy.remove(index);
            }
            _ => {}
        }
        match msg {
            Msg::HomeLoaded {
                generation,
                index,
                row,
            } => {
                if generation != self.home_generation() {
                    return;
                }
                match row {
                    Ok(r) => {
                        if let Some(repo) = self.repos.get(index) {
                            save_home_cache(&repo.path, &r);
                        }
                        self.home_rows.insert(index, r);
                        if self.screen == Screen::Home {
                            self.status.clear();
                        }
                    }
                    Err(e) => {
                        self.home_rows.remove(&index);
                        self.error = Some(e);
                    }
                }
                // A row loading/failing can change filtered_home()'s length
                // out from under the cursor — most visibly with the `n`
                // (needs-attention) filter, where a background refresh can
                // resolve a repo and shrink the list with no user action.
                self.home_selected = clamp_index(self.home_selected, self.filtered_home().len());
            }
            Msg::RepoLoaded { index, data } => {
                if self.repo_index != Some(index) {
                    return;
                }
                self.repo_loading = false;
                match *data {
                    Ok(snap) => {
                        self.status.clear();
                        if let Some(repo) = self.repos.get(index) {
                            save_tui_cache(&repo.path, &snap);
                        }
                        self.repo_data = Some(snap);
                        if self.repo_tab == RepoTab::Commits {
                            self.request_commit_preview();
                        }
                    }
                    Err(e) => self.error = Some(e),
                }
            }
            Msg::DiffLoaded { seq, result } => {
                if seq != self.diff_seq {
                    return;
                }
                let Some(diff) = self.diff.as_mut() else {
                    return;
                };
                diff.loading = false;
                match result {
                    Ok(fd) => {
                        diff.error = None;
                        diff.lines = flatten_diff(&fd);
                        apply_hunks(diff);
                        if let Some(saved) = diff.pending_scroll_restore.take() {
                            diff.scroll = saved.min(diff.lines.len().saturating_sub(1));
                            sync_hunk_from_scroll(diff);
                        }
                    }
                    Err(e) => {
                        diff.pending_scroll_restore = None;
                        diff.error = Some(e);
                        diff.lines.clear();
                        diff.hunks.clear();
                        diff.hunk_idx = 0;
                    }
                }
            }
            Msg::BlameLoaded { seq, result } => {
                if seq != self.diff_seq {
                    return;
                }
                let Some(diff) = self.diff.as_mut() else {
                    return;
                };
                diff.blame_loading = false;
                match result {
                    Ok(entries) => diff.blame = Some(entries),
                    Err(e) => {
                        diff.blame = None;
                        self.error = Some(e);
                    }
                }
            }
            Msg::CommitMeta { seq, header, files } => {
                if seq != self.diff_seq {
                    return;
                }
                let load = {
                    let Some(diff) = self.diff.as_mut() else {
                        return;
                    };
                    diff.header = header.ok();
                    match files {
                        Ok(f) => {
                            diff.files = f;
                            diff.file_idx = 0;
                            true
                        }
                        Err(e) => {
                            diff.loading = false;
                            diff.error = Some(e);
                            false
                        }
                    }
                };
                if load {
                    self.load_selected_diff_file();
                }
            }
            Msg::FilesLoaded {
                seq,
                files,
                preselect,
            } => {
                if seq != self.diff_seq {
                    return;
                }
                let mut focus_content = false;
                let load = {
                    let Some(diff) = self.diff.as_mut() else {
                        return;
                    };
                    match files {
                        Ok(f) => {
                            diff.file_idx = preselect
                                .as_ref()
                                .and_then(|p| f.iter().position(|x| &x.path == p))
                                .unwrap_or(0);
                            diff.files = f;
                            focus_content = preselect.is_some();
                            true
                        }
                        Err(e) => {
                            diff.loading = false;
                            diff.error = Some(e);
                            false
                        }
                    }
                };
                if focus_content {
                    self.focus = FocusPane::Content;
                }
                if load {
                    self.load_selected_diff_file();
                }
            }
            Msg::OpDone {
                ok,
                text,
                repo_index,
            } => {
                if !ok {
                    self.error = Some(text);
                    return;
                }
                self.status = text;
                if let Some(idx) = self.repo_index {
                    self.reload_repo(idx);
                } else if let Some(idx) = repo_index {
                    self.refresh_home_row(idx);
                } else {
                    self.refresh_home();
                }
            }
            Msg::CommitSearchLoaded {
                seq,
                hits,
                failed_repos,
            } => {
                if seq != self.search_seq {
                    return;
                }
                let Some(search) = self.commit_search.as_mut() else {
                    return;
                };
                search.loading = false;
                search.hits = hits;
                search.selected = 0;
                // Surfaced without discarding hits from repos that *did*
                // search successfully — a failure and "no matches" must not
                // look identical to the user.
                if !failed_repos.is_empty() {
                    self.error = Some(format!(
                        "{}: {}",
                        self.tt(
                            "Could not search some repositories",
                            "検索できなかったリポジトリ"
                        ),
                        failed_repos.join(", ")
                    ));
                }
            }
            Msg::GlobalMembersLoaded { generation, list } => {
                if generation != self.global_gen {
                    return;
                }
                self.global_members_loading = false;
                self.global_members = list;
                self.global_member_selected =
                    clamp_index(self.global_member_selected, self.global_members.len());
                self.global_member_repo_selected = 0;
                self.status.clear();
            }
            Msg::CommitPreviewLoaded {
                seq,
                hash,
                header,
                files,
            } => {
                if seq != self.preview_seq {
                    return;
                }
                self.commit_preview = Some(CommitPreview {
                    hash,
                    header: header.unwrap_or_default(),
                    files: files.unwrap_or_default(),
                });
            }
            Msg::LogLoaded { title, body } => match body {
                Ok(raw) => {
                    self.status.clear();
                    self.log = Some(LogView {
                        title,
                        // The oneline log keeps git's colours, so it is not
                        // tab-expanded upstream; a tabbed commit subject would
                        // otherwise skew the whole pane.
                        body: strip_ansi(&raw)
                            .lines()
                            .map(|l| crate::git::expand_tabs(l, crate::git::TAB_WIDTH))
                            .collect::<Vec<_>>()
                            .join("\n"),
                        scroll: 0,
                    });
                    self.screen = Screen::Log;
                }
                Err(e) => self.error = Some(e),
            },
        }
    }
}

#[cfg(test)]
mod tests;
