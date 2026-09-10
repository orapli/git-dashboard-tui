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
        if self.confirm.is_some()
            || self.input.is_some()
            || self.tool_menu.is_some()
            || self.nav_popup
        {
            return;
        }
        match self.screen {
            Screen::Workspace => self.move_workspace(delta.signum()),
            Screen::Home => {
                let len = self.filtered_home().len();
                if len > 0 {
                    self.home_selected = move_index(self.home_selected, len, delta.signum());
                }
            }
            Screen::Repo => {
                let len = self.visible_indices().len();
                if len > 0 {
                    self.list_selected = move_index(self.list_selected, len, delta.signum());
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
            Screen::GlobalMembers => match self.global_member_pane {
                FocusPane::List => {
                    let len = self.filtered_global_members().len();
                    if len > 0 {
                        self.global_member_selected =
                            move_index(self.global_member_selected, len, delta.signum());
                        self.global_member_repo_selected = 0;
                    }
                }
                _ => {
                    let len = self
                        .filtered_global_members()
                        .get(self.global_member_selected)
                        .and_then(|&i| self.global_members.get(i))
                        .map_or(0, |m| m.contributions.len());
                    if len > 0 {
                        self.global_member_repo_selected =
                            move_index(self.global_member_repo_selected, len, delta.signum());
                    }
                }
            },
            Screen::RepoFinder => {
                let len = self.filtered_finder_repos().len();
                if len > 0
                    && let Some(ref mut finder) = self.repo_finder
                {
                    finder.selected_idx = move_index(finder.selected_idx, len, delta.signum());
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
        if self.confirm.is_some() || self.input.is_some() || self.tool_menu.is_some() {
            return;
        }

        if self.nav_popup {
            let rect = self.nav_popup_rect.get();
            let inner_x = rect.x.saturating_add(1);
            let inner_width = rect.width.saturating_sub(2);
            if col >= inner_x
                && col < inner_x.saturating_add(inner_width)
                && row > rect.y
                && row < rect.y.saturating_add(rect.height).saturating_sub(1)
            {
                let first = rect.y.saturating_add(1);
                if row >= first && row < first.saturating_add(4) {
                    self.nav_popup_selected = (row - first) as usize;
                    self.execute_navigation(self.navigation_options()[self.nav_popup_selected]);
                }
            } else {
                self.nav_popup = false;
            }
            return;
        }

        if row == 0 {
            let bounds = self.nav_button_bounds.borrow().clone();
            if let Some((_, _, action)) = bounds.iter().find(|(x1, x2, _)| col >= *x1 && col < *x2)
            {
                self.execute_navigation(*action);
                self.nav_popup_selected = 0;
                return;
            }
        }

        match self.screen {
            Screen::Workspace => {
                if let Some(i) = self.workspace_viewport.get().index_at(row)
                    && i < self.filtered_workspace().len()
                {
                    if i == self.workspace.selected {
                        if let Some(path) = self.selected_workspace_row().map(|r| r.path.clone()) {
                            self.open_tool_menu(path);
                        }
                    } else {
                        self.workspace.selected = i;
                    }
                }
            }
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
                if row == self.settings_tab_row.get()
                    && let Some((idx, _)) = self
                        .settings_tab_bounds
                        .borrow()
                        .iter()
                        .enumerate()
                        .find(|(_, (x1, x2))| col >= *x1 && col < *x2)
                {
                    self.settings_tab = if idx == 0 {
                        SettingsTab::Repositories
                    } else {
                        SettingsTab::Members
                    };
                    return;
                }
                let vp = self.settings_viewport.get();
                if let Some(idx) = vp.index_at_position(col, row) {
                    match self.settings_tab {
                        SettingsTab::Repositories if idx < self.repos.len() => {
                            self.settings_selected = idx;
                        }
                        SettingsTab::Members if idx < self.members.len() => {
                            self.settings_member_selected = idx;
                        }
                        _ => {}
                    }
                }
            }
            Screen::RepoFinder => {
                let Some(vp_idx) = self.finder_viewport.get().index_at_position(col, row) else {
                    return;
                };
                let vis = self.filtered_finder_repos();
                let Some(&raw_idx) = vis.get(vp_idx) else {
                    return;
                };
                let vp = self.finder_viewport.get();
                if let Some(finder) = self.repo_finder.as_mut() {
                    if finder.selected_idx != vp_idx {
                        finder.selected_idx = vp_idx;
                    }
                    if col >= vp.x.saturating_add(2)
                        && col < vp.x.saturating_add(5)
                        && !finder.repos[raw_idx].is_already_added
                    {
                        finder.repos[raw_idx].is_selected = !finder.repos[raw_idx].is_selected;
                    }
                }
            }
            Screen::GlobalMembers => {
                let vis = self.filtered_global_members();
                if let Some(idx) = self
                    .global_members_viewport
                    .get()
                    .index_at_position(col, row)
                    && idx < vis.len()
                {
                    self.global_member_pane = FocusPane::List;
                    if self.global_member_selected != idx {
                        self.global_member_selected = idx;
                        self.global_member_repo_selected = 0;
                    }
                    return;
                }
                let Some(repo_idx) = self
                    .global_member_repos_viewport
                    .get()
                    .index_at_position(col, row)
                else {
                    return;
                };
                let Some(member_idx) = vis.get(self.global_member_selected).copied() else {
                    return;
                };
                let Some(member) = self.global_members.get(member_idx) else {
                    return;
                };
                if repo_idx >= member.contributions.len() {
                    return;
                }
                if self.global_member_pane == FocusPane::Content
                    && self.global_member_repo_selected == repo_idx
                {
                    self.open_selected_global_member_repo();
                } else {
                    self.global_member_repo_selected = repo_idx;
                    self.global_member_pane = FocusPane::Content;
                }
            }
            Screen::CommitSearch => {
                let Some(idx) = self
                    .commit_search_viewport
                    .get()
                    .index_at_position(col, row)
                else {
                    return;
                };
                let Some(search) = self.commit_search.as_mut() else {
                    return;
                };
                if idx >= search.hits.len() {
                    return;
                }
                if search.selected == idx {
                    self.jump_to_search_hit();
                } else {
                    search.selected = idx;
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
