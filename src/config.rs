#[cfg(not(test))]
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

/// Write to a sibling temp file then rename, so a crash mid-write
/// never leaves a truncated/corrupt JSON behind. The temp name is unique
/// per call because worker threads may save the same file concurrently.
pub(crate) fn write_atomic(path: &Path, content: &str) -> std::io::Result<()> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let tmp = path.with_extension(format!("tmp.{}.{}", std::process::id(), n));
    fs::write(&tmp, content)?;
    fs::rename(&tmp, path)
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Repository {
    pub name: String,
    pub path: PathBuf,
    #[serde(default)]
    pub group: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Member {
    pub canonical_name: String,
    pub aliases: Vec<String>,
    pub is_active: bool,
}

/// In test builds, every config read/write is redirected to a process-local
/// temp directory instead of the real (and, for the sibling GUI app, shared)
/// config file. Without this, `cargo test` permanently overwrites whatever
/// theme/diff/auto-refresh preferences the developer actually has set.
#[cfg(test)]
fn test_config_dir() -> PathBuf {
    use std::sync::OnceLock;
    static DIR: OnceLock<PathBuf> = OnceLock::new();
    DIR.get_or_init(|| {
        let dir = std::env::temp_dir().join(format!(
            "git-dashboard-tui-test-config-{}",
            std::process::id()
        ));
        let _ = fs::create_dir_all(&dir);
        dir
    })
    .clone()
}

#[cfg(test)]
pub fn get_config_dir() -> PathBuf {
    test_config_dir()
}

#[cfg(not(test))]
pub fn get_config_dir() -> PathBuf {
    if let Some(path) = std::env::var_os("GIT_DASHBOARD_CONFIG_DIR").filter(|p| !p.is_empty()) {
        return PathBuf::from(path);
    }
    if let Some(proj_dirs) = ProjectDirs::from("com", "git-dashboard", "git-dashboard") {
        let path = proj_dirs.config_dir();
        if !path.exists() {
            let _ = fs::create_dir_all(path);

            // Migrate existing local files if they exist in current directory
            for filename in &["config.json", "members.json", "tech_rules.json"] {
                let local = Path::new(filename);
                if local.exists() {
                    let dest = path.join(filename);
                    let _ = fs::copy(local, dest);
                }
            }
        }
        path.to_path_buf()
    } else {
        PathBuf::from(".")
    }
}

/// Directory for cached analysis results (one JSON per repository).
pub fn cache_dir() -> PathBuf {
    get_config_dir().join("cache")
}

/// Cache file path for a repository, keyed by a hash of its absolute path.
pub fn repo_cache_path(repo_path: &Path) -> PathBuf {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    repo_path.hash(&mut h);
    cache_dir().join(format!("repo-{:016x}.json", h.finish()))
}

pub fn tui_cache_path(repo_path: &Path) -> PathBuf {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    repo_path.hash(&mut h);
    cache_dir().join(format!("tui-{:016x}.json", h.finish()))
}

/// Cache file for a repository's Home row. Separate from `tui_cache_path`
/// because the two are written at different times: the Home row after a
/// dashboard refresh, the full snapshot only once a repository is opened.
pub fn home_cache_path(repo_path: &Path) -> PathBuf {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    repo_path.hash(&mut h);
    cache_dir().join(format!("home-{:016x}.json", h.finish()))
}

/// Load a JSON config file. `Ok(None)` means "not written yet"; a parse or read
/// failure is an error rather than an empty value, because silently returning
/// the default would present a corrupt config as an empty one — and the next
/// save would then overwrite the user's real data with that empty default.
fn load_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<Option<T>, String> {
    if !path.exists() {
        return Ok(None);
    }
    let content = fs::read_to_string(path)
        .map_err(|e| format!("{} を読み込めません: {e}", path.display()))?;
    serde_json::from_str(&content)
        .map(Some)
        .map_err(|e| format!("{} を解析できません: {e}", path.display()))
}

pub fn load_repositories() -> Result<Vec<Repository>, String> {
    // Fresh install: start with no repositories (the user registers their own)
    Ok(load_json(&get_config_dir().join("config.json"))?.unwrap_or_default())
}

pub fn save_repositories(repos: &[Repository]) -> Result<(), String> {
    let config_dir = get_config_dir();
    let path = config_dir.join("config.json");
    let content =
        serde_json::to_string_pretty(repos).map_err(|e| format!("Serialization error: {}", e))?;
    write_atomic(&path, &content).map_err(|e| format!("Failed to write config file: {}", e))
}

pub fn load_members() -> Result<Vec<Member>, String> {
    // Fresh install: start with no members (name merging is opt-in)
    Ok(load_json(&get_config_dir().join("members.json"))?.unwrap_or_default())
}

pub fn save_members(members: &[Member]) -> Result<(), String> {
    let config_dir = get_config_dir();
    let path = config_dir.join("members.json");
    let content =
        serde_json::to_string_pretty(members).map_err(|e| format!("Serialization error: {}", e))?;
    write_atomic(&path, &content).map_err(|e| format!("Failed to write members config file: {}", e))
}

/// One remembered base→target comparison (per repository, newest first).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct RecentCompare {
    pub repo_path: String,
    pub base: String,
    pub target: String,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Language {
    #[default]
    English,
    Japanese,
}

/// Shared byte-for-byte with the sibling GUI app's `prefs.json` in the same
/// config directory — a field this TUI adds (e.g. `auto_refresh_secs`) is
/// unknown to the GUI, and a `theme` value the GUI writes may not be one
/// [`crate::colors::Palette::for_name`] recognises. `#[serde(default)]`
/// keeps either app's writes from *erroring* on the other's fields, but
/// **cannot** stop a save from either side from silently dropping fields it
/// doesn't know about — that would need coordinating the schema with the GUI
/// app's own codebase, which this repository doesn't own. Not fixable from
/// here; documented so it isn't mistaken for an oversight.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct Preferences {
    pub onboarding_dismissed: bool,
    pub theme: String,
    pub language: Language,
    pub sidebar_collapsed: bool,
    pub sidebar_width: f32,
    pub editor_command: String,
    pub editor_wait: bool,
    pub worktree_notes: std::collections::HashMap<String, WorktreeNote>,
    // Diff view toggles, persisted across sessions
    pub diff_ignore_whitespace: bool,
    pub diff_full_file: bool,
    pub diff_show_blame: bool,
    // Recent range comparisons (capped per repo and overall)
    pub recent_compares: Vec<RecentCompare>,
    /// External diff tool. Empty = TUI builtin. `{path}` `{range}` `{base}` `{target}` `{file}`
    /// expand. A bare `hunk` uses `hunk diff` (working tree or `aaaa..bbbb`)
    /// and `hunk show <hash>` (single commit). A space between refs is a file compare.
    #[serde(default)]
    pub diff_command: String,
    /// Sort order for the Home list. 0-3 keep the meanings the sibling GUI
    /// writes — 0=name asc, 1=name desc, 2=updated desc, 3=updated asc — and
    /// 4-9 add branch, dirty and sync in both directions; see the `SORT_*`
    /// constants in `app::helpers`. A value this build does not recognise
    /// falls back to newest-first rather than being rejected.
    #[serde(default = "default_repo_sort")]
    pub repo_sort: usize,
    /// Seconds between automatic Home refreshes; 0 disables it (the
    /// default). Cycled with `i` in Settings through `AUTO_REFRESH_OPTIONS`.
    ///
    /// Off by default: local analysis can still be expensive. GitHub has a
    /// separate five-minute cache and a one-minute failure retry delay.
    /// Opting an existing prefs.json into automatic refresh on
    /// upgrade would silently turn an idle screen into a background worker.
    #[serde(default = "default_auto_refresh_secs")]
    pub auto_refresh_secs: u64,
}

fn default_repo_sort() -> usize {
    2
}

fn default_auto_refresh_secs() -> u64 {
    0
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            onboarding_dismissed: false,
            theme: crate::colors::THEME_MOCHA.to_string(),
            language: Language::English,
            sidebar_collapsed: false,
            sidebar_width: 260.0,
            editor_command: "code".to_string(),
            editor_wait: false,
            worktree_notes: Default::default(),
            diff_ignore_whitespace: false,
            diff_full_file: false,
            diff_show_blame: false,
            recent_compares: Vec::new(),
            diff_command: String::new(),
            repo_sort: 2,
            auto_refresh_secs: default_auto_refresh_secs(),
        }
    }
}

pub fn load_preferences() -> Result<Preferences, String> {
    match load_json(&get_config_dir().join("prefs.json"))? {
        Some(prefs) => Ok(prefs),
        None => {
            let default_prefs = Preferences::default();
            save_preferences(&default_prefs)?;
            Ok(default_prefs)
        }
    }
}

pub fn save_preferences(prefs: &Preferences) -> Result<(), String> {
    let config_dir = get_config_dir();
    let path = config_dir.join("prefs.json");
    let content =
        serde_json::to_string_pretty(prefs).map_err(|e| format!("Serialization error: {}", e))?;
    write_atomic(&path, &content).map_err(|e| format!("Failed to write config file: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_language_serde_roundtrip() {
        let prefs = Preferences {
            language: Language::Japanese,
            ..Preferences::default()
        };
        let json = serde_json::to_string(&prefs).unwrap();
        assert!(json.contains("\"language\":\"Japanese\""));
        let decoded: Preferences = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.language, Language::Japanese);
    }

    #[test]
    fn test_load_json_reports_corruption_instead_of_defaulting() {
        let dir = std::env::temp_dir().join(format!("gdt-cfg-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.json");

        // Missing file is "not written yet", not an error
        assert!(load_json::<Vec<Repository>>(&path).unwrap().is_none());

        // A corrupt file must not read as an empty repository list — the app
        // would then save that empty list back over the user's data.
        fs::write(&path, "{ this is not json").unwrap();
        assert!(load_json::<Vec<Repository>>(&path).is_err());

        fs::write(&path, "[]").unwrap();
        assert_eq!(
            load_json::<Vec<Repository>>(&path).unwrap(),
            Some(Vec::new())
        );
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_language_default_fallback() {
        // Without language key, serde_json should fall back to Default (English)
        let json = r#"{"theme":"Catppuccin Mocha"}"#;
        let decoded: Preferences = serde_json::from_str(json).unwrap();
        assert_eq!(decoded.language, Language::English);
        assert!(decoded.diff_command.is_empty());
        assert_eq!(decoded.repo_sort, 2);
        assert_eq!(decoded.auto_refresh_secs, 0);
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
#[serde(default)]
pub struct WorktreeNote {
    pub note: String,
    pub favorite: bool,
}
