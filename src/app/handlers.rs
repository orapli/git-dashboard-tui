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
            Screen::Help => {}
        }
    }

    pub fn handle_mouse_click(&mut self, col: u16, row: u16) {
        if self.confirm.is_some() || self.input.is_some() {
            return;
        }

        match self.screen {
            Screen::Home => {
                // Row 2 is the column header: clicking one sorts by it, and
                // clicking the active one again reverses the direction.
                if row == 2 {
                    if let Some(col) = self.home_column_at(col) {
                        self.sort_by_column(col);
                    }
                } else if row >= 3 {
                    // The table scrolls, so the top visible row is not index 0
                    let row_idx = self.home_offset.get() + (row - 3) as usize;
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
                // Tab bar click (usually row 1). Route through switch_tab so the
                // selection and filter are reset: a stale list_selected from a
                // longer tab leaves Enter/d/space silently doing nothing.
                if row == 1 || row == 2 {
                    let tab = match col {
                        0..=13 => Some(RepoTab::Status),
                        14..=27 => Some(RepoTab::Commits),
                        28..=41 => Some(RepoTab::Branches),
                        42..=53 => Some(RepoTab::Tags),
                        54..=65 => Some(RepoTab::Stash),
                        66..=83 => Some(RepoTab::Contributors),
                        84..=99 => Some(RepoTab::Worktrees),
                        _ => None,
                    };
                    if let Some(tab) = tab {
                        self.switch_tab(tab);
                    }
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
            }
            _ => {}
        }
    }
}
