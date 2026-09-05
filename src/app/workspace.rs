use super::*;
use std::collections::HashSet;

/// One bulk worker bounds concurrency to one Git subprocess at a time; the
/// primary worker remains available for repository/diff operations.
pub fn collect_workspace(
    repos: &[(String, PathBuf)],
    cancelled: impl Fn() -> bool,
    mut emit: impl FnMut(WorkspaceRow),
) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut errors = Vec::new();
    for (parent, repo) in repos {
        if cancelled() {
            break;
        }
        if git::parse_ssh_repo(repo).is_some() {
            continue;
        }
        let trees = match git::get_worktrees(repo) {
            Ok(trees) => trees,
            Err(e) => {
                errors.push(format!("{parent}: {e}"));
                continue;
            }
        };
        for tree in trees {
            if cancelled() {
                return errors;
            }
            if tree.is_bare {
                continue;
            }
            let path = PathBuf::from(&tree.path);
            let path = path.canonicalize().unwrap_or(path);
            if !seen.insert(path.clone()) {
                continue;
            }
            let mut row = WorkspaceRow {
                parent: parent.clone(),
                path: path.clone(),
                branch: tree.branch.unwrap_or_else(|| "(detached)".into()),
                dirty: None,
                last_commit: String::new(),
                checked_at: chrono::Utc::now().timestamp(),
                locked: tree.is_locked,
                prunable: tree.is_prunable,
                error: None,
            };
            match worktree_dirty(&path) {
                Ok(count) => row.dirty = Some(count),
                Err(e) => row.error = Some(e),
            }
            if cancelled() {
                return errors;
            }
            match git::run_git_cmd(&path, &["log", "-1", "--format=%ci"]) {
                Ok(date) => row.last_commit = date.trim().to_owned(),
                Err(e) => {
                    if row.error.is_none() {
                        row.error = Some(e);
                    }
                }
            }
            emit(row);
        }
    }
    errors
}

fn worktree_dirty(path: &std::path::Path) -> Result<usize, String> {
    // Read raw porcelain only for counting: the display-oriented runner replaces NULs.
    let output = git::run_with_timeout(
        git::git_command_for(
            path,
            &["status", "--porcelain=v1", "-z", "--untracked-files=normal"],
        ),
        git::GIT_TIMEOUT,
    )?;
    if !output.status.success() {
        return Err(git::strip_control_sequences(
            &String::from_utf8_lossy(&output.stderr),
            false,
        ));
    }
    Ok(porcelain_count(&String::from_utf8_lossy(&output.stdout)))
}

fn porcelain_count(status: &str) -> usize {
    let mut entries = status.split('\0').filter(|s| !s.is_empty());
    let mut count = 0;
    while let Some(entry) = entries.next() {
        count += 1;
        if entry
            .as_bytes()
            .first()
            .is_some_and(|b| *b == b'R' || *b == b'C')
            || entry
                .as_bytes()
                .get(1)
                .is_some_and(|b| *b == b'R' || *b == b'C')
        {
            entries.next();
        }
    }
    count
}

impl App {
    pub fn workspace_note(&self, path: &std::path::Path) -> config::WorktreeNote {
        self.prefs
            .worktree_notes
            .get(path.to_string_lossy().as_ref())
            .cloned()
            .unwrap_or_default()
    }

    pub fn filtered_workspace(&self) -> Vec<usize> {
        let query = self.workspace.filter.to_lowercase();
        self.workspace
            .rows
            .iter()
            .enumerate()
            .filter_map(|(i, row)| {
                let note = self.workspace_note(&row.path);
                if self.workspace.favorites_only && !note.favorite {
                    return None;
                }
                let haystack = format!(
                    "{} {} {} {}",
                    row.parent,
                    row.path.display(),
                    row.branch,
                    note.note
                )
                .to_lowercase();
                haystack.contains(&query).then_some(i)
            })
            .collect()
    }

    pub fn selected_workspace_row(&self) -> Option<&WorkspaceRow> {
        self.filtered_workspace()
            .get(self.workspace.selected)
            .and_then(|&i| self.workspace.rows.get(i))
    }

    pub(super) fn reload_workspace(&mut self) {
        let seq = self.workspace_generation.fetch_add(1, Ordering::Relaxed) + 1;
        self.workspace.loading = true;
        self.workspace.seen.clear();
        self.workspace.errors.clear();
        self.workspace.error_selected = 0;
        self.workspace.skipped_ssh = self
            .repos
            .iter()
            .filter(|r| git::parse_ssh_repo(&r.path).is_some())
            .count();
        let repos = self
            .repos
            .iter()
            .filter(|r| git::parse_ssh_repo(&r.path).is_none())
            .map(|r| (r.name.clone(), r.path.clone()))
            .collect();
        self.send_job(Job::LoadWorkspace {
            seq,
            generation: self.workspace_generation.clone(),
            repos,
        });
    }

    pub(super) fn apply_workspace_row(&mut self, seq: u64, row: WorkspaceRow) {
        if seq != self.workspace_generation.load(Ordering::Relaxed) {
            return;
        }
        let selected = self.selected_workspace_row().map(|r| r.path.clone());
        self.workspace.seen.insert(row.path.clone());
        if let Some(i) = self.workspace.rows.iter().position(|r| r.path == row.path) {
            self.workspace.rows[i] = row;
        } else {
            self.workspace.rows.push(row);
        }
        self.restore_workspace_selection(selected);
    }

    fn restore_workspace_selection(&mut self, path: Option<PathBuf>) {
        let indices = self.filtered_workspace();
        self.workspace.selected = path
            .and_then(|p| {
                indices
                    .iter()
                    .position(|&i| self.workspace.rows[i].path == p)
            })
            .unwrap_or_else(|| clamp_index(self.workspace.selected, indices.len()));
    }

    pub(super) fn finish_workspace(&mut self, seq: u64, errors: Vec<String>) {
        if seq != self.workspace_generation.load(Ordering::Relaxed) {
            return;
        }
        let selected = self.selected_workspace_row().map(|r| r.path.clone());
        self.workspace
            .rows
            .retain(|r| self.workspace.seen.contains(&r.path));
        self.workspace.errors = errors;
        self.workspace.loading = false;
        self.restore_workspace_selection(selected);
    }

    pub(super) fn finish_worktree_note(&mut self, note: String) {
        if let Some(path) = self.pending_worktree_note.take() {
            self.prefs.worktree_notes.entry(path).or_default().note = note;
            self.persist_prefs();
            self.workspace.selected =
                clamp_index(self.workspace.selected, self.filtered_workspace().len());
        }
    }

    pub(super) fn handle_workspace(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                self.screen = Screen::Home;
            }
            KeyCode::Char('r') => self.reload_workspace(),
            KeyCode::Char('[') => {
                self.workspace.error_selected = move_index(
                    self.workspace.error_selected,
                    self.workspace.errors.len(),
                    -1,
                );
            }
            KeyCode::Char(']') => {
                self.workspace.error_selected = move_index(
                    self.workspace.error_selected,
                    self.workspace.errors.len(),
                    1,
                );
            }
            KeyCode::Down | KeyCode::Char('j') => self.move_workspace(1),
            KeyCode::Up | KeyCode::Char('k') => self.move_workspace(-1),
            KeyCode::Char('g') => self.workspace.selected = 0,
            KeyCode::Char('G') => {
                self.workspace.selected = self.filtered_workspace().len().saturating_sub(1)
            }
            KeyCode::Char('/') => {
                self.input = Some(InputKind::WorkspaceQuery);
                self.input_buf = self.workspace.filter.clone();
            }
            KeyCode::Char('f') => {
                self.workspace.favorites_only = !self.workspace.favorites_only;
                self.workspace.selected = 0;
            }
            KeyCode::Char('*') => {
                if let Some(path) = self
                    .selected_workspace_row()
                    .map(|r| r.path.to_string_lossy().into_owned())
                {
                    let note = self.prefs.worktree_notes.entry(path).or_default();
                    note.favorite = !note.favorite;
                    self.persist_prefs();
                    self.workspace.selected =
                        clamp_index(self.workspace.selected, self.filtered_workspace().len());
                }
            }
            KeyCode::Char('m') => {
                if let Some(path) = self.selected_workspace_row().map(|r| r.path.clone()) {
                    self.input_buf = self.workspace_note(&path).note;
                    self.pending_worktree_note = Some(path.to_string_lossy().into_owned());
                    self.input = Some(InputKind::WorktreeNote);
                }
            }
            KeyCode::Enter => {
                if let Some(path) = self.selected_workspace_row().map(|r| r.path.clone()) {
                    self.open_tool_menu(path);
                }
            }
            KeyCode::Char('t') => self.open_terminal_for_current_repo(),
            _ => {}
        }
    }

    pub fn move_workspace(&mut self, delta: isize) {
        self.workspace.selected = move_index(
            self.workspace.selected,
            self.filtered_workspace().len(),
            delta,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicBool;

    fn row(path: &str) -> WorkspaceRow {
        WorkspaceRow {
            parent: "repo".into(),
            path: path.into(),
            branch: "topic".into(),
            dirty: Some(0),
            last_commit: "2026-01-01".into(),
            checked_at: 1,
            locked: false,
            prunable: false,
            error: None,
        }
    }

    #[test]
    fn old_messages_do_not_replace_rows_or_clear_loading_and_notes_filter() {
        let mut a = App::new();
        a.screen = Screen::Workspace;
        a.workspace_generation.store(2, Ordering::Relaxed);
        a.workspace.loading = true;
        a.apply_msg_for_test(Msg::WorkspaceRow {
            seq: 1,
            row: row("/old"),
        });
        a.apply_msg_for_test(Msg::WorkspaceDone {
            seq: 1,
            errors: vec![],
        });
        assert!(a.workspace.rows.is_empty());
        assert!(a.workspace.loading);
        a.apply_msg_for_test(Msg::WorkspaceRow {
            seq: 2,
            row: row("/first"),
        });
        a.apply_msg_for_test(Msg::WorkspaceRow {
            seq: 2,
            row: row("/second"),
        });
        a.handle_key(KeyEvent::from(KeyCode::Char('j')));
        assert_eq!(
            a.selected_workspace_row().unwrap().path,
            PathBuf::from("/second")
        );
        a.apply_msg_for_test(Msg::WorkspaceRow {
            seq: 2,
            row: row("/first"),
        });
        assert_eq!(a.workspace.rows.len(), 2);
        a.prefs.worktree_notes.insert(
            "/second".into(),
            config::WorktreeNote {
                note: "review tomorrow".into(),
                favorite: true,
            },
        );
        a.workspace.filter = "tomorrow".into();
        a.workspace.selected = 0;
        assert_eq!(a.filtered_workspace(), vec![1]);
        a.workspace.favorites_only = true;
        assert_eq!(a.filtered_workspace(), vec![1]);
        a.apply_msg_for_test(Msg::WorkspaceDone {
            seq: 2,
            errors: vec![],
        });
        assert!(!a.workspace.loading);
        assert_eq!(a.current_work_path(), Some(PathBuf::from("/second")));
    }

    #[test]
    fn local_worktrees_are_deduplicated_and_report_real_dirty_state() {
        let root = std::env::temp_dir().join(format!("gdt-workspace-{}", std::process::id()));
        let repo = root.join("parent repo");
        let linked = root.join("review worktree");
        std::fs::create_dir_all(&repo).unwrap();
        git::run_git_cmd(&repo, &["init", "-q"]).unwrap();
        git::run_git_cmd(
            &repo,
            &[
                "-c",
                "user.name=Sample",
                "-c",
                "user.email=sample@example.com",
                "-c",
                "commit.gpgsign=false",
                "commit",
                "--allow-empty",
                "-qm",
                "initial",
            ],
        )
        .unwrap();
        git::run_git_cmd(
            &repo,
            &["worktree", "add", "-qb", "review", linked.to_str().unwrap()],
        )
        .unwrap();
        std::fs::write(linked.join("draft"), "work").unwrap();
        #[cfg(unix)]
        std::fs::write(linked.join("line\nbreak"), "work").unwrap();
        let repos = vec![
            ("parent".into(), repo.clone()),
            ("also registered".into(), linked.clone()),
        ];
        let mut rows = Vec::new();
        assert!(collect_workspace(&repos, || false, |r| rows.push(r)).is_empty());
        assert_eq!(rows.len(), 2);
        let r = rows
            .iter()
            .find(|r| r.path == linked.canonicalize().unwrap())
            .unwrap();
        assert_eq!(r.dirty, Some(if cfg!(unix) { 2 } else { 1 }));
        assert!(!r.last_commit.is_empty());
        assert!(r.error.is_none());
        let cancelled = AtomicBool::new(false);
        let mut count = 0;
        collect_workspace(
            &repos,
            || cancelled.load(Ordering::Relaxed),
            |_| {
                count += 1;
                cancelled.store(true, Ordering::Relaxed);
            },
        );
        assert_eq!(count, 1);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn nul_porcelain_counts_renames_once_and_newline_names_once() {
        assert_eq!(
            porcelain_count("R  new\0old\0?? line\nbreak\0 M changed\0"),
            3
        );
        assert_eq!(porcelain_count(""), 0);
    }
}
