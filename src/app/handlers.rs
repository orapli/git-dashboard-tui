use super::App;
use super::helpers::{move_index, sync_hunk_from_scroll};
use super::types::*;
use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};

impl App {
    pub fn handle_mouse(&mut self, mouse: MouseEvent) {
        match mouse.kind {
            MouseEventKind::ScrollDown => {
                self.handle_mouse_scroll(1);
            }
            MouseEventKind::ScrollUp => {
                self.handle_mouse_scroll(-1);
            }
            MouseEventKind::Down(MouseButton::Left) => {
                self.handle_mouse_click(mouse.column, mouse.row);
            }
            _ => {}
        }
    }

    pub fn handle_mouse_scroll(&mut self, delta: isize) {
        match self.screen {
            Screen::Home => {
                let len = self.filtered_home().len();
                if len > 0 {
                    self.home_selected = move_index(self.home_selected, len, delta.signum());
                }
            }
            Screen::Repo => {
                if let Some(ref data) = self.repo_data {
                    let len = match self.repo_tab {
                        RepoTab::Status => data.working_files.len(),
                        RepoTab::Commits => data.commits.len(),
                        RepoTab::Branches => data.branches.len(),
                        RepoTab::Tags => data.tags.len(),
                        RepoTab::Contributors => data.contributors.len(),
                        RepoTab::Stash => data.stashes.len(),
                        RepoTab::Worktrees => data.worktrees.len(),
                    };
                    if len > 0 {
                        self.list_selected = move_index(self.list_selected, len, delta.signum());
                    }
                }
            }
            Screen::Diff => {
                if let Some(ref mut diff) = self.diff {
                    if delta > 0 {
                        diff.scroll = diff.scroll.saturating_add(3);
                    } else {
                        diff.scroll = diff.scroll.saturating_sub(3);
                    }
                    sync_hunk_from_scroll(diff);
                }
            }
            Screen::Settings => {
                if self.settings_tab == SettingsTab::Repositories {
                    let len = self.repos.len();
                    if len > 0 {
                        self.settings_selected =
                            move_index(self.settings_selected, len, delta.signum());
                    }
                } else {
                    let len = self.members.len();
                    if len > 0 {
                        self.settings_member_selected =
                            move_index(self.settings_member_selected, len, delta.signum());
                    }
                }
            }
            Screen::GlobalMembers => {
                let len = self.filtered_global_members().len();
                if len > 0 {
                    self.global_member_selected =
                        move_index(self.global_member_selected, len, delta.signum());
                }
            }
            Screen::RepoFinder => {
                if let Some(ref mut finder) = self.repo_finder {
                    let len = finder.repos.len();
                    if len > 0 {
                        finder.selected_idx = move_index(finder.selected_idx, len, delta.signum());
                    }
                }
            }
            Screen::Log => {
                if let Some(ref mut log) = self.log {
                    if delta > 0 {
                        log.scroll = log.scroll.saturating_add(3);
                    } else {
                        log.scroll = log.scroll.saturating_sub(3);
                    }
                }
            }
            Screen::CommitSearch => {
                if let Some(ref mut search) = self.commit_search {
                    let len = search.hits.len();
                    if len > 0 {
                        search.selected = move_index(search.selected, len, delta.signum());
                    }
                }
            }
            Screen::Help => {
                self.scroll_help_by(delta.signum() * 3);
            }
        }
    }

    /// Width of the `[ ]` comparison marker at the start of a Commits or Tags
    /// row, as `draw_commits`/`draw_tags` render it. Clicking it is the mouse
    /// equivalent of pressing space on that row.
    const MARKER_WIDTH: u16 = 3;
    /// Width of the `▸ ` selection indicator every list reserves, marker or no.
    const HIGHLIGHT_WIDTH: u16 = 2;

    /// Which item in the repository-detail list a click row lands on, or
    /// `None` if the click was outside the list (tab bar, border, blank space
    /// past the last row).
    fn repo_list_index_at(&self, row: u16) -> Option<usize> {
        let idx = self.list_viewport.get().index_at(row)?;
        (idx < self.visible_indices().len()).then_some(idx)
    }

    /// A click on a row of the repository-detail list.
    ///
    /// On Commits and Tags the `[ ]` marker at the start of the row is its own
    /// target: clicking it picks the comparison base/target, exactly as space
    /// does. Clicking anywhere else selects the row, and clicking the row that
    /// is already selected activates it — the same select-then-open rhythm the
    /// Home list already has.
    fn click_repo_item(&mut self, index: usize, col: u16) {
        let vp = self.list_viewport.get();
        let marker_start = vp.x.saturating_add(Self::HIGHLIGHT_WIDTH);
        let on_marker = col >= marker_start && col < marker_start + Self::MARKER_WIDTH;

        if on_marker {
            // Select first: the marker toggle and everything else in this tab
            // read the selection, and leaving it on the previous row after a
            // click is how "I marked the wrong commit" happens.
            self.list_selected = index;
            self.after_list_move();
            if self.toggle_marker_at(index) {
                return;
            }
        }
        if self.list_selected == index {
            self.activate_repo_item();
        } else {
            self.list_selected = index;
            self.after_list_move();
        }
    }

    pub fn handle_mouse_click(&mut self, col: u16, row: u16) {
        if self.confirm.is_some() || self.input.is_some() {
            return;
        }

        match self.screen {
            Screen::Home => {
                // Use the table bounds recorded by rendering: the summary
                // and compact layouts put headers on different rows.
                let (header_y, end_y) = self.home_table_bounds.get();
                if row == header_y {
                    if let Some(col) = self.home_column_at(col) {
                        self.sort_by_column(col);
                    }
                } else if row > header_y && row < end_y {
                    // The table scrolls, so the top visible row is not index 0
                    let row_idx = self.home_offset.get() + (row - header_y - 1) as usize;
                    let indices = self.filtered_home();
                    if row_idx < indices.len() {
                        if self.home_selected == row_idx {
                            // Click on already selected row opens the repo
                            self.open_repo(indices[row_idx]);
                        } else {
                            self.home_selected = row_idx;
                        }
                    }
                }
            }
            Screen::Repo => {
                if let Some(idx) = self.repo_list_index_at(row) {
                    self.click_repo_item(idx, col);
                    return;
                }
                // Tab bar click (usually row 1). Route through switch_tab so the
                // selection and filter are reset: a stale list_selected from a
                // longer tab leaves Enter/d/space silently doing nothing.
                if (row == 1 || row == 2)
                    && let Some(tab) = self.repo_tab_at(col)
                {
                    self.switch_tab(tab);
                }
            }
            Screen::Settings => {
                // Tab click in settings
                if row == 1 || row == 2 {
                    if col < 20 {
                        self.settings_tab = SettingsTab::Repositories;
                    } else {
                        self.settings_tab = SettingsTab::Members;
                    }
                }
            }
            Screen::Help => {
                // Click anywhere on help screen returns
                self.screen = self.help_return.take().unwrap_or(Screen::Home);
                self.help_scroll.set(0);
            }
            _ => {}
        }
    }
}
