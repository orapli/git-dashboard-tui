use crate::app::{
    App, CiOutcome, FocusPane, HomeFailure, ListViewport, RepoTab, Screen, classify_ci_status,
    classify_home_error, column_for_sort_mode, dirty_split, short_hash, shorten_path,
    sort_is_ascending,
};
use crate::colors::Palette;
use crate::git::{CommitRef, DiffRowKind, GitOpState, UpstreamState};
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Margin, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, Borders, Cell, Clear, List, ListItem, ListState, Paragraph, Row, Table, TableState,
    Tabs, Wrap,
};

/// Selection marker used by the Home table and every item list. Its width
/// shifts everything on the row right, so click hit-tests must account for it.
const HIGHLIGHT_SYMBOL: &str = "▸ ";

pub fn draw(frame: &mut Frame, app: &App) {
    let pal = Palette::for_name(app.theme_name());
    let area = frame.area();
    frame.render_widget(
        Block::default().style(Style::default().bg(pal.bg).fg(pal.text)),
        area,
    );

    // The hint bar takes a second row only when the hints don't fit on one.
    // A fixed one-row footer silently cut the tail off every screen — Home's
    // hints need 154 columns in English, so on a normal terminal the last
    // third of them, `o sort` and `q quit` among them, simply did not exist as
    // far as the user could tell.
    let hint_rows = footer_hint_rows(app, area.width);
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(hint_rows + 1),
        ])
        .split(area);

    draw_title(frame, app, chunks[0], pal);
    match app.screen {
        Screen::Home => draw_home(frame, app, chunks[1], pal),
        Screen::Workspace => draw_workspace(frame, app, chunks[1], pal),
        Screen::Repo => draw_repo(frame, app, chunks[1], pal),
        Screen::Diff => draw_diff(frame, app, chunks[1], pal),
        Screen::Settings => draw_settings(frame, app, chunks[1], pal),
        Screen::GlobalMembers => draw_global_members(frame, app, chunks[1], pal),
        Screen::RepoFinder => draw_repo_finder(frame, app, chunks[1], pal),
        Screen::CommitSearch => draw_commit_search(frame, app, chunks[1], pal),
        Screen::Help => draw_help(frame, app, chunks[1], pal),
        Screen::Log => draw_log(frame, app, chunks[1], pal),
    }
    draw_footer(frame, app, chunks[2], pal);

    if app.tool_menu.is_some() {
        let rect = centered_rect(area, 30, 90, 12);
        frame.render_widget(Clear, rect);
        frame.render_widget(
            Paragraph::new(
                app.tool_menu_lines()
                    .into_iter()
                    .map(Line::from)
                    .collect::<Vec<_>>(),
            )
            .block(Block::bordered().title(app.tt("Open in tool", "ツールで開く")))
            .style(Style::default().bg(pal.surface).fg(pal.text)),
            rect,
        );
    }
    if app.nav_popup {
        let rect = navigation_popup_rect(area);
        app.nav_popup_rect.set(rect);
        frame.render_widget(Clear, rect);
        let labels = [
            app.tt("Settings", "設定"),
            app.tt("Worktrees", "Worktree"),
            app.tt("Global Members", "横断メンバー"),
            app.tt("Commit Search", "コミット検索"),
        ];
        let lines = labels
            .into_iter()
            .enumerate()
            .map(|(i, label)| {
                Line::from(vec![
                    Span::raw(if i == app.nav_popup_selected {
                        "▸ "
                    } else {
                        "  "
                    }),
                    Span::raw(label),
                ])
            })
            .collect::<Vec<_>>();
        frame.render_widget(
            Paragraph::new(lines)
                .block(Block::bordered().title(app.tt("Navigate", "移動")))
                .style(Style::default().bg(pal.surface).fg(pal.text)),
            rect,
        );
    }
    if app.is_adding_repo() {
        draw_prompt(frame, area, &app.prompt_title(), app.input_buf(), pal);
    }
    if let Some(msg) = app.confirm_message() {
        draw_prompt(frame, area, &msg, "", pal);
    }
}

fn draw_title(frame: &mut Frame, app: &App, area: Rect, pal: Palette) {
    app.nav_button_bounds.borrow_mut().clear();
    let mut x = area.x;
    let buttons = [
        (
            app.tt("[Back]", "[戻る]"),
            crate::app::NavigationAction::Back,
        ),
        (
            app.tt("[Home]", "[Home]"),
            crate::app::NavigationAction::Home,
        ),
        (
            app.tt("[Navigate]", "[移動]"),
            crate::app::NavigationAction::Move,
        ),
    ];
    let mut spans = Vec::new();
    for (label, action) in buttons {
        use unicode_width::UnicodeWidthStr;
        let width = label.width() as u16;
        if x.saturating_add(width) > area.right() {
            continue;
        }
        app.nav_button_bounds
            .borrow_mut()
            .push((x, x.saturating_add(width), action));
        spans.push(Span::styled(
            label,
            Style::default().fg(pal.bg).bg(pal.accent),
        ));
        spans.push(Span::raw(" "));
        x = x.saturating_add(width + 1);
    }
    let title = match app.screen {
        Screen::Home => app.tt("Repositories", "リポジトリ一覧"),
        Screen::Workspace => app.tt("Local worktrees", "ローカルWorktree一覧"),
        Screen::Repo => app
            .repo_index
            .and_then(|i| app.repos.get(i))
            .map(|r| r.name.clone())
            .unwrap_or_else(|| app.t("dashboard")),
        Screen::Diff => app
            .diff
            .as_ref()
            .map(|d| d.title.clone())
            .unwrap_or_else(|| app.t("diff")),
        Screen::Settings => app.t("settings"),
        Screen::GlobalMembers => app.tt("Global Members", "全リポジトリ横断メンバー"),
        Screen::RepoFinder => app.tt("Repository Finder", "リポジトリ検出・一括登録"),
        Screen::CommitSearch => app.tt("Commit Search", "コミット検索"),
        Screen::Help => app.tt("Help", "ヘルプ"),
        Screen::Log => app
            .log
            .as_ref()
            .map(|l| l.title.clone())
            .unwrap_or_else(|| app.tt("Log", "ログ")),
    };
    spans.extend([
        Span::styled(
            format!(" {title} "),
            Style::default()
                .fg(pal.bg)
                .bg(pal.accent)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("  git-dashboard-tui", Style::default().fg(pal.muted)),
    ]);
    // Work started from Home keeps running while the user moves elsewhere, so
    // the "still working" indicator lives in the title bar, which every screen
    // has, rather than on the screen that started it.
    let busy = app.busy_count();
    if busy > 0 {
        spans.push(Span::styled(
            format!("   {} {}", app.spinner(), app.busy_label(busy)),
            Style::default().fg(pal.yellow).add_modifier(Modifier::BOLD),
        ));
    }
    let bar = Paragraph::new(Line::from(spans));
    frame.render_widget(bar, area);
}

pub fn navigation_popup_rect(area: Rect) -> Rect {
    let width = 34.min(area.width.saturating_sub(2));
    let height = 6.min(area.height.saturating_sub(2));
    Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    }
}

/// How many rows the hint bar needs: one, or two when the hints overflow.
///
/// Capped at two — past that the footer would eat the screen. Anything still
/// not fitting is marked with an ellipsis so the user knows to look in `?`.
fn footer_hint_rows(app: &App, width: u16) -> u16 {
    wrap_hints(&app.footer_hints(), width, MAX_HINT_ROWS).len() as u16
}

/// Greedily pack `key desc` hints into at most `max_rows` rows of `width`
/// columns, measuring display width so CJK labels are not counted as half
/// their size.
///
/// Packed twice when hints don't all fit: the first pass finds out that some
/// were dropped, the second re-packs with room held back on the final row for
/// the `… ?` marker. Otherwise the marker is itself truncated away, which is
/// precisely the failure it exists to prevent.
fn wrap_hints(hints: &[(String, String)], width: u16, max_rows: usize) -> Vec<Vec<usize>> {
    let rows = pack_hints(hints, width, max_rows, 0);
    let shown: usize = rows.iter().map(|r| r.len()).sum();
    if shown == hints.len() {
        return rows;
    }
    pack_hints(hints, width, max_rows, MARKER_WIDTH)
}

/// Columns held back on the last row for `  … ?`.
const MARKER_WIDTH: usize = 5;

fn pack_hints(
    hints: &[(String, String)],
    width: u16,
    max_rows: usize,
    last_row_reserve: usize,
) -> Vec<Vec<usize>> {
    use unicode_width::UnicodeWidthStr;
    let width = width as usize;
    let mut rows: Vec<Vec<usize>> = vec![Vec::new()];
    let mut used = 0usize;
    for (i, (key, desc)) in hints.iter().enumerate() {
        let item = if desc.is_empty() {
            key.width()
        } else {
            key.width() + 1 + desc.width()
        };
        let empty = rows.last().is_some_and(|r| r.is_empty());
        let sep = if empty { 0 } else { 2 };
        let reserve = if rows.len() == max_rows {
            last_row_reserve
        } else {
            0
        };
        if used + sep + item + reserve > width && !empty {
            if rows.len() == max_rows {
                break;
            }
            rows.push(Vec::new());
            used = 0;
        }
        let sep = if rows.last().is_some_and(|r| r.is_empty()) {
            0
        } else {
            2
        };
        used += sep + item;
        rows.last_mut().expect("always at least one row").push(i);
    }
    rows
}

/// Two rows fit Home's hints down to a 71-column terminal in Japanese and 77
/// in English; below that the ellipsis takes over rather than the footer
/// growing without bound.
const MAX_HINT_ROWS: usize = 2;

fn draw_footer(frame: &mut Frame, app: &App, area: Rect, pal: Palette) {
    let hints = app.footer_hints();
    let wrapped = wrap_hints(&hints, area.width, MAX_HINT_ROWS);
    let shown: usize = wrapped.iter().map(|r| r.len()).sum();
    let mut constraints = vec![Constraint::Length(1); wrapped.len()];
    constraints.push(Constraint::Length(1));
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(area);
    for (n, row) in wrapped.iter().enumerate() {
        let items: Vec<(String, String)> = row.iter().map(|&i| hints[i].clone()).collect();
        // Only the last row can be short, so that is where the "there is more,
        // press ?" marker belongs.
        let elided = n + 1 == wrapped.len() && shown < hints.len();
        frame.render_widget(
            Paragraph::new(shortcut_line(&items, pal, elided))
                .style(Style::default().bg(pal.surface)),
            rows[n],
        );
    }
    let (msg, color) = if let Some(e) = app.error.as_deref() {
        (e, pal.red)
    } else if !app.status.is_empty() {
        (app.status.as_str(), pal.yellow)
    } else if app.is_filtering() {
        (app.input_buf(), pal.accent)
    } else {
        ("", pal.muted)
    };
    // A status line reading "Fetching..." looks the same whether the fetch is
    // running or wedged. The spinner is the difference.
    let msg = if app.is_busy() && !msg.is_empty() {
        format!("{} {msg}", app.spinner())
    } else {
        msg.to_string()
    };
    frame.render_widget(
        Paragraph::new(msg).style(Style::default().fg(color).bg(pal.surface)),
        rows[wrapped.len()],
    );
}

fn draw_workspace(frame: &mut Frame, app: &App, area: Rect, pal: Palette) {
    // Row failures belong in the same inspector as whole-repository ones:
    // a row that could not be read is the only place its reason exists.
    let issues = app.workspace_issues();
    let parts = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(3),
        Constraint::Length(4),
        Constraint::Length(if issues.is_empty() { 0 } else { 4 }),
    ])
    .split(area);
    let indices = app.filtered_workspace();
    let state = if app.workspace.loading {
        app.tt(
            "loading; existing rows may be cached",
            "取得中・既存行は前回値の場合があります",
        )
    } else {
        app.tt("ready", "取得完了")
    };
    // "ready, errors: 0" while rows read "?" told the user everything was
    // fine when nothing had been learned about those rows at all.
    let unknown = app.workspace_unknown();
    let mut header = vec![Span::raw(format!(
        "{} / {}  {}  SSH: {}  {}: {}  ",
        indices.len(),
        app.workspace.rows.len(),
        state,
        app.workspace.skipped_ssh,
        app.tt("errors", "取得失敗"),
        app.workspace.errors.len(),
    ))];
    header.push(Span::styled(
        format!("{}: {unknown}", app.tt("unknown", "状態不明")),
        Style::default().fg(if unknown == 0 {
            pal.subtext
        } else {
            pal.yellow
        }),
    ));
    header.push(Span::raw(format!("  / {}", app.workspace.filter)));
    frame.render_widget(Paragraph::new(Line::from(header)), parts[0]);
    let rows = indices.iter().map(|&i| {
        let r = &app.workspace.rows[i];
        let note = app.workspace_note(&r.path);
        Row::new(vec![
            if note.favorite {
                "★".into()
            } else {
                String::new()
            },
            r.parent.clone(),
            r.branch.clone(),
            match r.dirty {
                Some(n) => n.to_string(),
                // A bare "?" read as "clean, probably". Say which it is: the
                // collector always fills in either a count or a reason.
                None if r.error.is_some() => app.tt("unknown", "状態不明"),
                None => "…".into(),
            },
            if r.last_commit.is_empty() && r.error.is_some() {
                "—".into()
            } else {
                r.last_commit.chars().take(16).collect::<String>()
            },
            r.path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| r.path.display().to_string()),
            note.note,
        ])
        .style(Style::default().fg(if r.error.is_some() {
            pal.yellow
        } else {
            pal.text
        }))
    });
    let table = Table::new(
        rows,
        [
            Constraint::Length(2),
            Constraint::Percentage(15),
            Constraint::Percentage(17),
            Constraint::Length(10),
            Constraint::Length(17),
            Constraint::Percentage(25),
            Constraint::Min(8),
        ],
    )
    .header(
        Row::new(vec![
            "★".into(),
            app.tt("Repository", "親リポジトリ"),
            app.tt("Branch", "ブランチ"),
            app.tt("Dirty", "未コミット"),
            app.tt("Last commit", "最終コミット"),
            "Worktree".into(),
            app.tt("Note", "用途メモ"),
        ])
        .style(Style::default().fg(pal.accent)),
    )
    .block(Block::bordered().title(if app.workspace.favorites_only {
        app.tt("Favorites", "お気に入り")
    } else {
        app.tt("Worktrees (local only)", "Worktree（ローカルのみ）")
    }))
    .row_highlight_style(
        Style::default()
            .bg(pal.overlay)
            .add_modifier(Modifier::BOLD),
    )
    .highlight_symbol(HIGHLIGHT_SYMBOL);
    let mut selection = TableState::default().with_selected(if indices.is_empty() {
        None
    } else {
        Some(app.workspace.selected)
    });
    frame.render_stateful_widget(table, parts[1], &mut selection);
    app.workspace_viewport.set(ListViewport {
        y: parts[1].y.saturating_add(2),
        height: parts[1].height.saturating_sub(3),
        x: parts[1].x + 1,
        width: parts[1].width.saturating_sub(2),
        offset: selection.offset(),
    });
    let detail = if let Some(row) = app.selected_workspace_row() {
        format!(
            "{}\n{}{}{}\n{}",
            row.path.display(),
            if row.locked { "locked  " } else { "" },
            if row.prunable { "prunable  " } else { "" },
            app.workspace_note(&row.path).note,
            row.error.as_ref().map_or_else(
                || {
                    format!(
                        "{}: {}",
                        app.tt("Checked", "取得"),
                        crate::git::format_timestamp(row.checked_at)
                    )
                },
                |e| format!("{}: {e}", app.tt("Unknown state", "状態不明")),
            )
        )
    } else {
        app.tt(
            "No matching local worktrees. /: search  f: toggle favorites",
            "一致するローカルWorktreeなし。/: 検索  f: お気に入り切替",
        )
    };
    frame.render_widget(
        Paragraph::new(detail).style(Style::default().fg(pal.subtext)),
        parts[2],
    );
    // Clamp rather than index: rows stream in, so the selection can outlive
    // the list it was made against.
    let selected_issue = app
        .workspace
        .error_selected
        .min(issues.len().saturating_sub(1));
    if let Some(error) = issues.get(selected_issue) {
        let title = format!(
            "{} {}/{}  [/] {}",
            app.tt("Errors", "取得失敗"),
            selected_issue + 1,
            issues.len(),
            app.tt("previous/next", "前/次")
        );
        frame.render_widget(
            Paragraph::new(error.as_str())
                .block(Block::default().borders(Borders::TOP).title(title))
                .wrap(Wrap { trim: false })
                .style(Style::default().fg(pal.red)),
            parts[3],
        );
    }
}

fn draw_home(frame: &mut Frame, app: &App, area: Rect, pal: Palette) {
    app.home_table_bounds.set((0, 0));
    if app.repos.is_empty() {
        let body = vec![
            Line::from(""),
            Line::from(Span::styled(
                app.tt("No repositories registered.", "リポジトリがありません。"),
                Style::default().fg(pal.text).add_modifier(Modifier::BOLD),
            )),
            Line::from(Span::styled(
                app.tt(
                    "A  Add repositories from a folder (e.g. ~/work)",
                    "A  作業フォルダからまとめて追加 (例: ~/work)",
                ),
                Style::default().fg(pal.accent),
            )),
            Line::from(Span::styled(
                app.tt("a  Add a single repository", "a  リポジトリを1件だけ追加"),
                Style::default().fg(pal.subtext),
            )),
            Line::from(app.tt(
                "Choose a folder, select repositories, then press Enter to register.",
                "フォルダを指定し、検出したリポジトリを選んで Enter で登録します。",
            )),
        ];
        frame.render_widget(
            Paragraph::new(body).wrap(Wrap { trim: false }).block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(pal.border))
                    .title(app.t("repositories")),
            ),
            area,
        );
        return;
    }

    let area = if app.onboarding_visible && area.height >= 12 && area.width >= 54 {
        let parts = Layout::vertical([Constraint::Min(6), Constraint::Length(4)]).split(area);
        frame.render_widget(
            Paragraph::new(vec![
                Line::from(app.tt(
                    "n  Needs attention    Enter  Details",
                    "n  要対応だけ見る    Enter  詳細",
                )),
                Line::from(app.tt(
                    "t  Open shell    ?  Help    Esc  Dismiss guide",
                    "t  シェルで作業    ?  ヘルプ    Esc  案内を閉じる",
                )),
            ])
            .wrap(Wrap { trim: false })
            .block(Block::bordered().title(app.tt("Start here", "まずはこの4つから")))
            .style(Style::default().fg(pal.accent)),
            parts[1],
        );
        parts[0]
    } else {
        area
    };

    let area = if area.height >= 15 && area.width >= 72 {
        let parts = Layout::vertical([
            Constraint::Length(1),
            Constraint::Min(5),
            // Six lines inside the border: the attention reason, the full
            // path, the two freshness stamps, the key hints, CI and PR.
            Constraint::Length(8),
        ])
        .split(area);
        let counts = app.home_counts();
        // `Dirty` counts only tracked changes now that the rows separate the
        // two, so the header and the column mean the same thing by the word.
        let mut summary = vec![Span::styled(
            format!(
                "{} {}  {} {}  {} {}  {} {}  {} {}",
                app.tt("Attention", "要対応"),
                counts.attention,
                app.tt("Dirty", "未コミット"),
                counts.dirty,
                app.tt("Untracked", "未追跡"),
                counts.untracked,
                app.tt("Sync delta", "同期差分"),
                counts.sync,
                app.tt("Unverified", "未確認"),
                counts.unknown
            ),
            Style::default().fg(pal.accent),
        )];
        if counts.failed > 0 {
            summary.push(Span::styled(
                format!("  {} {}", app.tt("Unreadable", "読取不可"), counts.failed),
                Style::default().fg(pal.red).add_modifier(Modifier::BOLD),
            ));
        }
        frame.render_widget(Paragraph::new(Line::from(summary)), parts[0]);
        frame.render_widget(
            Paragraph::new(
                app.home_context()
                    .into_iter()
                    .map(Line::from)
                    .collect::<Vec<_>>(),
            )
            .block(Block::bordered().title(app.tt("Selected repository", "選択リポジトリ")))
            .style(Style::default().fg(pal.subtext)),
            parts[2],
        );
        parts[1]
    } else {
        area
    };
    app.home_table_bounds
        .set((area.y.saturating_add(1), area.bottom().saturating_sub(1)));

    let indices = app.filtered_home();
    // The sorted column carries the direction marker, so the current sort is
    // visible in the table itself rather than only in the title.
    let sorted_col = column_for_sort_mode(app.sort_mode());
    let marker = if sort_is_ascending(app.sort_mode()) {
        "▲"
    } else {
        "▼"
    };
    let labels = [
        app.tt("Name", "名前"),
        app.tt("Branch", "ブランチ"),
        app.tt("Sync", "同期"),
        app.tt("Dirty", "未コミット"),
        app.tt("Last commit", "最終コミット"),
        app.tt("Path", "パス"),
    ];
    let header_cells: Vec<Cell> = labels
        .clone()
        .into_iter()
        .enumerate()
        .map(|(i, label)| {
            if Some(i) == sorted_col {
                Cell::from(Line::from(vec![Span::styled(
                    format!("{label}{marker}"),
                    Style::default().fg(pal.accent).add_modifier(Modifier::BOLD),
                )]))
            } else {
                Cell::from(label)
            }
        })
        .collect();
    let header = Row::new(header_cells).style(
        Style::default()
            .fg(pal.subtext)
            .add_modifier(Modifier::BOLD),
    );

    // Fixed widths were picked for the English headers, so `未コミット` (10
    // columns) was silently cut to `未コミッ` in an 8-column cell. Take the
    // wider of the two, plus a column for the sort marker, so a header can
    // never be truncated by its own column — in any language, including one
    // added later.
    let min_w = |i: usize, fixed: u16| {
        use unicode_width::UnicodeWidthStr;
        Constraint::Length(fixed.max(labels[i].width() as u16 + 1))
    };
    let widths = [
        min_w(0, 26),
        min_w(1, 30),
        min_w(2, 10),
        min_w(3, 8),
        min_w(4, 18),
        Constraint::Min(10),
    ];
    // Record where each column actually landed so click-to-sort hit-tests
    // against the real layout instead of a second copy of this arithmetic.
    // `Table` lays its columns out inside the block, after the highlight
    // symbol, with `column_spacing` between them. The path column's real
    // width also decides how much of each path survives shortening, so this
    // has to be known before the rows are built.
    let path_width = {
        let inner = area.inner(Margin::new(1, 1));
        let sym_w = HIGHLIGHT_SYMBOL.chars().count() as u16;
        let cols_area = Rect {
            x: inner.x.saturating_add(sym_w),
            width: inner.width.saturating_sub(sym_w),
            ..inner
        };
        let cols = Layout::horizontal(widths).spacing(1).split(cols_area);
        let path_width = cols.get(5).map_or(10, |r| r.width as usize);
        *app.home_col_bounds.borrow_mut() = cols.iter().map(|r| (r.x, r.x + r.width)).collect();
        path_width
    };

    let rows: Vec<Row> = indices
        .iter()
        .map(|&i| {
            let repo = &app.repos[i];
            let row_data = app.home_rows.get(&i);
            // A registration that could not be read is its own state: not
            // "still loading" (which is what an absent row used to look like,
            // forever) and not a healthy repository whose every git command
            // happened to return nothing.
            let failure = row_data.and_then(|r| r.error.as_deref());
            let healthy = row_data.filter(|r| r.error.is_none());
            let updated = match (healthy, failure) {
                (_, Some(_)) => "—".to_string(),
                (Some(r), None) => r.last_commit.clone(),
                (None, None) => "…".to_string(),
            };
            // `↑0 ↓0` on a branch with no upstream is not "in sync", it is
            // "nothing was ever compared to anything" — `?` says so without
            // posing as a count. Which kind of missing upstream it is (no
            // remote at all, or a branch that was never pushed) is spelled
            // out in the selected-repository panel, where there is room for
            // words in both languages.
            let (sync, sync_style) = match (healthy, failure) {
                (_, Some(_)) => ("—".to_string(), Style::default().fg(pal.red)),
                (None, None) => ("…".to_string(), Style::default().fg(pal.muted)),
                (Some(r), None) => match r.upstream {
                    UpstreamState::NoRemote | UpstreamState::NoUpstream => {
                        ("↑? ↓?".to_string(), Style::default().fg(pal.subtext))
                    }
                    UpstreamState::Tracking | UpstreamState::Unknown => (
                        format!("↑{} ↓{}", r.ahead, r.behind),
                        if r.ahead + r.behind > 0 {
                            Style::default().fg(pal.yellow)
                        } else {
                            Style::default().fg(pal.muted)
                        },
                    ),
                },
            };
            // A running pull/fetch/refresh takes over the Sync cell: that is
            // the value the operation is about to change, so replacing it with
            // a spinner says both "working" and "this number is being
            // recomputed" in the space of one cell.
            let (sync, sync_style) = match app.activity(i) {
                Some(act) => (
                    format!("{} {}", app.spinner(), act.label()),
                    Style::default().fg(pal.accent),
                ),
                None => (sync, sync_style),
            };
            // Tracked edits and untracked files answer different questions,
            // so the cell keeps them apart with the same letters the Status
            // tab uses: `2M 5?` rather than a single `7`. A row restored from
            // an older cache knows only the total and shows it unsuffixed
            // instead of inventing an attribution.
            let dirty_spans = match (healthy, failure) {
                (_, Some(_)) => vec![Span::styled("—", Style::default().fg(pal.red))],
                (None, None) => vec![Span::styled("…", Style::default().fg(pal.muted))],
                (Some(r), None) => match dirty_split(r) {
                    None => vec![Span::styled(
                        r.dirty.to_string(),
                        Style::default().fg(pal.red),
                    )],
                    Some((0, 0)) => vec![Span::styled("0", Style::default().fg(pal.green))],
                    Some((tracked, untracked)) => {
                        let mut spans = Vec::new();
                        if tracked > 0 {
                            spans.push(Span::styled(
                                format!("{tracked}M"),
                                Style::default().fg(pal.red),
                            ));
                        }
                        if untracked > 0 {
                            if !spans.is_empty() {
                                spans.push(Span::raw(" "));
                            }
                            spans.push(Span::styled(
                                format!("{untracked}?"),
                                Style::default().fg(pal.yellow),
                            ));
                        }
                        spans
                    }
                },
            };
            let name_spans = if let Some(g) = &repo.group {
                let trimmed = g.trim();
                if !trimmed.is_empty() {
                    vec![
                        Span::styled(format!("[{trimmed}] "), Style::default().fg(pal.yellow)),
                        Span::from(repo.name.clone()),
                    ]
                } else {
                    vec![Span::from(repo.name.clone())]
                }
            } else {
                vec![Span::from(repo.name.clone())]
            };
            let mut branch_spans = match failure {
                Some(err) => vec![Span::styled(
                    format!("⚠ {}", home_failure_reason(app, err)),
                    Style::default().fg(pal.red).add_modifier(Modifier::BOLD),
                )],
                None => vec![Span::styled(
                    match healthy {
                        Some(r) => r.branch.clone(),
                        None => "…".to_string(),
                    },
                    Style::default().fg(pal.accent),
                )],
            };
            if let Some(row_data) = healthy {
                // Attention badges go first: a fixed-width cell truncates
                // from the right, and a merge/rebase/etc. left mid-operation
                // by work done outside the dashboard — the one state this
                // observation-only tool cannot fix — must survive that
                // truncation even when PR/CI text would otherwise fill the
                // column first.
                if row_data.op_state != GitOpState::None {
                    branch_spans.push(Span::raw(" "));
                    branch_spans.push(Span::styled(
                        format!("⚠{}", row_data.op_state.label()),
                        Style::default().fg(pal.red).add_modifier(Modifier::BOLD),
                    ));
                }
                if row_data.conflicts > 0 {
                    branch_spans.push(Span::raw(" "));
                    branch_spans.push(Span::styled(
                        format!("⚠{}conflict", row_data.conflicts),
                        Style::default().fg(pal.red).add_modifier(Modifier::BOLD),
                    ));
                }
                if let Some(prs) = row_data.open_prs {
                    branch_spans.push(Span::raw(" "));
                    branch_spans.push(Span::styled(
                        format!("[PR:{prs}{}]", if prs >= 100 { "+" } else { "" }),
                        Style::default().fg(pal.accent),
                    ));
                }
                if let Some(ref ci) = row_data.ci_status {
                    let (ci_text, ci_style) = match classify_ci_status(ci) {
                        CiOutcome::Success => ("✓CI", Style::default().fg(pal.green)),
                        CiOutcome::Failure => ("✗CI", Style::default().fg(pal.red)),
                        CiOutcome::Other => ("●CI", Style::default().fg(pal.yellow)),
                    };
                    branch_spans.push(Span::raw(" "));
                    branch_spans.push(Span::styled(ci_text, ci_style));
                }
            }
            Row::new(vec![
                Cell::from(Line::from(name_spans)),
                Cell::from(Line::from(branch_spans)),
                Cell::from(Span::styled(sync, sync_style)),
                Cell::from(Line::from(dirty_spans)),
                Cell::from(Span::styled(updated, Style::default().fg(pal.subtext))),
                Cell::from(Span::styled(
                    shorten_path(&repo.path, path_width),
                    Style::default().fg(pal.muted),
                )),
            ])
        })
        .collect();

    let filter = if app.is_filtering() || !app.home_filter.is_empty() {
        format!(" / {}", app.home_filter)
    } else {
        String::new()
    };
    let group_str = format!(" [Group: {}]", app.group_filter_label());
    let attention_str = if app.attention_only {
        format!("  ⚠{}", app.tt("needs attention", "要対応"))
    } else {
        String::new()
    };
    let title = if indices.is_empty() {
        format!(
            "{} (0/{}){group_str}{attention_str}{filter}  {}",
            app.t("repositories"),
            app.repos.len(),
            app.tt("no matches — esc to clear", "一致なし — esc で解除")
        )
    } else {
        format!(
            "{} ({}/{}){group_str}{attention_str} {}{filter}",
            app.t("repositories"),
            indices.len(),
            app.repos.len(),
            app.sort_label()
        )
    };

    let table = Table::new(rows, widths)
        .header(header)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(pal.border))
                .title(title)
                .title_style(Style::default().fg(pal.accent)),
        )
        .row_highlight_style(
            Style::default()
                .bg(pal.overlay)
                .fg(pal.text)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol(HIGHLIGHT_SYMBOL)
        .column_spacing(1);

    let mut state = TableState::default().with_offset(app.home_offset.get());
    if !indices.is_empty() {
        state.select(Some(app.home_selected.min(indices.len() - 1)));
    }
    frame.render_stateful_widget(table, area, &mut state);
    // Remember where the viewport ended up so handle_mouse_click can turn a
    // screen row back into a repository index.
    app.home_offset.set(state.offset());
}

/// Width `Tabs` needs for these labels: each is padded by a space on both
/// sides and separated by a divider.
/// Short, localized reason for a Home row that could not be read. The cell
/// it lands in is one table column wide, so the full message stays in the
/// selected-repository panel and this only has to say which kind of broken.
fn home_failure_reason(app: &App, err: &str) -> String {
    match classify_home_error(err) {
        HomeFailure::MissingPath => app.tt("path missing", "パスなし"),
        HomeFailure::NotARepository => app.tt("not a git repo", "Git管理外"),
        HomeFailure::Unreadable => app.tt("unreadable", "読取不可"),
    }
}

fn tab_bar_width(labels: &[String]) -> usize {
    use unicode_width::UnicodeWidthStr;
    labels.iter().map(|l| l.width() + 2).sum::<usize>() + labels.len().saturating_sub(1)
}

/// Tab labels at the widest level that fits, falling back to shorter names and
/// finally to bare numbers.
///
/// The full labels need an 87-column terminal in English and 95 in Japanese.
/// On an 80-column one the bar was clipped, and in Japanese that removed
/// `7 ワークツリー` outright — leaving no sign a seventh tab existed, though
/// pressing `7` still worked. Shrinking beats vanishing.
pub(crate) fn repo_tab_labels(app: &App, width: u16) -> Vec<String> {
    let full: Vec<String> = vec![
        app.tt("1 Status", "1 状態"),
        app.tt("2 Commits", "2 コミット"),
        format!("3 {}", app.t("tab_branches")),
        app.tt("4 Tags", "4 タグ"),
        format!("5 {}", app.tt("Stash", "Stash")),
        app.tt("6 Contributors", "6 コントリビューター"),
        app.tt("7 Worktrees", "7 ワークツリー"),
    ];
    let inner = width.saturating_sub(2) as usize;
    if tab_bar_width(&full) <= inner {
        return full;
    }
    let short: Vec<String> = vec![
        app.tt("1 Status", "1 状態"),
        app.tt("2 Commits", "2 コミット"),
        app.tt("3 Branch", "3 ブランチ"),
        app.tt("4 Tags", "4 タグ"),
        app.tt("5 Stash", "5 Stash"),
        app.tt("6 People", "6 貢献者"),
        app.tt("7 Trees", "7 ツリー"),
    ];
    if tab_bar_width(&short) <= inner {
        return short;
    }
    (1..=7).map(|n| n.to_string()).collect()
}

fn draw_repo(frame: &mut Frame, app: &App, area: Rect, pal: Palette) {
    let tabs = RepoTab::all();
    let labels_text = repo_tab_labels(app, area.width);
    let labels: Vec<Line> = labels_text.iter().cloned().map(Line::from).collect();
    let selected = tabs.iter().position(|t| *t == app.repo_tab).unwrap_or(0);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(1)])
        .split(area);

    let tabs_w = Tabs::new(labels)
        .select(selected)
        .highlight_style(Style::default().fg(pal.accent).add_modifier(Modifier::BOLD))
        .style(Style::default().fg(pal.subtext))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(pal.border)),
        );
    frame.render_widget(tabs_w, chunks[0]);
    // Record where each tab landed so a click maps to the tab actually under
    // the cursor. `Tabs` pads every label with a space on each side and puts a
    // one-column divider between them.
    {
        use unicode_width::UnicodeWidthStr;
        let inner = chunks[0].inner(Margin::new(1, 1));
        let mut x = inner.x;
        let mut bounds = Vec::with_capacity(labels_text.len());
        for label in &labels_text {
            let w = label.width() as u16 + 2;
            bounds.push((x, x + w));
            x += w + 1; // divider
        }
        *app.tab_bounds.borrow_mut() = bounds;
    }

    if app.repo_loading && app.repo_data.is_none() {
        frame.render_widget(
            Paragraph::new(format!(
                "{} {}",
                app.spinner(),
                app.t("analyzing_repo_data")
            ))
            .style(Style::default().fg(pal.yellow))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(pal.border)),
            ),
            chunks[1],
        );
        return;
    }

    match app.repo_tab {
        RepoTab::Status => draw_status(frame, app, chunks[1], pal),
        RepoTab::Commits => draw_commits(frame, app, chunks[1], pal),
        RepoTab::Branches => draw_branches(frame, app, chunks[1], pal),
        RepoTab::Tags => draw_tags(frame, app, chunks[1], pal),
        RepoTab::Stash => draw_stash(frame, app, chunks[1], pal),
        RepoTab::Contributors => draw_contributors(frame, app, chunks[1], pal),
        RepoTab::Worktrees => draw_worktrees(frame, app, chunks[1], pal),
    }
}

fn draw_status(frame: &mut Frame, app: &App, area: Rect, pal: Palette) {
    let Some(data) = app.repo_data.as_ref() else {
        return;
    };
    let s = &data.summary;
    let split = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(7),
            Constraint::Length(8),
            Constraint::Min(1),
        ])
        .split(area);

    let dirty_color = if s.uncommitted_changes > 0 {
        pal.red
    } else {
        pal.green
    };
    let dirty_text = if s.uncommitted_changes == 0 {
        app.tt("clean", "クリーン")
    } else {
        format!("{} files", s.uncommitted_changes)
    };

    let mut kpis = vec![
        Line::from(vec![
            Span::styled(
                format!("  {:<14}", app.tt("Branch:", "ブランチ:")),
                Style::default().fg(pal.muted),
            ),
            Span::styled(
                truncate(&s.current_branch, 20),
                Style::default().fg(pal.accent).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("  {:<14}", app.tt("Commits:", "コミット数:")),
                Style::default().fg(pal.muted),
            ),
            Span::styled(
                format!("{:<10}", s.total_commits),
                Style::default().fg(pal.text),
            ),
            Span::styled(
                format!("  {:<14}", app.tt("Contributors:", "貢献者数:")),
                Style::default().fg(pal.muted),
            ),
            Span::styled(
                s.total_contributors.to_string(),
                Style::default().fg(pal.text),
            ),
        ]),
        Line::from(vec![
            Span::styled(
                format!("  {:<14}", app.tt("Sync (↑/↓):", "同期 (↑/↓):")),
                Style::default().fg(pal.muted),
            ),
            // Same distinction the Home table's Sync cell makes, with room
            // here to say which case it is: zero/zero without an upstream is
            // not agreement with a remote, it is the absence of a comparison.
            {
                let (text, style) = if s.has_upstream {
                    (
                        format!("↑{} ↓{}", s.ahead, s.behind),
                        Style::default().fg(pal.yellow),
                    )
                } else if s.has_remote {
                    (
                        app.tt("↑? ↓? no upstream", "↑? ↓? 上流なし"),
                        Style::default().fg(pal.subtext),
                    )
                } else {
                    (
                        app.tt("↑? ↓? no remote", "↑? ↓? リモートなし"),
                        Style::default().fg(pal.subtext),
                    )
                };
                Span::styled(truncate(&text, 20), style)
            },
            Span::styled(
                format!("  {:<14}", app.tt("Branches:", "ブランチ数:")),
                Style::default().fg(pal.muted),
            ),
            Span::styled(
                format!("{:<10}", s.total_branches),
                Style::default().fg(pal.text),
            ),
            Span::styled(
                format!("  {:<14}", app.tt("Uncommitted:", "未コミット:")),
                Style::default().fg(pal.muted),
            ),
            Span::styled(dirty_text.to_string(), Style::default().fg(dirty_color)),
        ]),
        Line::from({
            let mut spans = vec![
                Span::styled(
                    format!("  {:<14}", app.tt("Local Path:", "パス:")),
                    Style::default().fg(pal.muted),
                ),
                Span::styled(truncate(&s.repo_path, 34), Style::default().fg(pal.subtext)),
            ];
            if let Some(ref ci_pr) = s.remote_ci_pr {
                if let Some(prs) = ci_pr.open_prs {
                    spans.push(Span::styled(
                        format!("  {:<10}", "PR:"),
                        Style::default().fg(pal.muted),
                    ));
                    spans.push(Span::styled(
                        format!(
                            "{:<10}",
                            format!("{prs}{} open", if prs >= 100 { "+" } else { "" })
                        ),
                        Style::default().fg(pal.accent),
                    ));
                }
                if let Some(ref ci) = ci_pr.ci_status {
                    let (ci_icon, ci_style) = match classify_ci_status(ci) {
                        CiOutcome::Success => ("✓ passing", Style::default().fg(pal.green)),
                        CiOutcome::Failure => ("✗ failing", Style::default().fg(pal.red)),
                        CiOutcome::Other => (ci.as_str(), Style::default().fg(pal.yellow)),
                    };
                    spans.push(Span::styled(
                        format!("  CI({}): ", ci_pr.ci_branch.as_deref().unwrap_or("all")),
                        Style::default().fg(pal.muted),
                    ));
                    spans.push(Span::styled(
                        format!(
                            "{ci_icon} {}",
                            app.tt(
                                "(repo latest; Home: context)",
                                "（全体最新・Homeに取得状態）"
                            )
                        ),
                        ci_style,
                    ));
                }
            }
            spans
        }),
    ];
    if s.op_state != GitOpState::None || s.conflicts > 0 {
        let mut warn = vec![Span::styled(
            format!("  ⚠ {}", s.op_state.label()),
            Style::default().fg(pal.red).add_modifier(Modifier::BOLD),
        )];
        if s.conflicts > 0 {
            warn.push(Span::styled(
                format!(
                    "  {} {}",
                    s.conflicts,
                    app.tt("unresolved conflict(s)", "件の未解決コンフリクト")
                ),
                Style::default().fg(pal.red),
            ));
        }
        kpis.push(Line::from(warn));
    }
    frame.render_widget(
        Paragraph::new(kpis).block(
            Block::default()
                .borders(Borders::ALL)
                .title(app.tt(
                    "Overview  (2 = full commit log)",
                    "概要  (2 = コミット履歴)",
                ))
                .border_style(Style::default().fg(pal.border))
                .title_style(Style::default().fg(pal.accent)),
        ),
        split[0],
    );

    let recent: Vec<Line> = data
        .commits
        .iter()
        .take(6)
        .map(|c| {
            let mut spans = vec![
                Span::styled(c.hash.clone(), Style::default().fg(pal.accent)),
                Span::styled(format!("  {}  ", c.date), Style::default().fg(pal.muted)),
            ];
            spans.extend(render_ref_badges(&c.refs, pal));
            spans.push(Span::raw(truncate(&c.message, 48)));
            Line::from(spans)
        })
        .collect();
    let recent_body = if recent.is_empty() {
        vec![Line::styled(
            app.tt("No recent commits.", "最近のコミットはありません。"),
            Style::default().fg(pal.muted),
        )]
    } else {
        recent
    };
    frame.render_widget(
        Paragraph::new(recent_body).block(
            Block::default()
                .borders(Borders::ALL)
                .title(app.tt("Recent commits", "最近のコミット"))
                .border_style(Style::default().fg(pal.border))
                .title_style(Style::default().fg(pal.accent)),
        ),
        split[1],
    );

    render_path_list(
        frame,
        app,
        split[2],
        pal,
        format!(
            "{} ({})",
            app.t("uncommitted_changes_working"),
            data.working_files.len()
        ),
        app.tt(
            "No uncommitted files. Press 2 for commits.",
            "未コミットなし。2 でコミット履歴。",
        ),
    );
}

fn render_ref_badges(refs: &[CommitRef], pal: Palette) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    for r in refs {
        let (label, style) = if r.is_head {
            (
                format!(" ({})", r.name),
                Style::default().fg(pal.green).add_modifier(Modifier::BOLD),
            )
        } else if r.is_tag {
            (
                format!(" [tag: {}]", r.name),
                Style::default().fg(pal.yellow).add_modifier(Modifier::BOLD),
            )
        } else if r.is_remote {
            (format!(" [{}]", r.name), Style::default().fg(pal.red))
        } else {
            (format!(" [{}]", r.name), Style::default().fg(pal.accent))
        };
        spans.push(Span::styled(label, style));
    }
    if !refs.is_empty() {
        spans.push(Span::raw(" "));
    }
    spans
}

/// Map one `git log --graph --color=always` SGR code to a lane color.
/// `code` is everything between `\x1b[` and `m` (e.g. `"31"`, `"1;31"`, or
/// `""` for a bare reset) — only the final `;`-separated number matters,
/// since git never combines a color with another attribute here.
/// One lane's resolved rendering state: which of the theme's 6 hues, and
/// whether git marked it "bold" (the attribute it uses to extend 6 basic
/// colors into 12 distinguishable lanes — lane 7 reuses lane 1's hue as
/// bold red rather than introducing a 7th hue).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct GraphColor {
    idx: usize,
    bold: bool,
}

/// Resolve one SGR code (the text between `\x1b[` and `m`, e.g. `"31"` or
/// `"1;31"`) to a lane color/weight pair.
///
/// Every `;`-separated field is inspected rather than just the last one, for
/// two reasons: git emits the bold attribute *before* the color (`"1;31"`,
/// not `"31;1"`), and a 256-color or truecolor spec (`"38;5;208"`,
/// `"38;2;R;G;B"`) has a trailing field that collides with a basic color
/// number by pure coincidence — reading only that field previously mapped a
/// user's custom `log.graphColors` truecolor value onto a arbitrary,
/// unrelated lane color instead of leaving it unrecognized.
fn ansi_sgr_to_graph_color(code: &str) -> Option<GraphColor> {
    let fields: Vec<i32> = code.split(';').filter_map(|s| s.parse().ok()).collect();
    if fields.iter().any(|&n| n == 38 || n == 48) {
        return None; // extended/256/truecolor spec — not a basic lane color
    }
    let idx = fields.iter().find_map(|&n| match n {
        31..=36 => Some((n - 31) as usize),
        91..=96 => Some((n - 91) as usize),
        _ => None,
    })?;
    Some(GraphColor {
        idx,
        bold: fields.contains(&1),
    })
}

/// Parse `--graph --color=always` output into styled spans.
///
/// git's own graph-layout algorithm already assigns each lane a consistent
/// ANSI color (and, past the 6th concurrent lane, a bold attribute on top of
/// a reused hue) across every row it spans (`log.graphColors`, default
/// red/green/yellow/blue/magenta/cyan); this maps that straight onto the
/// active theme instead of re-deriving lane identity from the plain
/// characters, so any topology git can lay out — however many concurrent
/// branches — gets correct, lane-consistent color for free. Commit markers
/// (`*`/`o`) are never colored by git itself, so they keep a fixed bold
/// treatment regardless of the surrounding lane color.
fn render_graph_spans(graph: &str, pal: Palette) -> Vec<Span<'static>> {
    let mut spans: Vec<Span<'static>> = Vec::new();
    if graph.is_empty() {
        return spans;
    }
    let flush = |buf: &mut String, spans: &mut Vec<Span<'static>>, color: Option<GraphColor>| {
        for ch in std::mem::take(buf).chars() {
            let style = match ch {
                '*' | 'o' => Style::default().fg(pal.yellow).add_modifier(Modifier::BOLD),
                _ => match color.and_then(|c| pal.graph_colors.get(c.idx)) {
                    Some(&rgb) => {
                        let mut s = Style::default().fg(rgb);
                        if color.is_some_and(|c| c.bold) {
                            s = s.add_modifier(Modifier::BOLD);
                        }
                        s
                    }
                    None => Style::default().fg(pal.muted),
                },
            };
            spans.push(Span::styled(ch.to_string(), style));
        }
    };

    // A well-formed SGR code here is always short ("31", "1;31", ...); if no
    // 'm' shows up within a generous bound, the sequence isn't one git would
    // emit (e.g. a cursor-control or private-mode escape) and consuming the
    // rest of the string looking for 'm' would silently eat real content.
    const MAX_SGR_CODE_LEN: usize = 16;

    let mut current_color: Option<GraphColor> = None;
    let mut buf = String::new();
    let mut chars = graph.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' && chars.peek() == Some(&'[') {
            chars.next();
            let mut code = String::new();
            let mut terminated = false;
            for nc in chars.by_ref() {
                if nc == 'm' {
                    terminated = true;
                    break;
                }
                code.push(nc);
                if code.len() >= MAX_SGR_CODE_LEN {
                    break;
                }
            }
            if terminated {
                flush(&mut buf, &mut spans, current_color);
                current_color = ansi_sgr_to_graph_color(&code);
            }
            // An unterminated/oversized sequence is dropped rather than
            // rendered — it's already been consumed from the iterator with
            // no way to push it back, but it's bounded to MAX_SGR_CODE_LEN
            // bytes rather than potentially the rest of the string.
            continue;
        }
        buf.push(c);
    }
    flush(&mut buf, &mut spans, current_color);
    spans
}

fn draw_commits(frame: &mut Frame, app: &App, area: Rect, pal: Palette) {
    let Some(data) = app.repo_data.as_ref() else {
        return;
    };
    let split = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(58), Constraint::Percentage(42)])
        .split(area);
    let vis = app.visible_indices();
    let items: Vec<ListItem> = vis
        .iter()
        .map(|&i| {
            let c = &data.commits[i];
            let (mark, mark_style) = if app.commit_base.as_deref() == Some(c.hash.as_str()) {
                (
                    "B",
                    Style::default().fg(pal.green).add_modifier(Modifier::BOLD),
                )
            } else if app.commit_target.as_deref() == Some(c.hash.as_str()) {
                (
                    "T",
                    Style::default().fg(pal.yellow).add_modifier(Modifier::BOLD),
                )
            } else {
                (" ", Style::default().fg(pal.muted))
            };
            let mut line_spans = vec![Span::styled(format!("[{mark}] "), mark_style)];
            line_spans.extend(render_graph_spans(&c.graph, pal));
            line_spans.push(Span::styled(
                c.hash.clone(),
                Style::default().fg(pal.accent),
            ));
            line_spans.push(Span::styled(
                format!("  {}  ", c.date),
                Style::default().fg(pal.muted),
            ));
            line_spans.push(Span::styled(
                format!("{}  ", truncate(&c.author, 16)),
                Style::default().fg(pal.subtext),
            ));
            line_spans.extend(render_ref_badges(&c.refs, pal));
            line_spans.push(Span::raw(c.message.clone()));
            ListItem::new(Line::from(line_spans))
        })
        .collect();
    let label = app.tt("Commits", "コミット");
    let title = match (&app.commit_base, &app.commit_target) {
        (Some(b), Some(t)) => format!(
            "{label}  {b}...{t}  {}",
            app.tt("(enter to compare)", "(enter で比較)")
        ),
        (Some(b), None) => format!(
            "{label}  {}={b}  {}",
            app.tt("base", "基準"),
            app.tt("(pick a target)", "(比較対象を選択)")
        ),
        // The `[ ]` at the start of each row is the affordance, so name it —
        // "space to mark" alone never told anyone the marker was clickable.
        _ => format!(
            "{label} ({}/{})  {}",
            vis.len(),
            data.commits.len(),
            app.tt("(click [ ] or space)", "([ ] クリック / space)")
        ),
    };
    app.list_viewport.set(render_items(
        frame,
        split[0],
        pal,
        items,
        app.list_selected,
        title,
        app.list_error(),
        &app.tt("No commits to show.", "表示するコミットがありません。"),
    ));

    let mut preview_lines: Vec<Line> = Vec::new();
    if let Some(sel) = vis
        .get(app.list_selected)
        .and_then(|&i| data.commits.get(i))
    {
        let mut header_spans = vec![
            Span::styled(sel.hash.clone(), Style::default().fg(pal.accent)),
            Span::raw("  "),
            Span::styled(sel.author.clone(), Style::default().fg(pal.subtext)),
            Span::raw("  "),
            Span::raw(sel.date.clone()),
        ];
        if !sel.refs.is_empty() {
            header_spans.push(Span::raw("  "));
            header_spans.extend(render_ref_badges(&sel.refs, pal));
        }
        preview_lines.push(Line::from(header_spans));
        preview_lines.push(Line::from(sel.message.clone()));
    }
    if let Some(p) = &app.commit_preview {
        preview_lines.push(Line::from(""));
        for row in p.header.lines().take(8) {
            preview_lines.push(Line::styled(
                row.to_string(),
                Style::default().fg(pal.subtext),
            ));
        }
        if !p.files.is_empty() {
            preview_lines.push(Line::from(""));
            for f in p.files.iter().take(12) {
                preview_lines.push(file_status_line(f, pal));
            }
            if p.files.len() > 12 {
                preview_lines.push(Line::styled(
                    format!("… {} more", p.files.len() - 12),
                    Style::default().fg(pal.muted),
                ));
            }
        }
    }
    if preview_lines.is_empty() {
        preview_lines.push(Line::styled(
            app.tt("Select a commit.", "コミットを選択。"),
            Style::default().fg(pal.muted),
        ));
    }
    frame.render_widget(
        Paragraph::new(preview_lines).block(
            Block::default()
                .borders(Borders::ALL)
                .title(app.tt("Commit", "コミット内容"))
                .border_style(Style::default().fg(pal.border))
                .title_style(Style::default().fg(pal.accent)),
        ),
        split[1],
    );
}

fn draw_contributors(frame: &mut Frame, app: &App, area: Rect, pal: Palette) {
    let Some(data) = app.repo_data.as_ref() else {
        return;
    };
    let vis = app.visible_indices();
    let items: Vec<ListItem> = vis
        .iter()
        .map(|&i| {
            let c = &data.contributors[i];
            let (mark, mark_style, name_style) = if c.is_member && c.is_active {
                (
                    "[✓] active  ",
                    Style::default().fg(pal.green).add_modifier(Modifier::BOLD),
                    Style::default().fg(pal.text),
                )
            } else if c.is_member {
                (
                    "[ ] inactive",
                    Style::default().fg(pal.yellow),
                    Style::default().fg(pal.subtext),
                )
            } else {
                (
                    "    -       ",
                    Style::default().fg(pal.muted),
                    Style::default().fg(pal.subtext),
                )
            };
            ListItem::new(Line::from(vec![
                Span::styled(mark, mark_style),
                Span::styled(
                    format!("  {:>5}  ", c.commit_count),
                    Style::default().fg(pal.accent),
                ),
                Span::styled(format!("{}  ", truncate(&c.name, 22)), name_style),
                Span::styled(
                    format!("last {}  ", c.last_commit),
                    Style::default().fg(pal.muted),
                ),
                Span::styled(c.email.clone(), Style::default().fg(pal.subtext)),
            ]))
        })
        .collect();
    let period_label = match app.lang() {
        crate::config::Language::English => app.contributor_time_span.label_en(),
        crate::config::Language::Japanese => app.contributor_time_span.label_ja(),
    };
    let title = format!(
        // The active-members note is conditional, so append it rather than
        // interpolating an empty string and leaving a space before the border.
        "{} ({}/{}) [{period_label}]{}",
        app.tt("Contributors", "貢献者"),
        vis.len(),
        data.contributors.len(),
        if app.active_only {
            format!(" {}", app.tt("[active members]", "[在籍メンバーのみ]"))
        } else {
            String::new()
        }
    );
    app.list_viewport.set(render_items(
        frame,
        area,
        pal,
        items,
        app.list_selected,
        title,
        app.list_error(),
        &app.tt("No contributors.", "貢献者がいません。"),
    ));
}

fn draw_branches(frame: &mut Frame, app: &App, area: Rect, pal: Palette) {
    let Some(data) = app.repo_data.as_ref() else {
        return;
    };
    let vis = app.visible_indices();
    let items: Vec<ListItem> = vis
        .iter()
        .map(|&i| {
            let b = &data.branches[i];
            let kind = if b.is_remote { "remote" } else { "local " };
            ListItem::new(format!(
                "{kind}  {:<32}  {}  {}",
                truncate(&b.name, 32),
                b.date,
                b.message
            ))
            .style(if b.is_remote {
                Style::default().fg(pal.subtext)
            } else {
                Style::default().fg(pal.accent)
            })
        })
        .collect();
    app.list_viewport.set(render_items(
        frame,
        area,
        pal,
        items,
        app.list_selected,
        format!("{} ({})", app.t("tab_branches"), vis.len()),
        app.list_error(),
        &app.tt("No branches.", "ブランチがありません。"),
    ));
}

fn draw_tags(frame: &mut Frame, app: &App, area: Rect, pal: Palette) {
    let Some(data) = app.repo_data.as_ref() else {
        return;
    };
    let vis = app.visible_indices();
    let items: Vec<ListItem> = vis
        .iter()
        .map(|&i| {
            let t = &data.tags[i];
            let mark = if app.tag_base.as_deref() == Some(t.name.as_str()) {
                "B"
            } else if app.tag_target.as_deref() == Some(t.name.as_str()) {
                "T"
            } else {
                " "
            };
            ListItem::new(format!(
                "[{mark}] {:<24}  {}  {}  {}",
                t.name, t.hash, t.date, t.message
            ))
        })
        .collect();
    let label = app.tt("Tags", "タグ");
    let title = match (&app.tag_base, &app.tag_target) {
        (Some(b), Some(t)) => format!(
            "{label}  {b}...{t}  {}",
            app.tt("(enter to diff)", "(enter で比較)")
        ),
        (Some(b), None) => format!(
            "{label}  {}={b}  {}",
            app.tt("base", "基準"),
            app.tt("(pick a target)", "(比較対象を選択)")
        ),
        _ => format!(
            "{label} ({})  {}",
            vis.len(),
            app.tt("(click [ ] or space)", "([ ] クリック / space)")
        ),
    };
    app.list_viewport.set(render_items(
        frame,
        area,
        pal,
        items,
        app.list_selected,
        title,
        app.list_error(),
        &app.tt("No tags.", "タグがありません。"),
    ));
}

fn draw_stash(frame: &mut Frame, app: &App, area: Rect, pal: Palette) {
    let Some(data) = app.repo_data.as_ref() else {
        return;
    };
    let vis = app.visible_indices();
    let items: Vec<ListItem> = vis
        .iter()
        .map(|&i| {
            let s = &data.stashes[i];
            ListItem::new(format!(
                "{}  {}  {}  {}",
                s.ref_name, s.date_relative, s.author, s.message
            ))
        })
        .collect();
    app.list_viewport.set(render_items(
        frame,
        area,
        pal,
        items,
        app.list_selected,
        format!("Stash ({})", vis.len()),
        app.list_error(),
        &app.tt("No stashes.", "stash はありません。"),
    ));
}

fn draw_worktrees(frame: &mut Frame, app: &App, area: Rect, pal: Palette) {
    let Some(data) = app.repo_data.as_ref() else {
        return;
    };
    let vis = app.visible_indices();
    let rows: Vec<Row> = vis
        .iter()
        .map(|&i| {
            let wt = &data.worktrees[i];
            let branch_str = if let Some(ref b) = wt.branch {
                format!("({})", b)
            } else if wt.is_detached {
                "(detached)".to_string()
            } else if wt.is_bare {
                "(bare)".to_string()
            } else {
                "-".to_string()
            };

            let status_str = if wt.is_locked {
                "[locked]"
            } else if wt.is_prunable {
                "[prunable]"
            } else {
                ""
            };

            let head_short = short_hash(&wt.head);

            let cells = vec![
                Cell::from(wt.path.clone()).style(Style::default().fg(pal.text)),
                Cell::from(branch_str).style(Style::default().fg(pal.accent)),
                Cell::from(head_short).style(Style::default().fg(pal.yellow)),
                Cell::from(status_str).style(Style::default().fg(pal.red)),
            ];
            Row::new(cells)
        })
        .collect();

    let header = Row::new(vec![
        Cell::from(app.tt("Path", "パス")).style(
            Style::default()
                .fg(pal.subtext)
                .add_modifier(Modifier::BOLD),
        ),
        Cell::from(app.tt("Branch", "ブランチ")).style(
            Style::default()
                .fg(pal.subtext)
                .add_modifier(Modifier::BOLD),
        ),
        Cell::from("HEAD").style(
            Style::default()
                .fg(pal.subtext)
                .add_modifier(Modifier::BOLD),
        ),
        Cell::from(app.tt("Status", "状態")).style(
            Style::default()
                .fg(pal.subtext)
                .add_modifier(Modifier::BOLD),
        ),
    ]);

    let title = format!("{} ({})", app.tt("Worktrees", "ワークツリー"), vis.len());
    let table = Table::new(
        rows,
        [
            Constraint::Percentage(50),
            Constraint::Percentage(25),
            Constraint::Percentage(15),
            Constraint::Percentage(10),
        ],
    )
    .header(header)
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(pal.border))
            .title(title),
    )
    .row_highlight_style(
        Style::default()
            .bg(pal.surface)
            .add_modifier(Modifier::BOLD),
    );

    let mut state = TableState::default();
    if !vis.is_empty() {
        state.select(Some(app.list_selected.min(vis.len().saturating_sub(1))));
    }
    frame.render_stateful_widget(table, area, &mut state);
    // This tab is a Table, not a List, and it has a header row — so the first
    // item sits one row lower than in the other tabs.
    let inner = area.inner(Margin::new(1, 1));
    app.list_viewport.set(ListViewport {
        y: inner.y.saturating_add(1),
        height: inner.height.saturating_sub(1),
        x: inner.x,
        width: inner.width,
        offset: state.offset(),
    });
}

fn draw_diff(frame: &mut Frame, app: &App, area: Rect, pal: Palette) {
    let Some(diff) = app.diff.as_ref() else {
        return;
    };
    let has_hunks = !diff.hunks.is_empty();
    let split = if has_hunks {
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(26),
                Constraint::Percentage(24),
                Constraint::Percentage(50),
            ])
            .split(area)
    } else {
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(32), Constraint::Percentage(68)])
            .split(area)
    };

    let file_items: Vec<ListItem> = diff
        .files
        .iter()
        .map(|f| {
            ListItem::new(format!("[{}] {}", f.status, f.path)).style(status_style(&f.status, pal))
        })
        .collect();
    let file_title = format!(
        "{} {} ({})",
        if app.focus == FocusPane::List {
            "▸"
        } else {
            " "
        },
        app.t("changed_files_header"),
        diff.files.len()
    );
    let no_changes = app.t("no_changes");
    render_items(
        frame,
        split[0],
        pal,
        file_items,
        diff.file_idx,
        file_title,
        None,
        no_changes.trim(),
    );

    let content_area = if has_hunks {
        let hunk_items: Vec<ListItem> = diff
            .hunks
            .iter()
            .map(|h| ListItem::new(h.label.clone()))
            .collect();
        let hunk_title = format!(
            "{} hunks ({}/{})",
            if app.focus == FocusPane::Hunks {
                "▸"
            } else {
                " "
            },
            if diff.hunks.is_empty() {
                0
            } else {
                diff.hunk_idx + 1
            },
            diff.hunks.len()
        );
        render_items(
            frame,
            split[1],
            pal,
            hunk_items,
            diff.hunk_idx,
            hunk_title,
            None,
            &app.tt("No hunks.", "hunk なし"),
        );
        split[2]
    } else {
        split[1]
    };

    let content_border = if app.focus == FocusPane::Content {
        Style::default().fg(pal.accent)
    } else {
        Style::default().fg(pal.border)
    };
    let hunk_range = diff.hunks.get(diff.hunk_idx).map(|h| (h.start, h.end));
    let body: Vec<Line> = if let Some(err) = &diff.error {
        vec![Line::styled(err.clone(), Style::default().fg(pal.red))]
    } else if diff.loading {
        vec![Line::styled(
            format!("{} {}", app.spinner(), app.t("fetching_diff")),
            Style::default().fg(pal.yellow),
        )]
    } else if diff.files.is_empty() {
        vec![Line::styled(
            app.t("no_changes"),
            Style::default().fg(pal.muted),
        )]
    } else {
        let start = if diff.lines.is_empty() {
            0
        } else {
            diff.scroll.min(diff.lines.len() - 1)
        };
        const BLAME_GUTTER_WIDTH: usize = 22;
        diff.lines
            .iter()
            .enumerate()
            .skip(start)
            .map(|(idx, l)| {
                let mut style = match l.kind {
                    DiffRowKind::Added => Style::default().fg(pal.green),
                    DiffRowKind::Removed => Style::default().fg(pal.red),
                    DiffRowKind::Modified => Style::default().fg(pal.yellow),
                    DiffRowKind::Context => Style::default().fg(pal.text),
                };
                let in_hunk = hunk_range.is_some_and(|(hs, he)| idx >= hs && idx < he);
                if in_hunk {
                    style = style.bg(pal.overlay);
                }
                let Some(blame) = diff.blame.as_ref() else {
                    return Line::styled(l.text.clone(), style);
                };
                // Blame is indexed by the *new* file's line number: removed
                // lines have no new_no and get a blank gutter.
                let entry = l
                    .new_no
                    .and_then(|n| n.checked_sub(1))
                    .and_then(|i| blame.get(i));
                let gutter = entry
                    .map(|b| format!("{} {}", short_hash(&b.hash), b.author))
                    .unwrap_or_default();
                let mut gutter_style = Style::default().fg(pal.muted);
                let mut sep_style = Style::default().fg(pal.border);
                // Extend the current-hunk highlight across the gutter and
                // separator too — applying it only to the text span left
                // highlighted rows looking cut off partway through the line.
                if in_hunk {
                    gutter_style = gutter_style.bg(pal.overlay);
                    sep_style = sep_style.bg(pal.overlay);
                }
                Line::from(vec![
                    Span::styled(truncate(&gutter, BLAME_GUTTER_WIDTH), gutter_style),
                    Span::styled("│ ", sep_style),
                    Span::styled(l.text.clone(), style),
                ])
            })
            .collect()
    };
    let header = diff
        .header
        .as_deref()
        .unwrap_or("")
        .lines()
        .next()
        .unwrap_or("");
    let mut flags = Vec::new();
    if app.diff_ignore_whitespace() {
        flags.push("w:ignore-ws");
    }
    if app.diff_full_file() {
        flags.push("f:full");
    }
    if app.diff_show_blame() {
        flags.push(if diff.blame_loading {
            "b:blame…"
        } else {
            "b:blame"
        });
    }
    let flags_str = if flags.is_empty() {
        String::new()
    } else {
        format!("  [{}]", flags.join(" "))
    };
    let title = format!(
        "{} {}  {}{flags_str}",
        if app.focus == FocusPane::Content {
            "▸"
        } else {
            " "
        },
        app.t("diff"),
        truncate(header, 40).trim_end()
    );
    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .border_style(content_border)
        .title_style(Style::default().fg(pal.accent));
    // Wrapping doesn't know about the blame gutter: a wrapped continuation
    // line starts at column 0 with no gutter or separator of its own,
    // reading as a *different* line's blame. Clipping long lines instead of
    // corrupting the gutter is the better trade-off while blame is shown.
    let paragraph = if diff.blame.is_some() {
        Paragraph::new(body)
    } else {
        Paragraph::new(body).wrap(Wrap { trim: false })
    };
    frame.render_widget(paragraph.block(block), content_area);
}

fn draw_settings(frame: &mut Frame, app: &App, area: Rect, pal: Palette) {
    use unicode_width::UnicodeWidthStr;
    let config_label = app.t("config_file_location");
    let config_label_w = config_label.width() as u16;
    let lang = match app.lang() {
        crate::config::Language::English => "English",
        crate::config::Language::Japanese => "日本語",
    };
    let header = vec![
        Line::from(vec![
            Span::styled(app.t("display_language"), Style::default().fg(pal.muted)),
            Span::raw("  "),
            Span::styled(lang, Style::default().fg(pal.accent)),
            Span::styled("  (l)", Style::default().fg(pal.muted)),
        ]),
        Line::from(vec![
            Span::styled("Diff  ", Style::default().fg(pal.muted)),
            Span::styled(app.diff_tool_label(), Style::default().fg(pal.accent)),
            Span::styled("  (c)", Style::default().fg(pal.muted)),
        ]),
        Line::from(vec![
            Span::styled(
                app.tt("Auto-refresh  ", "自動更新  "),
                Style::default().fg(pal.muted),
            ),
            Span::styled(app.auto_refresh_label(), Style::default().fg(pal.accent)),
            Span::styled("  (i)", Style::default().fg(pal.muted)),
        ]),
        Line::from(vec![
            Span::styled(
                app.tt("Theme  ", "テーマ  "),
                Style::default().fg(pal.muted),
            ),
            Span::styled(
                app.theme_name().to_string(),
                Style::default().fg(pal.accent),
            ),
            Span::styled("  (T)", Style::default().fg(pal.muted)),
        ]),
        Line::from(vec![
            Span::styled(config_label.clone(), Style::default().fg(pal.muted)),
            Span::raw("  "),
            Span::raw(truncate_middle(
                &crate::config::get_config_dir().display().to_string(),
                area.width.saturating_sub(config_label_w + 4) as usize,
            )),
        ]),
    ];
    let split = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(7),
            Constraint::Length(3),
            Constraint::Min(1),
        ])
        .split(area);
    frame.render_widget(
        Paragraph::new(header).block(
            Block::default()
                .borders(Borders::ALL)
                .title(app.t("appearance"))
                .border_style(Style::default().fg(pal.border)),
        ),
        split[0],
    );

    let tab_labels = vec![
        app.tt("1 Repositories", "1 リポジトリ"),
        app.tt("2 Members (Committers)", "2 メンバー管理"),
    ];
    app.settings_viewport.set(ListViewport::default());
    let tab_inner = split[1].inner(Margin::new(1, 1));
    app.settings_tab_row.set(tab_inner.y);
    let mut x = tab_inner.x;
    let mut bounds = Vec::new();
    for label in &tab_labels {
        let w = label.width() as u16;
        x = x.saturating_add(1);
        bounds.push((x, x.saturating_add(w)));
        x = x.saturating_add(w + 1 + 1); // right padding and divider
    }
    *app.settings_tab_bounds.borrow_mut() = bounds;
    let tab_idx = match app.settings_tab {
        crate::app::SettingsTab::Repositories => 0,
        crate::app::SettingsTab::Members => 1,
    };
    let tabs_w = Tabs::new(tab_labels)
        .select(tab_idx)
        .highlight_style(Style::default().fg(pal.accent).add_modifier(Modifier::BOLD))
        .style(Style::default().fg(pal.subtext))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(pal.border)),
        );
    frame.render_widget(tabs_w, split[1]);

    match app.settings_tab {
        crate::app::SettingsTab::Repositories => {
            let items: Vec<ListItem> = app
                .repos
                .iter()
                .map(|r| {
                    let group_span = if let Some(g) = &r.group {
                        let trimmed = g.trim();
                        if !trimmed.is_empty() {
                            Span::styled(format!("[{trimmed}] "), Style::default().fg(pal.yellow))
                        } else {
                            Span::raw("")
                        }
                    } else {
                        Span::raw("")
                    };
                    ListItem::new(Line::from(vec![
                        group_span,
                        Span::styled(format!("{}  ", r.name), Style::default().fg(pal.text)),
                        Span::styled(r.path.display().to_string(), Style::default().fg(pal.muted)),
                    ]))
                })
                .collect();
            app.settings_viewport.set(render_items(
                frame,
                split[2],
                pal,
                items,
                app.settings_selected,
                app.tt(
                    "Repositories (a: add, A: bulk add, g: group, e: alias, d: delete)",
                    "リポジトリ (a: 追加, A: 一括追加, g: グループ, e: 別名, d: 削除)",
                ),
                None,
                &app.tt(
                    "No repositories. Press a to add, A to bulk add.",
                    "リポジトリなし。a で追加、A で一括追加。",
                ),
            ));
        }
        crate::app::SettingsTab::Members => {
            let items: Vec<ListItem> = app
                .members()
                .iter()
                .map(|m| {
                    let mark = if m.is_active {
                        Span::styled(
                            "[✓] ",
                            Style::default().fg(pal.green).add_modifier(Modifier::BOLD),
                        )
                    } else {
                        Span::styled("[ ] ", Style::default().fg(pal.muted))
                    };
                    let name_style = if m.is_active {
                        Style::default().fg(pal.text).add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(pal.muted)
                    };
                    let alias_str = if m.aliases.is_empty() {
                        String::new()
                    } else {
                        format!(" ({})", m.aliases.join(", "))
                    };
                    ListItem::new(Line::from(vec![
                        mark,
                        Span::styled(m.canonical_name.clone(), name_style),
                        Span::styled(alias_str, Style::default().fg(pal.subtext)),
                    ]))
                })
                .collect();
            app.settings_viewport.set(render_items(
                frame,
                split[2],
                pal,
                items,
                app.settings_member_selected,
                app.tt(
                    "Members (Space: toggle active, a: add, e: edit aliases, d: delete)",
                    "メンバー管理 (Space: 在籍切替, a: 追加, e: 別名編集, d: 削除)",
                ),
                None,
                &app.tt(
                    "No members registered. Press a to add member.",
                    "メンバーが登録されていません。a で追加。",
                ),
            ));
        }
    }
}

fn draw_log(frame: &mut Frame, app: &App, area: Rect, pal: Palette) {
    let Some(log) = app.log.as_ref() else {
        return;
    };
    let lines: Vec<Line> = log
        .body
        .lines()
        .skip(log.scroll)
        .map(|l| Line::from(l.to_string()))
        .collect();
    frame.render_widget(
        Paragraph::new(lines).block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!("{}  (esc back)", log.title))
                .border_style(Style::default().fg(pal.accent))
                .title_style(Style::default().fg(pal.accent)),
        ),
        area,
    );
}

fn draw_commit_search(frame: &mut Frame, app: &App, area: Rect, pal: Palette) {
    app.commit_search_viewport.set(ListViewport::default());
    let Some(search) = app.commit_search.as_ref() else {
        frame.render_widget(
            Paragraph::new(app.tt(
                "Press / to search commit messages across all repositories.",
                "/ で全リポジトリ横断のコミットメッセージ検索を開始します。",
            ))
            .style(Style::default().fg(pal.subtext))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(pal.border)),
            ),
            area,
        );
        return;
    };
    if search.loading && search.hits.is_empty() {
        frame.render_widget(
            Paragraph::new(format!(
                "{} {}",
                app.spinner(),
                app.tt("Searching commits...", "コミットを検索中...")
            ))
            .style(Style::default().fg(pal.yellow))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(format!("\"{}\"", search.query))
                    .border_style(Style::default().fg(pal.border)),
            ),
            area,
        );
        return;
    }
    let items: Vec<ListItem> = search
        .hits
        .iter()
        .map(|h| {
            let spans = vec![
                Span::styled(
                    format!("[{}] ", truncate(&h.repo_name, 16)),
                    Style::default().fg(pal.yellow),
                ),
                Span::styled(h.hash.clone(), Style::default().fg(pal.accent)),
                Span::styled(format!("  {}  ", h.date), Style::default().fg(pal.muted)),
                Span::styled(
                    format!("{}  ", truncate(&h.author, 16)),
                    Style::default().fg(pal.subtext),
                ),
                Span::raw(h.message.clone()),
            ];
            ListItem::new(Line::from(spans))
        })
        .collect();
    let title = format!(
        "{} \"{}\" ({})",
        app.tt("Commit Search", "コミット検索"),
        search.query,
        search.hits.len()
    );
    let viewport = render_items(
        frame,
        area,
        pal,
        items,
        search.selected,
        title,
        None,
        &app.tt(
            "No matching commits in any repository.",
            "一致するコミットはどのリポジトリにもありません。",
        ),
    );
    app.commit_search_viewport.set(viewport);
}

fn draw_global_members(frame: &mut Frame, app: &App, area: Rect, pal: Palette) {
    app.global_members_viewport.set(ListViewport::default());
    app.global_member_repos_viewport
        .set(ListViewport::default());
    // The aggregation walks git log in every repository on a worker thread, so
    // the screen opens before any results exist.
    if app.global_members_loading && app.global_members.is_empty() {
        frame.render_widget(
            Paragraph::new(format!(
                "{} {}",
                app.spinner(),
                app.tt("Aggregating members...", "メンバーを集計しています...")
            ))
            .style(Style::default().fg(pal.subtext))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(pal.border)),
            ),
            area,
        );
        return;
    }
    let split = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(45), Constraint::Percentage(55)])
        .split(area);

    let vis = app.filtered_global_members();
    let left_border = if app.global_member_pane == FocusPane::List {
        pal.accent
    } else {
        pal.border
    };
    let right_border = if app.global_member_pane == FocusPane::Content {
        pal.accent
    } else {
        pal.border
    };

    // Left pane: Member list
    let member_items: Vec<ListItem> = vis
        .iter()
        .map(|&i| {
            let m = &app.global_members[i];
            let mark = if m.is_active {
                Span::styled(
                    "[✓] ",
                    Style::default().fg(pal.green).add_modifier(Modifier::BOLD),
                )
            } else {
                Span::styled("[ ] ", Style::default().fg(pal.muted))
            };
            let name_style = if m.is_active {
                Style::default().fg(pal.text).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(pal.muted)
            };
            let stats = format!(" ({} repos, {} commits)", m.repo_count, m.total_commits);
            let last = if m.latest_commit_date.is_empty() {
                String::new()
            } else {
                format!("  last: {}", m.latest_commit_date)
            };
            ListItem::new(Line::from(vec![
                mark,
                Span::styled(truncate(&m.canonical_name, 16), name_style),
                Span::styled(stats, Style::default().fg(pal.accent)),
                Span::styled(last, Style::default().fg(pal.subtext)),
            ]))
        })
        .collect();

    let filter_str = if !app.global_member_filter.is_empty() {
        format!(" / {}", app.global_member_filter)
    } else {
        String::new()
    };
    let active_str = if app.active_only {
        format!(" {}", app.tt("[active only]", "[在籍のみ]"))
    } else {
        String::new()
    };
    let left_title = format!(
        "{} ({}/{}){active_str}{filter_str}",
        app.tt("Members", "メンバー一覧"),
        vis.len(),
        app.global_members.len(),
    );

    let left_list = List::new(member_items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(left_border))
                .title(left_title)
                .title_style(Style::default().fg(pal.accent)),
        )
        .highlight_style(
            Style::default()
                .bg(pal.overlay)
                .fg(pal.text)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▸ ");

    let mut left_state = ListState::default();
    if !vis.is_empty() {
        left_state.select(Some(app.global_member_selected.min(vis.len() - 1)));
    }
    frame.render_stateful_widget(left_list, split[0], &mut left_state);
    let left_inner = split[0].inner(Margin::new(1, 1));
    app.global_members_viewport.set(ListViewport {
        y: left_inner.y,
        height: left_inner.height,
        x: left_inner.x,
        width: left_inner.width,
        offset: left_state.offset(),
    });

    // Right pane: Repositories of selected member
    let selected_member = vis
        .get(app.global_member_selected)
        .and_then(|&i| app.global_members.get(i));

    if let Some(m) = selected_member {
        let repo_rows: Vec<Row> = m
            .contributions
            .iter()
            .map(|c| {
                Row::new(vec![
                    Cell::from(Span::styled(
                        c.repo_name.clone(),
                        Style::default().fg(pal.accent).add_modifier(Modifier::BOLD),
                    )),
                    Cell::from(Span::styled(
                        format!("{:>5}", c.commit_count),
                        Style::default().fg(pal.yellow),
                    )),
                    Cell::from(Span::styled(
                        format!("{} ~ {}", c.first_commit, c.last_commit),
                        Style::default().fg(pal.subtext),
                    )),
                    Cell::from(Span::styled(
                        c.repo_path.display().to_string(),
                        Style::default().fg(pal.muted),
                    )),
                ])
            })
            .collect();

        let header = Row::new(vec![
            app.tt("Repository", "リポジトリ"),
            app.tt("Commits", "コミット数"),
            app.tt("Activity Period", "活動期間"),
            app.tt("Path", "パス"),
        ])
        .style(
            Style::default()
                .fg(pal.subtext)
                .add_modifier(Modifier::BOLD),
        );

        let right_title = format!(
            "{} - {} ({} repos, {} commits)  [Enter: Open Repo]",
            app.tt("Involved Repositories", "担当・関与リポジトリ"),
            m.canonical_name,
            m.repo_count,
            m.total_commits
        );

        let table = Table::new(
            repo_rows,
            [
                Constraint::Length(22),
                Constraint::Length(10),
                Constraint::Length(24),
                Constraint::Min(10),
            ],
        )
        .header(header)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(right_border))
                .title(right_title)
                .title_style(Style::default().fg(pal.accent)),
        )
        .row_highlight_style(
            Style::default()
                .bg(pal.overlay)
                .fg(pal.text)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▸ ")
        .column_spacing(1);

        let mut right_state = TableState::default();
        if !m.contributions.is_empty() {
            right_state.select(Some(
                app.global_member_repo_selected
                    .min(m.contributions.len() - 1),
            ));
        }
        frame.render_stateful_widget(table, split[1], &mut right_state);
        let inner = split[1].inner(Margin::new(1, 1));
        // The first inner row is the table header; clicks and offsets refer
        // only to data rows below it.
        app.global_member_repos_viewport.set(ListViewport {
            y: inner.y.saturating_add(1),
            height: inner.height.saturating_sub(1),
            x: inner.x,
            width: inner.width,
            offset: right_state.offset(),
        });
    } else {
        let empty_msg =
            Paragraph::new(app.tt("No member selected.", "メンバーが選択されていません。"))
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_style(Style::default().fg(right_border))
                        .title(app.tt("Involved Repositories", "担当・関与リポジトリ")),
                )
                .style(Style::default().fg(pal.muted));
        frame.render_widget(empty_msg, split[1]);
    }
}

fn draw_repo_finder(frame: &mut Frame, app: &App, area: Rect, pal: Palette) {
    let Some(finder) = &app.repo_finder else {
        return;
    };
    app.finder_viewport.set(ListViewport::default());
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(5), Constraint::Min(1)])
        .split(area);

    let vis = app.filtered_finder_repos();
    let total_found = finder.repos.len();
    let already_added = finder.repos.iter().filter(|r| r.is_already_added).count();
    let newly_found = total_found.saturating_sub(already_added);
    let selected_count = finder
        .repos
        .iter()
        .filter(|r| r.is_selected && !r.is_already_added)
        .count();

    let scan_info = vec![
        Line::from(if finder.loading {
            app.tt(
                "Scanning… Esc: cancel / Enter: import found repositories",
                "走査中… Esc: 中止 / Enter: 検出済みを登録",
            )
        } else if let Some(error) = finder.errors.first() {
            format!(
                "{} ({}): {error}",
                app.tt("Scan errors", "走査エラー"),
                finder.errors.len()
            )
        } else {
            app.tt("Scan complete", "走査完了")
        }),
        Line::from(vec![
            Span::styled(
                format!("  {:<14}", app.tt("Scan Root:", "探索フォルダ:")),
                Style::default().fg(pal.muted),
            ),
            Span::styled(
                finder.scan_root.display().to_string(),
                Style::default().fg(pal.accent).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                app.tt("  (press 'r' to change)", "  ('r' でフォルダ変更)"),
                Style::default().fg(pal.muted),
            ),
        ]),
        Line::from(vec![
            Span::styled(
                format!("  {:<14}", app.tt("Found Repos:", "検出リポジトリ:")),
                Style::default().fg(pal.muted),
            ),
            Span::styled(
                format!("{total_found} repos"),
                Style::default().fg(pal.text),
            ),
            Span::styled(
                format!("  ({newly_found} new, {already_added} already registered)"),
                Style::default().fg(pal.subtext),
            ),
            Span::styled(
                format!("    {:<12}", app.tt("Selected:", "選択中:")),
                Style::default().fg(pal.muted),
            ),
            Span::styled(
                format!("{selected_count} repos"),
                Style::default().fg(pal.green).add_modifier(Modifier::BOLD),
            ),
        ]),
    ];
    frame.render_widget(
        Paragraph::new(scan_info).block(
            Block::default()
                .borders(Borders::ALL)
                .title(app.tt("Scan Info", "探索情報"))
                .border_style(Style::default().fg(pal.border))
                .title_style(Style::default().fg(pal.accent)),
        ),
        chunks[0],
    );

    let items: Vec<ListItem> = vis
        .iter()
        .map(|&idx| {
            let r = &finder.repos[idx];
            let (check_str, check_style) = if r.is_already_added {
                ("[Added] ", Style::default().fg(pal.muted))
            } else if r.is_selected {
                (
                    "[x] ",
                    Style::default().fg(pal.green).add_modifier(Modifier::BOLD),
                )
            } else {
                ("[ ] ", Style::default().fg(pal.muted))
            };

            let name_style = if r.is_already_added {
                Style::default().fg(pal.muted)
            } else {
                Style::default().fg(pal.accent).add_modifier(Modifier::BOLD)
            };

            let spans = vec![
                Span::styled(check_str, check_style),
                Span::styled(truncate(&r.name, 24), name_style),
                Span::styled(
                    format!(" ({})  ", r.branch),
                    Style::default().fg(pal.yellow),
                ),
                Span::styled(
                    r.path.display().to_string(),
                    Style::default().fg(pal.subtext),
                ),
            ];
            ListItem::new(Line::from(spans))
        })
        .collect();

    let list_title = if finder.filter.is_empty() {
        app.tt(
            "Found Repositories  (Space: toggle, a: select all, Enter: import)",
            "検出されたリポジトリ  (Space: 選択, a: 全選択, Enter: 登録)",
        )
    } else {
        format!(
            "{}  (filter: {})",
            app.tt("Found Repositories", "検出されたリポジトリ"),
            finder.filter
        )
    };

    app.finder_viewport.set(render_items(
        frame,
        chunks[1],
        pal,
        items,
        finder.selected_idx,
        list_title,
        None,
        &if finder.loading {
            app.tt("Discovering repositories…", "リポジトリを検出中…")
        } else {
            app.tt(
            "No git repositories found in this folder. Press 'r' to scan another folder.",
            "このフォルダ内に Git リポジトリは見つかりませんでした。'r' で別フォルダをスキャンしてください。",
        )
        },
    ));
}

/// One block of the help: the screens it answers for, its heading, and its
/// key rows.
struct HelpSection {
    /// Screens this block is the primary answer for. The tools menu claims
    /// none — its popup cannot be open at the same time as the help.
    screens: &'static [Screen],
    title: String,
    rows: Vec<Line<'static>>,
}

/// One key column for every row, so the descriptions still line up once they
/// are twice as wide in Japanese.
fn help_row(key: &str, text: String) -> Line<'static> {
    Line::from(format!("  {key:<14} {text}"))
}

fn help_head(text: String, pal: Palette) -> Line<'static> {
    Line::from(Span::styled(
        text,
        Style::default().fg(pal.accent).add_modifier(Modifier::BOLD),
    ))
}

fn help_note(text: String, pal: Palette) -> Line<'static> {
    Line::from(Span::styled(text, Style::default().fg(pal.subtext)))
}

fn push_help_section(lines: &mut Vec<Line<'static>>, section: &HelpSection, pal: Palette) {
    lines.push(help_head(section.title.clone(), pal));
    lines.extend(section.rows.iter().cloned());
    lines.push(Line::from(""));
}

/// Keys that work on every screen, shown right below the keys for the screen
/// the help was opened from.
fn help_global_section(app: &App) -> HelpSection {
    HelpSection {
        screens: &[],
        title: app.tt("Global", "全体"),
        rows: vec![
            help_row("q / Ctrl+C", app.tt("quit", "終了")),
            help_row(
                "Esc / h",
                app.tt(
                    "back one screen (← too), or clear the filter",
                    "1つ前の画面へ戻る（←も可）。絞り込み中は解除",
                ),
            ),
            help_row(
                "?",
                app.tt(
                    "toggle this help (returns here)",
                    "このヘルプの表示切替（元の画面に戻る）",
                ),
            ),
            help_row("/", app.tt("filter current list", "現在の一覧を絞り込む")),
            help_row(
                "j / k",
                app.tt("move selection (↓ / ↑ too)", "選択を移動（↓ / ↑ も可）"),
            ),
            help_row("g / G", app.tt("first / last", "先頭 / 末尾へ移動")),
            help_row(
                "t / T",
                app.tt(
                    "open terminal in repo directory",
                    "リポジトリのディレクトリでターミナルを開く",
                ),
            ),
            help_row(
                "wheel / click",
                app.tt(
                    "scroll and select; title bar: Back / Home / Navigate",
                    "スクロール・選択。タイトルバー: Back / Home / Navigate",
                ),
            ),
        ],
    }
}

/// Every per-screen block, in the order they are listed once the current
/// screen has been pulled to the top.
fn help_sections(app: &App) -> Vec<HelpSection> {
    vec![
        HelpSection {
            screens: &[Screen::Home],
            title: app.t("repositories"),
            rows: vec![
                help_row(
                    "j k",
                    app.tt(
                        "move    enter open    a/d add/delete",
                        "移動    enter 開く    a/d 追加/削除",
                    ),
                ),
                help_row(
                    "A",
                    app.tt(
                        "scan folder and bulk import git repositories",
                        "フォルダを走査して git リポジトリを一括登録",
                    ),
                ),
                help_row(
                    "[ / ]",
                    app.tt(
                        "switch repository group filter",
                        "リポジトリのグループフィルタを切り替え",
                    ),
                ),
                help_row(
                    "P / F",
                    app.tt(
                        "bulk pull / bulk fetch all filtered repos",
                        "絞り込み中の全リポジトリへ一括 pull / fetch",
                    ),
                ),
                help_row(
                    "p / f",
                    app.tt(
                        "pull / fetch single repo",
                        "選択中のリポジトリを pull / fetch",
                    ),
                ),
                help_row(
                    "t",
                    app.tt(
                        "open terminal in repository",
                        "リポジトリでターミナルを開く",
                    ),
                ),
                help_row(
                    "M",
                    app.tt(
                        "open Global Members view (cross-repo)",
                        "全リポジトリ横断のメンバー画面を開く",
                    ),
                ),
                help_row(
                    "S",
                    app.tt(
                        "search commit messages across all repositories",
                        "全リポジトリのコミットメッセージを検索",
                    ),
                ),
                help_row(
                    "n",
                    app.tt(
                        "toggle: only repos needing attention (failing CI,",
                        "要対応のみ表示（CI 失敗・未解決のコンフリクト・",
                    ),
                ),
                help_row(
                    "",
                    app.tt(
                        "unresolved conflict, or a mid-operation merge/rebase)",
                        "中断中の merge/rebase）の切替",
                    ),
                ),
                help_row(
                    "W",
                    app.tt(
                        "cross-repository local worktrees (Home)",
                        "ローカルWorktreeを横断表示（Home）",
                    ),
                ),
                help_row(
                    "O",
                    app.tt(
                        "open shell/editor/lazygit/GitUI menu (Home, Repo, Diff)",
                        "シェル・エディタ・lazygit・GitUIメニュー（Home・詳細・diff）",
                    ),
                ),
                help_row(
                    "C",
                    app.tt(
                        "open the selected repository's latest CI run (Home)",
                        "選択リポジトリ全体の最新CI実行を開く（Home）",
                    ),
                ),
                help_row(
                    "o / e",
                    app.tt("sort repos / rename alias", "並び替え / 表示名の変更"),
                ),
                help_row(
                    "click header",
                    app.tt(
                        "sort by that column; click again to reverse",
                        "その列で並び替え。再クリックで昇降反転",
                    ),
                ),
                help_row(
                    "r / s",
                    app.tt("reload / settings", "再読み込み / 設定画面"),
                ),
            ],
        },
        HelpSection {
            screens: &[Screen::Repo],
            title: app.t("repo_detail"),
            rows: vec![
                Line::from(app.tt(
                    "  1 Status  2 Commits  3 Branches  4 Tags  5 Stash  6 Contributors  7 Worktrees",
                    "  1 状態  2 コミット  3 ブランチ  4 タグ  5 Stash  6 貢献者  7 ワークツリー",
                )),
                help_row(
                    "Tab / [ / ]",
                    app.tt(
                        "previous / next tab (1 to 7 jump straight to one)",
                        "前 / 次のタブへ移動（1〜7 で直接移動）",
                    ),
                ),
                help_row(
                    "enter",
                    app.tt(
                        "commit/file/stash diff, branch log, or shell in Worktree",
                        "コミット/ファイル/stash の差分、ブランチのログ、Worktree でシェル",
                    ),
                ),
                help_row(
                    "p / f",
                    app.tt(
                        "pull / fetch this repository",
                        "このリポジトリを pull / fetch",
                    ),
                ),
                help_row(
                    "space",
                    app.tt(
                        "mark commit/tag base+target, or toggle active in Contributors",
                        "コミット/タグの比較の基準・対象を選択、貢献者タブでは在籍切替",
                    ),
                ),
                help_row(
                    "a / d",
                    app.tt(
                        "apply / drop the selected stash (Stash tab)",
                        "選択中の stash を適用 / 破棄（Stash タブ）",
                    ),
                ),
                help_row(
                    "click [ ]",
                    app.tt(
                        "same as space: pick the compare base, then the target",
                        "space と同じ。比較の基準→対象を選択（再クリックで解除）",
                    ),
                ),
                help_row(
                    "click a row",
                    app.tt(
                        "select it; click the selected row again to open it",
                        "選択。選択済みの行をもう一度クリックすると開く",
                    ),
                ),
                help_row(
                    "w",
                    app.tt(
                        "cycle time span filter (All / 1w / 1m / 3m)",
                        "集計期間を切替（全期間 / 1週 / 1月 / 3月）",
                    ),
                ),
                help_row(
                    "m",
                    app.tt(
                        "filter active members only (in Contributors tab)",
                        "在籍メンバーのみ表示（貢献者タブ）",
                    ),
                ),
                help_row(
                    "i",
                    app.tt(
                        "always open builtin TUI diff",
                        "常に内蔵の TUI 差分ビューアで開く",
                    ),
                ),
                help_row(
                    "c",
                    app.tt(
                        "set external diff (empty = builtin, e.g. hunk)",
                        "外部 diff ツールを設定（空欄で内蔵、例: hunk）",
                    ),
                ),
                help_row(
                    "r",
                    app.tt(
                        "reload without leaving the tab",
                        "タブを移動せずに再読み込み",
                    ),
                ),
            ],
        },
        HelpSection {
            screens: &[Screen::Diff],
            title: app.t("diff"),
            rows: vec![
                help_row(
                    "Tab / h l",
                    app.tt("files ↔ hunks ↔ diff", "ファイル ↔ ハンク ↔ 差分 の移動"),
                ),
                help_row(
                    "n / p",
                    app.tt(
                        "next/prev hunk (wraps; highlights current)",
                        "次/前のハンク（末尾で先頭へ、現在位置を強調）",
                    ),
                ),
                help_row(
                    "[ / ]",
                    app.tt("previous/next changed file", "前/次の変更ファイル"),
                ),
                help_row(
                    "space / PgDn",
                    app.tt(
                        "scroll the diff a page (PgUp scrolls back)",
                        "差分を1画面分スクロール（PgUp で戻る）",
                    ),
                ),
                help_row(
                    "enter",
                    app.tt(
                        "load the file (Files pane) or jump to the hunk (Hunks pane)",
                        "ファイルを読み込む（ファイル欄）/ ハンクへ移動（ハンク欄）",
                    ),
                ),
                help_row(
                    "w",
                    app.tt("toggle ignore-whitespace", "空白差分を無視する切替"),
                ),
                help_row(
                    "f",
                    app.tt("toggle full-file context", "ファイル全体を表示する切替"),
                ),
                help_row(
                    "b",
                    app.tt(
                        "toggle blame gutter (hash + author per line)",
                        "blame 表示の切替（行ごとのハッシュと作者）",
                    ),
                ),
            ],
        },
        HelpSection {
            screens: &[Screen::Settings],
            title: app.tt("Settings", "設定画面"),
            rows: vec![
                help_row(
                    "Tab / 1 / 2",
                    app.tt(
                        "switch between Repositories and Members tabs",
                        "リポジトリ / メンバー管理タブを切替",
                    ),
                ),
                help_row(
                    "g",
                    app.tt("edit repository group", "リポジトリのグループを編集"),
                ),
                help_row(
                    "space / t",
                    app.tt(
                        "toggle member active / inactive",
                        "メンバーの在籍/非在籍を切替",
                    ),
                ),
                help_row(
                    "a / e / d",
                    app.tt(
                        "add / edit aliases / delete member or repo",
                        "追加 / 別名の編集 / メンバー・リポジトリの削除",
                    ),
                ),
                help_row(
                    "A",
                    app.tt(
                        "open the repository finder (Repositories tab)",
                        "リポジトリ検出を開く（リポジトリタブ）",
                    ),
                ),
                help_row(
                    "l / c",
                    app.tt(
                        "toggle language / change diff tool",
                        "表示言語の切替 / diff ツールの変更",
                    ),
                ),
                help_row(
                    "i",
                    app.tt(
                        "cycle Home auto-refresh interval (off/30s/1m/5m)",
                        "Home の自動更新間隔を切替（オフ/30秒/1分/5分）",
                    ),
                ),
                help_row(
                    "T",
                    app.tt(
                        "toggle theme (Catppuccin Mocha / Latte)",
                        "テーマを切替（Catppuccin Mocha / Latte）",
                    ),
                ),
            ],
        },
        HelpSection {
            screens: &[Screen::GlobalMembers],
            title: app.t("global_members"),
            rows: vec![
                help_row(
                    "Tab / h / l",
                    app.tt(
                        "switch pane between members and repo list",
                        "メンバー一覧とリポジトリ一覧のペインを切替",
                    ),
                ),
                help_row(
                    "Enter",
                    app.tt(
                        "jump directly into selected repository",
                        "選択したリポジトリへ直接移動",
                    ),
                ),
                help_row(
                    "space / t",
                    app.tt(
                        "toggle member active / inactive",
                        "メンバーの在籍/非在籍を切替",
                    ),
                ),
                help_row(
                    "m",
                    app.tt("filter active members only", "在籍メンバーのみ表示"),
                ),
                help_row(
                    "/",
                    app.tt(
                        "search members or repositories",
                        "メンバー / リポジトリを検索",
                    ),
                ),
            ],
        },
        HelpSection {
            screens: &[Screen::Workspace],
            title: app.tt(
                "Worktrees across repositories (Home W)",
                "Worktree横断一覧（Home W）",
            ),
            rows: vec![
                help_row(
                    "/",
                    app.tt(
                        "search paths, branches and notes",
                        "パス・ブランチ・用途メモを検索",
                    ),
                ),
                help_row("m", app.tt("edit the purpose note", "用途メモを編集")),
                help_row(
                    "* / f",
                    app.tt(
                        "toggle favorite / show favorites only",
                        "お気に入り切替 / お気に入りのみ表示",
                    ),
                ),
                help_row(
                    "Enter / O",
                    app.tt(
                        "tools menu for the selected worktree",
                        "選択中のWorktreeのツールメニュー",
                    ),
                ),
                help_row(
                    "t",
                    app.tt(
                        "shell in the selected worktree",
                        "選択中のWorktreeでシェルを開く",
                    ),
                ),
                help_row(
                    "r",
                    app.tt("refresh in the background", "バックグラウンドで再取得"),
                ),
                help_row(
                    "[ / ]",
                    app.tt(
                        "previous / next problem — why a row reads unknown",
                        "前 / 次の取得失敗（状態不明の行の理由）",
                    ),
                ),
            ],
        },
        HelpSection {
            screens: &[Screen::CommitSearch],
            title: app.tt("Commit search (Home S)", "コミット検索（Home S）"),
            rows: vec![
                help_row("/", app.tt("start a new search", "新しい検索を開始")),
                help_row(
                    "Enter",
                    app.tt(
                        "open the hit's diff inside its repository",
                        "ヒットしたコミットの差分をそのリポジトリで開く",
                    ),
                ),
                help_row(
                    "click a hit",
                    app.tt(
                        "select it; click the selected hit again to open it",
                        "選択。選択済みの行をもう一度クリックすると開く",
                    ),
                ),
            ],
        },
        HelpSection {
            screens: &[Screen::RepoFinder],
            title: app.tt("Repository finder (Home A)", "リポジトリ検出（Home A）"),
            rows: vec![
                help_row(
                    "space",
                    app.tt(
                        "toggle import selection for this row",
                        "この行の取り込み選択を切替",
                    ),
                ),
                help_row(
                    "a",
                    app.tt("select / deselect every row", "すべて選択 / 解除"),
                ),
                help_row(
                    "Enter",
                    app.tt(
                        "import the selected repositories",
                        "選択したリポジトリを取り込む",
                    ),
                ),
                help_row("r", app.tt("scan a different folder", "別のフォルダを走査")),
                help_row(
                    "/",
                    app.tt(
                        "filter the discovered repositories",
                        "検出されたリポジトリを絞り込む",
                    ),
                ),
                help_row(
                    "click [ ]",
                    app.tt(
                        "toggle that row's import selection",
                        "その行の取り込み選択を切替",
                    ),
                ),
                help_row("Esc / q", app.tt("cancel and go back", "取り消して戻る")),
            ],
        },
        HelpSection {
            screens: &[Screen::Log],
            title: app.tt("Branch log", "ブランチログ"),
            rows: vec![
                help_row(
                    "space / PgDn",
                    app.tt(
                        "scroll a page down (PgUp scrolls back)",
                        "1画面分スクロール（PgUp で戻る）",
                    ),
                ),
                help_row(
                    "Esc / h",
                    app.tt("back to the repository", "リポジトリ画面へ戻る"),
                ),
            ],
        },
        HelpSection {
            screens: &[],
            title: app.tt("Tools menu (O)", "ツールメニュー（O）"),
            rows: vec![
                help_row(
                    "t / e / l / g",
                    app.tt(
                        "shell / editor / lazygit / GitUI",
                        "シェル / エディタ / lazygit / GitUI",
                    ),
                ),
                help_row(
                    "c",
                    app.tt(
                        "set the editor command (default: code)",
                        "エディタのコマンドを設定（既定: code）",
                    ),
                ),
                help_row(
                    "w",
                    app.tt(
                        "wait for the editor (on for terminal editors)",
                        "エディタの終了を待つ（ターミナル用エディタで有効に）",
                    ),
                ),
                help_row("Esc / q", app.tt("close the menu", "メニューを閉じる")),
            ],
        },
    ]
}

fn draw_help(frame: &mut Frame, app: &App, area: Rect, pal: Palette) {
    // One flat list starting at "Global" pushed the keys for the screen the
    // user was actually on below the fold of any ordinary terminal, behind
    // four sections they had not asked about. Lead with that screen instead,
    // then the keys that work everywhere, then the rest — still one
    // scrollable list, so nothing becomes unreachable.
    let context = app.help_context();
    let sections = help_sections(app);
    let here = sections.iter().find(|s| s.screens.contains(&context));
    let context_name = here.map_or_else(|| app.tt("Global", "全体"), |s| s.title.clone());
    let mut lines = vec![
        help_head(
            format!("{} {}", app.tt("Keys for:", "この画面:"), context_name),
            pal,
        ),
        help_note(
            app.tt(
                "global keys and other screens follow — j/k, g/G, wheel to scroll",
                "この下に全画面共通のキーとほかの画面のキー。j/k・g/G・ホイールでスクロール",
            ),
            pal,
        ),
        Line::from(""),
    ];
    if let Some(section) = here {
        push_help_section(&mut lines, section, pal);
    }
    push_help_section(&mut lines, &help_global_section(app), pal);
    lines.push(help_note(
        app.tt("── Other screens ──", "── ほかの画面 ──"),
        pal,
    ));
    lines.push(Line::from(""));
    for section in sections.iter().filter(|s| !s.screens.contains(&context)) {
        push_help_section(&mut lines, section, pal);
    }

    // The help is the discoverability backstop, and it is longer than any
    // ordinary terminal. Scroll it, and say so in the title.
    let inner_h = area.height.saturating_sub(2) as usize;
    let max_scroll = lines.len().saturating_sub(inner_h);
    let scroll = app.help_scroll.get().min(max_scroll);
    app.help_scroll.set(scroll);
    let heading = format!("{} — {}", app.tt("Help", "ヘルプ"), context_name);
    let title = if max_scroll == 0 {
        heading
    } else {
        format!(
            "{heading} ({}-{}/{})  {}",
            scroll + 1,
            (scroll + inner_h).min(lines.len()),
            lines.len(),
            app.tt("j/k or wheel to scroll", "j/k・ホイールでスクロール")
        )
    };
    frame.render_widget(
        Paragraph::new(lines).scroll((scroll as u16, 0)).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(pal.border))
                .title(title)
                .title_style(Style::default().fg(pal.accent)),
        ),
        area,
    );
}

/// A dialog box centred in `area`, never larger than the area itself.
/// Clamping the width up to `min_w` on a narrower terminal produced a Rect
/// extending past the frame, and `Clear` indexes the buffer directly — that
/// is an out-of-bounds panic, on every redraw, with the dialog open.
fn centered_rect(area: Rect, min_w: u16, max_w: u16, h: u16) -> Rect {
    let w = area.width.clamp(min_w, max_w).min(area.width);
    let h = h.min(area.height);
    let x = area.x + area.width.saturating_sub(w) / 2;
    let y = area.y + area.height.saturating_sub(h) / 2;
    Rect::new(x, y, w, h)
}

fn draw_prompt(frame: &mut Frame, area: Rect, title: &str, value: &str, pal: Palette) {
    let rect = centered_rect(area, 20, 80, 5);
    frame.render_widget(Clear, rect);
    let body = if value.is_empty() {
        String::new()
    } else {
        format!("{value}█")
    };
    frame.render_widget(
        Paragraph::new(body).block(
            Block::default()
                .borders(Borders::ALL)
                .title(title)
                .border_style(Style::default().fg(pal.accent))
                .style(Style::default().bg(pal.surface).fg(pal.text)),
        ),
        rect,
    );
}

fn render_path_list(
    frame: &mut Frame,
    app: &App,
    area: Rect,
    pal: Palette,
    title: String,
    empty: String,
) {
    let Some(data) = app.repo_data.as_ref() else {
        return;
    };
    let vis = app.visible_indices();
    let items: Vec<ListItem> = vis
        .iter()
        .map(|&i| {
            let f = &data.working_files[i];
            ListItem::new(format!(
                "[{}] {:+}/-{}  {}",
                f.status, f.additions, f.deletions, f.path
            ))
            .style(status_style(&f.status, pal))
        })
        .collect();
    app.list_viewport.set(render_items(
        frame,
        area,
        pal,
        items,
        app.list_selected,
        title,
        app.list_error(),
        &empty,
    ));
}

/// Returns where the list ended up on screen, for callers that need to map a
/// mouse click back to an item. Callers that don't simply drop it — recording
/// it inside here instead would mean whichever list happened to draw last won,
/// and the Diff and Settings screens draw lists too.
#[allow(clippy::too_many_arguments)]
fn render_items(
    frame: &mut Frame,
    area: Rect,
    pal: Palette,
    items: Vec<ListItem>,
    selected: usize,
    title: String,
    error: Option<&str>,
    empty: &str,
) -> ListViewport {
    if let Some(err) = error {
        frame.render_widget(
            Paragraph::new(err)
                .style(Style::default().fg(pal.red))
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(title)
                        .border_style(Style::default().fg(pal.red)),
                ),
            area,
        );
        return ListViewport::default();
    }
    if items.is_empty() {
        frame.render_widget(
            Paragraph::new(empty)
                .style(Style::default().fg(pal.muted))
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(title)
                        .border_style(Style::default().fg(pal.border))
                        .title_style(Style::default().fg(pal.accent)),
                ),
            area,
        );
        return ListViewport::default();
    }
    let n = items.len();
    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(title)
                .border_style(Style::default().fg(pal.border))
                .title_style(Style::default().fg(pal.accent)),
        )
        .highlight_style(
            Style::default()
                .bg(pal.overlay)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol(HIGHLIGHT_SYMBOL);
    let mut state = ListState::default();
    state.select(Some(selected.min(n.saturating_sub(1))));
    frame.render_stateful_widget(list, area, &mut state);
    // Read the offset back *after* rendering: ratatui picks it while drawing,
    // to keep the selection on screen. Computing it here instead would be a
    // second implementation of that scrolling rule, free to disagree.
    let inner = area.inner(Margin::new(1, 1));
    ListViewport {
        y: inner.y,
        height: inner.height,
        x: inner.x,
        width: inner.width,
        offset: state.offset(),
    }
}

fn status_style(status: &str, pal: Palette) -> Style {
    match status {
        "A" => Style::default().fg(pal.green),
        "D" => Style::default().fg(pal.red),
        "M" => Style::default().fg(pal.yellow),
        "R" => Style::default().fg(pal.accent),
        _ => Style::default().fg(pal.subtext),
    }
}

/// `elided` marks that hints were dropped for want of room. Silently cutting
/// them is what made `o sort` look like a removed feature rather than an
/// off-screen one, so say so and point at the help screen.
fn shortcut_line<'a>(hints: &[(String, String)], pal: Palette, elided: bool) -> Line<'a> {
    let mut spans = Vec::new();
    for (i, (key, desc)) in hints.iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled("  ", Style::default().fg(pal.muted)));
        }
        spans.push(Span::styled(
            key.clone(),
            Style::default().fg(pal.accent).add_modifier(Modifier::BOLD),
        ));
        if !desc.is_empty() {
            spans.push(Span::raw(" "));
            spans.push(Span::styled(desc.clone(), Style::default().fg(pal.subtext)));
        }
    }
    if elided {
        spans.push(Span::styled(
            "  … ?",
            Style::default().fg(pal.accent).add_modifier(Modifier::BOLD),
        ));
    }
    Line::from(spans)
}

fn file_status_line<'a>(f: &crate::git::ChangedFile, pal: Palette) -> Line<'a> {
    Line::from(vec![
        Span::styled(format!("[{}] ", f.status), status_style(&f.status, pal)),
        Span::styled(format!("+{}", f.additions), Style::default().fg(pal.green)),
        Span::styled(format!("/-{}  ", f.deletions), Style::default().fg(pal.red)),
        Span::raw(f.path.clone()),
    ])
}

/// Fit `s` into exactly `max` terminal columns, padding or eliding as needed.
///
/// Counting characters instead of columns made a CJK string occupy twice its
/// budget, pushing the neighbouring fields of a fixed-width row off screen.
/// Shorten to `max` display columns by eliding the middle.
///
/// A path cut from the right loses the directory name, which is the part worth
/// reading; the config-file path in Settings was simply chopped at the frame
/// edge with nothing to signal it had been.
fn truncate_middle(s: &str, max: usize) -> String {
    use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};
    if s.width() <= max {
        return s.to_string();
    }
    if max <= 1 {
        return "…".to_string();
    }
    let budget = max - 1;
    let tail_budget = budget * 2 / 3;
    let head_budget = budget - tail_budget;
    fn take(it: impl Iterator<Item = char>, budget: usize) -> String {
        let mut out = String::new();
        let mut w = 0;
        for c in it {
            let cw = c.width().unwrap_or(0);
            if w + cw > budget {
                break;
            }
            out.push(c);
            w += cw;
        }
        out
    }
    let head = take(s.chars(), head_budget);
    let tail: String = take(s.chars().rev(), tail_budget).chars().rev().collect();
    format!("{head}…{tail}")
}

fn truncate(s: &str, max: usize) -> String {
    use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};
    if max == 0 {
        return String::new();
    }
    let width = s.width();
    if width <= max {
        return format!("{s}{}", " ".repeat(max - width));
    }
    let budget = max - 1; // one column for the ellipsis
    let mut out = String::new();
    let mut w = 0usize;
    for c in s.chars() {
        let cw = c.width().unwrap_or(0);
        if w + cw > budget {
            break;
        }
        out.push(c);
        w += cw;
    }
    out.push('…');
    w += 1;
    out.push_str(&" ".repeat(max.saturating_sub(w)));
    out
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::app::{RepoSnapshot, Screen};
    use crate::git::{Summary, WorktreeInfo};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    pub(crate) fn render(app: &App, width: u16, height: u16) {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal.draw(|f| draw(f, app)).unwrap();
    }

    /// Render and flatten the frame to plain text, for asserting specific
    /// content survived column truncation.
    pub(super) fn render_to_text(app: &App, width: u16, height: u16) -> String {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal.draw(|f| draw(f, app)).unwrap();
        terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect()
    }

    #[test]
    fn workspace_names_remain_distinct_under_a_long_common_parent() {
        let mut app = App::new();
        app.screen = Screen::Workspace;
        for name in ["review-one", "review-two"] {
            app.workspace.rows.push(crate::app::WorkspaceRow {
                parent: "project".into(),
                path: format!("/very/long/shared/workspace/project/{name}").into(),
                branch: "main".into(),
                dirty: Some(0),
                last_commit: String::new(),
                checked_at: 1,
                locked: false,
                prunable: false,
                error: None,
            });
        }
        let text = render_to_text(&app, 120, 30);
        assert!(text.contains("review-one"));
        assert!(text.contains("review-two"));
        assert!(text.contains("/very/long/shared/workspace/project/review-one"));
    }

    #[test]
    fn workspace_errors_can_be_inspected_with_healthy_rows_selected() {
        use crate::app::WorkspaceRow;
        use crossterm::event::{KeyCode, KeyEvent};
        let mut app = App::new();
        app.set_language_for_test(crate::config::Language::English);
        app.screen = Screen::Workspace;
        app.workspace.rows.push(WorkspaceRow {
            parent: "healthy".into(),
            path: "/healthy".into(),
            branch: "main".into(),
            dirty: Some(0),
            last_commit: String::new(),
            checked_at: 1,
            locked: false,
            prunable: false,
            error: None,
        });
        app.workspace.errors = vec![
            "broken-one: permission denied".into(),
            "broken-two: missing directory".into(),
        ];
        let text = render_to_text(&app, 120, 30);
        assert!(text.contains("healthy"));
        assert!(text.contains("broken-one: permission denied"));
        app.handle_key(KeyEvent::from(KeyCode::Char(']')));
        let text = render_to_text(&app, 120, 30);
        assert!(text.contains("broken-two: missing directory"));
        assert!(text.contains("Errors 2/2"));
        app.handle_key(KeyEvent::from(KeyCode::Char('[')));
        assert_eq!(app.workspace.error_selected, 0);
        assert_eq!(app.workspace.selected, 0);
    }

    /// A worktree whose directory cannot be read is not a clean worktree.
    /// It used to render as `Dirty ?` with an empty Last commit under a
    /// header reading "ready, errors: 0".
    #[test]
    fn an_uninspectable_worktree_row_is_named_counted_and_explained() {
        use crate::app::WorkspaceRow;
        use crossterm::event::{KeyCode, KeyEvent};
        let mut app = App::new();
        app.set_language_for_test(crate::config::Language::English);
        app.screen = Screen::Workspace;
        app.workspace.rows.push(WorkspaceRow {
            parent: "gone".into(),
            path: "/gone/review".into(),
            branch: "topic".into(),
            dirty: None,
            last_commit: String::new(),
            checked_at: 1,
            locked: false,
            prunable: false,
            error: Some("no such file or directory".into()),
        });
        let text = render_to_text(&app, 120, 30);
        assert!(
            text.contains("unknown: 1"),
            "the header should count the row it knows nothing about:\n{text}"
        );
        // The dump is one flat string of cells; 120 of them per row.
        let cells: Vec<char> = text.chars().collect();
        let line = cells
            .chunks(120)
            .map(|row| row.iter().collect::<String>())
            .find(|l| l.contains("topic"))
            .expect("the worktree row should be rendered");
        assert!(
            line.contains("unknown"),
            "the Dirty cell should name the state, not print '?': {line}"
        );
        assert!(
            line.contains("—"),
            "an unknown last commit should be marked, not left blank: {line}"
        );
        assert!(
            text.contains("no such file or directory"),
            "the reason should be on screen:\n{text}"
        );
        // And the reason is in the [ / ] inspector, not only in the detail
        // pane of whichever row happens to be selected.
        assert!(text.contains("Errors 1/1"), "{text}");
        app.handle_key(KeyEvent::from(KeyCode::Char(']')));
        let text = render_to_text(&app, 120, 30);
        assert!(text.contains("gone: /gone/review"), "{text}");
    }

    #[test]
    fn truncate_counts_display_columns() {
        // CJK characters are two columns wide; counting chars let a branch name
        // occupy twice its budget and push the next field off the row.
        assert_eq!(truncate("abc", 5).len(), 5);
        assert_eq!(truncate("日本語", 6), "日本語");
        // 4 columns of text + ellipsis, padded out to the full 6
        assert_eq!(truncate("日本語です", 6), "日本… ");
        assert_eq!(truncate("", 3), "   ");
        assert_eq!(truncate("abcdef", 3), "ab…");
        assert_eq!(truncate("anything", 0), "");
    }

    #[test]
    fn prompt_fits_a_terminal_narrower_than_the_dialog() {
        // The dialog clamped its width *up* to 20, producing a Rect outside the
        // frame; Clear indexes the buffer directly and panicked there.
        let area = Rect::new(0, 0, 15, 4);
        let rect = centered_rect(area, 20, 80, 5);
        assert!(rect.right() <= area.right() && rect.bottom() <= area.bottom());
    }

    /// The repository-detail overview had the same `↑0 ↓0` ambiguity as the
    /// Home row and has to answer it the same way.
    #[test]
    fn repo_overview_says_when_there_is_nothing_to_be_in_sync_with() {
        let mut app = App::new();
        app.set_language_for_test(crate::config::Language::English);
        app.repos = vec![crate::config::Repository {
            name: "r".to_string(),
            path: std::path::PathBuf::from("/tmp/repo"),
            group: None,
        }];
        app.repo_index = Some(0);
        app.screen = Screen::Repo;
        app.repo_tab = RepoTab::Status;
        app.repo_data = Some(RepoSnapshot {
            summary: Summary {
                current_branch: "main".to_string(),
                ..Summary::default()
            },
            commits: vec![],
            commits_err: None,
            branches: vec![],
            branches_err: None,
            tags: vec![],
            tags_err: None,
            stashes: vec![],
            stashes_err: None,
            working_files: vec![],
            working_err: None,
            contributors: vec![],
            contributors_err: None,
            worktrees: vec![],
            worktrees_err: None,
        });

        // No remote configured at all.
        let text = render_to_text(&app, 120, 30);
        assert!(text.contains("no remote"), "{text}");
        assert!(!text.contains("↑0 ↓0"), "{text}");

        // A remote exists, but this branch never was pushed.
        let summary = &mut app.repo_data.as_mut().unwrap().summary;
        summary.has_remote = true;
        let text = render_to_text(&app, 120, 30);
        assert!(text.contains("no upstream"), "{text}");

        // Tracking: the real counts, as before.
        let summary = &mut app.repo_data.as_mut().unwrap().summary;
        summary.has_upstream = true;
        summary.ahead = 2;
        let text = render_to_text(&app, 120, 30);
        assert!(text.contains("↑2 ↓0"), "{text}");
        assert!(!text.contains("no upstream"), "{text}");
    }

    /// The three states a Home row used to render identically: in sync, no
    /// upstream at all, and a registration that cannot be read.
    #[test]
    fn home_rows_tell_synced_unpushed_and_broken_repositories_apart() {
        use crate::app::HomeRow;
        use crate::git::UpstreamState;

        let mut app = App::new();
        app.set_language_for_test(crate::config::Language::English);
        app.repos = ["synced", "unpushed", "broken"]
            .iter()
            .map(|name| crate::config::Repository {
                name: (*name).to_string(),
                path: std::path::PathBuf::from(format!("/tmp/{name}")),
                group: None,
            })
            .collect();
        app.home_rows.clear();
        // `App::new` queues a refresh for the repositories it loaded, and a
        // busy row's Sync cell is a spinner instead of the counts under test.
        app.busy.clear();
        app.home_rows.insert(
            0,
            HomeRow {
                branch: "main".into(),
                upstream: UpstreamState::Tracking,
                // Two edited source files and five stray build artefacts.
                dirty: 7,
                tracked_changes: 2,
                untracked: 5,
                ..Default::default()
            },
        );
        app.home_rows.insert(
            1,
            HomeRow {
                branch: "wip".into(),
                upstream: UpstreamState::NoUpstream,
                ..Default::default()
            },
        );
        app.home_rows.insert(
            2,
            HomeRow {
                error: Some("not a git repository: /tmp/broken (fatal: ...)".into()),
                ..Default::default()
            },
        );

        // The panel describes the selected row, and the sort order comes
        // from the user's saved preference, so point the cursor at the
        // tracked repository explicitly rather than assuming it lands first.
        app.home_selected = app
            .filtered_home()
            .iter()
            .position(|&i| i == 0)
            .expect("the tracked repository is in the list");

        let text = render_to_text(&app, 140, 24);
        // ④ a tracked branch shows its counts; an unpushed one says it has
        // nothing to compare against instead of claiming agreement.
        assert!(text.contains("↑0 ↓0"), "{text}");
        assert!(text.contains("↑? ↓?"), "{text}");
        // ⑤ the broken registration is neither a spinner nor a clean row.
        assert!(text.contains("not a git repo"), "{text}");
        // ⑧ the dirty count is split, not merged into a single 7.
        assert!(text.contains("2M"), "{text}");
        assert!(text.contains("5?"), "{text}");
        assert!(
            !text.contains("Dirty 3"),
            "every row counted as dirty: {text}"
        );
        assert!(text.contains("Untracked 1"), "{text}");
        assert!(text.contains("Unreadable 1"), "{text}");

        // The panel explains the marker and carries the full path and the
        // remote's age, which the columns have no room for.
        assert!(text.contains("Remote fetched"), "{text}");
        assert!(text.contains("/tmp/synced"), "{text}");
    }

    #[test]
    fn attention_badge_survives_column_truncation_on_home() {
        use crate::app::HomeRow;

        let mut app = App::new();
        app.repos = vec![crate::config::Repository {
            name: "r".to_string(),
            path: std::path::PathBuf::from("/tmp/repo"),
            group: None,
        }];
        // Branch name + PR badge + CI badge alone is already close to the
        // 30-column cell; the conflict badge must still show up, meaning it
        // has to come before (not after) the PR/CI text.
        app.home_rows.insert(
            0,
            HomeRow {
                branch: "feature-branch".to_string(),
                ahead: 0,
                behind: 0,
                dirty: 0,
                last_commit: String::new(),
                open_prs: Some(123),
                ci_status: Some("failure".to_string()),
                op_state: crate::git::GitOpState::None,
                conflicts: 5,
                ..Default::default()
            },
        );
        let text = render_to_text(&app, 100, 20);
        assert!(
            text.contains("⚠5conflict"),
            "conflict badge was truncated out of the row: {text}"
        );
        // The PR/CI text is what should give way when the cell is tight —
        // proving the badge comes first rather than just being short enough
        // to fit by coincidence.
        assert!(
            !text.contains("PR:123") && !text.contains("✗CI"),
            "expected PR/CI to be the truncated content, not the badge: {text}"
        );
    }

    #[test]
    fn renders_settings_and_home_in_both_themes() {
        let mut app = App::new();
        app.repos = vec![crate::config::Repository {
            name: "alpha".to_string(),
            path: std::path::PathBuf::from("/tmp/repo"),
            group: None,
        }];
        app.screen = Screen::Settings;
        for _ in 0..2 {
            // Toggling exercises both themes regardless of which one a prior
            // test run left persisted in the shared config dir.
            app.handle_key(crossterm::event::KeyEvent::from(
                crossterm::event::KeyCode::Char('T'),
            ));
            render(&app, 100, 30);
            app.screen = Screen::Home;
            render(&app, 100, 30);
            app.screen = Screen::Settings;
        }
    }

    #[test]
    fn renders_every_screen_at_tiny_and_normal_sizes() {
        let mut app = App::new();
        app.repos = vec![crate::config::Repository {
            name: "リポジトリ".to_string(),
            path: std::path::PathBuf::from("/tmp/repo"),
            group: None,
        }];
        for screen in [
            Screen::Workspace,
            Screen::Home,
            Screen::Repo,
            Screen::Diff,
            Screen::Settings,
            Screen::GlobalMembers,
            Screen::RepoFinder,
            Screen::Help,
            Screen::Log,
        ] {
            app.screen = screen;
            render(&app, 15, 4);
            tests::render(&app, 80, 24);
            render(&app, 200, 60);
        }
    }

    #[test]
    fn renders_a_worktree_whose_head_is_not_a_hash() {
        // `git worktree list --porcelain` does not escape newlines in paths, so
        // a HEAD record can hold arbitrary multi-byte text.
        let mut app = App::new();
        app.repos = vec![crate::config::Repository {
            name: "r".to_string(),
            path: std::path::PathBuf::from("/tmp/repo"),
            group: None,
        }];
        app.repo_index = Some(0);
        app.screen = Screen::Repo;
        app.repo_tab = RepoTab::Worktrees;
        let mut snap = RepoSnapshot {
            summary: Summary::default(),
            commits: vec![],
            commits_err: None,
            branches: vec![],
            branches_err: None,
            tags: vec![],
            tags_err: None,
            stashes: vec![],
            stashes_err: None,
            working_files: vec![],
            working_err: None,
            contributors: vec![],
            contributors_err: None,
            worktrees: vec![],
            worktrees_err: None,
        };
        snap.worktrees.push(WorktreeInfo {
            path: "/tmp/wt".to_string(),
            head: "参照テスト".to_string(),
            branch: None,
            is_bare: false,
            is_detached: true,
            is_locked: false,
            is_prunable: false,
        });
        app.repo_data = Some(snap);
        tests::render(&app, 80, 24);
    }

    #[test]
    fn graph_ansi_codes_map_to_lane_colors_and_survive_marker_bolding() {
        let pal = Palette::mocha();
        // Sequence: red '|', reset, ' * ' marker (never colored by git),
        // 'e5f6g7h' plain hash text.
        let graph = "\u{1b}[31m|\u{1b}[m * ";
        let spans = render_graph_spans(graph, pal);
        let text: String = spans.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(text, "| * ");

        // The '|' took the red lane color...
        assert_eq!(spans[0].style.fg, Some(pal.graph_colors[0]));
        // ...and the marker '*' is always bold yellow, regardless of the
        // lane color active at that point in the string.
        let marker = spans.iter().find(|s| s.content.as_ref() == "*").unwrap();
        assert_eq!(marker.style.fg, Some(pal.yellow));
        assert!(marker.style.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn graph_lane_colors_cycle_and_ignore_unmapped_codes() {
        assert_eq!(
            ansi_sgr_to_graph_color("31"),
            Some(GraphColor {
                idx: 0,
                bold: false
            })
        );
        assert_eq!(
            ansi_sgr_to_graph_color("36"),
            Some(GraphColor {
                idx: 5,
                bold: false
            })
        );
        // Bright variant of the same lane maps to the same color slot.
        assert_eq!(
            ansi_sgr_to_graph_color("91"),
            Some(GraphColor {
                idx: 0,
                bold: false
            })
        );
        // git emits the bold attribute *before* the color for lanes 7-12
        // ("1;31", never "31;1") — this is git's actual mechanism for
        // getting 12 distinguishable lanes out of 6 hues, so the bold flag
        // must be captured, not discarded.
        assert_eq!(
            ansi_sgr_to_graph_color("1;33"),
            Some(GraphColor { idx: 2, bold: true })
        );
        // Reset and anything outside the recognised ranges must not panic
        // and must fall back to "no color" rather than a bogus index.
        assert_eq!(ansi_sgr_to_graph_color(""), None);
        assert_eq!(ansi_sgr_to_graph_color("0"), None);
        assert_eq!(ansi_sgr_to_graph_color("99"), None);
        assert_eq!(ansi_sgr_to_graph_color("not-a-number"), None);
    }

    #[test]
    fn graph_rejects_truecolor_and_256_color_instead_of_misreading_them() {
        // A user's custom `log.graphColors` truecolor/256-color value has a
        // trailing numeric field that can coincidentally collide with a
        // basic color code (e.g. 32 = green) — reading only the last field
        // previously mapped these onto an arbitrary, unrelated lane color
        // instead of correctly declining to guess.
        assert_eq!(ansi_sgr_to_graph_color("38;2;170;187;32"), None);
        assert_eq!(ansi_sgr_to_graph_color("38;5;208"), None);
        assert_eq!(ansi_sgr_to_graph_color("48;5;32"), None);
    }

    #[test]
    fn graph_bold_lane_is_visually_distinguishable_from_the_same_hue_plain() {
        // Lane 7 reuses lane 1's hue as "bold red" rather than a 7th color —
        // without applying the bold modifier, lanes 1 and 7 render pixel
        // identical, defeating the entire point of lane coloring.
        let pal = Palette::mocha();
        let plain = render_graph_spans("\u{1b}[31m|\u{1b}[m", pal);
        let bold = render_graph_spans("\u{1b}[1;31m|\u{1b}[m", pal);
        assert_eq!(plain[0].style.fg, bold[0].style.fg);
        assert!(!plain[0].style.add_modifier.contains(Modifier::BOLD));
        assert!(bold[0].style.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn graph_unterminated_escape_is_bounded_not_unbounded() {
        // No 'm' ever arrives; the scan for one must give up after
        // MAX_SGR_CODE_LEN rather than treating the rest of the string as
        // still part of this escape sequence. The scanned window (ESC + up
        // to 16 chars) is the only part allowed to be dropped as a rejected
        // color-code attempt — the remaining several thousand characters
        // must still come through as literal text, not vanish with it.
        let long_tail = "1".repeat(5000);
        let graph = format!("\u{1b}[{long_tail}");
        let spans = render_graph_spans(&graph, Palette::mocha());
        let consumed: usize = spans.iter().map(|s| s.content.chars().count()).sum();
        assert!(
            consumed > 4900,
            "expected the unterminated tail to still render as text, got {consumed} chars"
        );
    }

    #[test]
    fn renders_a_commit_row_with_a_real_ansi_colored_graph_prefix() {
        // Guards against a panic in render_graph_spans specifically, not
        // just draw() as a whole — the two-lane merge shape from a real
        // `git log --graph --color=always` capture.
        let mut app = App::new();
        app.repos = vec![crate::config::Repository {
            name: "r".to_string(),
            path: std::path::PathBuf::from("/tmp/repo"),
            group: None,
        }];
        app.repo_index = Some(0);
        app.screen = Screen::Repo;
        app.repo_tab = RepoTab::Commits;
        let mut snap = RepoSnapshot {
            summary: Summary::default(),
            commits: vec![],
            commits_err: None,
            branches: vec![],
            branches_err: None,
            tags: vec![],
            tags_err: None,
            stashes: vec![],
            stashes_err: None,
            working_files: vec![],
            working_err: None,
            contributors: vec![],
            contributors_err: None,
            worktrees: vec![],
            worktrees_err: None,
        };
        snap.commits = crate::git::parse_commit_log(
            "* a1b2c3d|||main|||Alice|||2023-01-01 12:00|||First\n\u{1b}[31m|\u{1b}[m * e5f6g7h|||feature|||Bob|||2023-01-02 15:30|||Second\n",
        );
        app.repo_data = Some(snap);
        render(&app, 100, 30);
    }

    #[test]
    fn renders_commit_search_empty_loading_and_populated() {
        use crate::app::{CommitSearchHit, CommitSearchState};

        let mut app = App::new();
        app.screen = Screen::CommitSearch;
        // No search started yet
        render(&app, 100, 30);

        app.commit_search = Some(CommitSearchState {
            query: "fix".into(),
            hits: vec![],
            selected: 0,
            loading: true,
        });
        render(&app, 100, 30);

        app.commit_search = Some(CommitSearchState {
            query: "fix".into(),
            hits: vec![CommitSearchHit {
                repo_index: 0,
                repo_name: "日本語リポジトリ".into(),
                hash: "abc1234".into(),
                author: "Someone".into(),
                date: "2024-01-01".into(),
                message: "fix: something".into(),
            }],
            selected: 0,
            loading: false,
        });
        render(&app, 100, 30);
        render(&app, 15, 4);
    }

    #[test]
    fn renders_diff_with_blame_gutter_including_a_removed_line() {
        use crate::app::{DiffLine, DiffView};
        use crate::git::BlameEntry;

        let mut app = App::new();
        app.screen = Screen::Diff;
        app.diff = Some(DiffView {
            title: "t".into(),
            target: "abc".into(),
            base: None,
            three_dot: false,
            files: vec![],
            file_idx: 0,
            lines: vec![
                DiffLine {
                    kind: DiffRowKind::Context,
                    text: " a".into(),
                    old_no: Some(1),
                    new_no: Some(1),
                },
                DiffLine {
                    kind: DiffRowKind::Removed,
                    text: "-b".into(),
                    old_no: Some(2),
                    new_no: None,
                },
            ],
            hunks: vec![],
            hunk_idx: 0,
            scroll: 0,
            loading: false,
            header: None,
            error: None,
            // Removed lines have no new_no, so this must not panic on an
            // out-of-range or absent lookup.
            blame: Some(vec![BlameEntry {
                hash: "abc1234def".into(),
                author: "Someone".into(),
                date: "2024-01-01".into(),
                summary: "s".into(),
            }]),
            blame_loading: false,
            pending_scroll_restore: None,
        });
        render(&app, 80, 24);
    }

    #[test]
    fn blame_gutter_does_not_duplicate_across_a_wrapped_long_line() {
        use crate::app::{DiffLine, DiffView};
        use crate::git::BlameEntry;

        let mut app = App::new();
        app.screen = Screen::Diff;
        let long_line = "x".repeat(300);
        app.diff = Some(DiffView {
            title: "t".into(),
            target: "abc".into(),
            base: None,
            three_dot: false,
            files: vec![crate::git::ChangedFile {
                status: "M".into(),
                path: "f.rs".into(),
                old_path: None,
                additions: 1,
                deletions: 0,
            }],
            file_idx: 0,
            lines: vec![DiffLine {
                kind: DiffRowKind::Context,
                text: format!(" {long_line}"),
                old_no: Some(1),
                new_no: Some(1),
            }],
            hunks: vec![],
            hunk_idx: 0,
            scroll: 0,
            loading: false,
            header: None,
            error: None,
            blame: Some(vec![BlameEntry {
                hash: "distinctivehash1".into(),
                // Short enough to survive the 22-column gutter truncation
                // intact, so the occurrence count below is exact.
                author: "Uniq".into(),
                date: "2024-01-01".into(),
                summary: "s".into(),
            }]),
            blame_loading: false,
            pending_scroll_restore: None,
        });
        // Narrower than the 300-char line, so the old wrap-enabled behavior
        // would have produced multiple wrapped rows, each read as if it had
        // its own (blank) gutter — i.e. as a different line's blame.
        let text = render_to_text(&app, 80, 24);
        let occurrences = text.matches("distinc Uniq").count();
        assert_eq!(
            occurrences, 1,
            "blame gutter text should render exactly once, not once per wrapped row: {text}"
        );
    }
}

#[cfg(test)]
mod sort_tests {
    use super::tests::render_to_text;
    use super::*;
    use crate::app::Screen;

    fn app_with_repos(n: usize) -> App {
        let mut app = App::new();
        app.repos = (0..n)
            .map(|i| crate::config::Repository {
                name: format!("repo-{i:02}"),
                path: std::path::PathBuf::from(format!("/tmp/r{i:02}")),
                group: None,
            })
            .collect();
        app.screen = Screen::Home;
        // Neither Name mode, so the first click on Name is unambiguously
        // "sort ascending" rather than "reverse the current direction".
        app.set_sort_for_test(crate::app::SORT_UPDATED_DESC);
        app
    }

    /// The click hit-test uses bounds the renderer records. If those bounds
    /// drifted from where the header text actually lands, clicking a header
    /// would sort by the wrong column — so check them against the real buffer.
    #[test]
    fn recorded_column_bounds_match_where_headers_actually_render() {
        let app = app_with_repos(3);
        let text = render_to_text(&app, 120, 12);
        let width = 120usize;
        // Row 2 is the header row (0 = title bar, 1 = table top border).
        let header: String = text.chars().skip(2 * width).take(width).collect();

        let bounds = app.home_col_bounds.borrow().clone();
        assert_eq!(bounds.len(), 6, "expected six columns, got {bounds:?}");

        for (label, col) in [
            ("Name", 0),
            ("Branch", 1),
            ("Sync", 2),
            ("Dirty", 3),
            ("Last commit", 4),
        ] {
            let found = super::commit_click_tests::column_of(&header, label)
                .unwrap_or_else(|| panic!("header {label:?} not rendered in: {header:?}"));
            let (start, end) = bounds[col];
            assert!(
                found >= start && found < end,
                "{label} renders at x={found} but column {col} was recorded as {start}..{end}"
            );
            // And the hit-test agrees for that x.
            assert_eq!(app.home_column_at(found), Some(col), "hit-test for {label}");
        }
    }

    #[test]
    fn home_mouse_tracks_summary_layout_and_ignores_context() {
        let mut app = app_with_repos(10);
        render_to_text(&app, 120, 24);
        let (header, end) = app.home_table_bounds.get();
        assert_eq!(header, 3);
        let x = app.home_col_bounds.borrow()[0].0;
        app.handle_mouse_click(x, header);
        assert_eq!(app.sort_mode(), crate::app::SORT_NAME_ASC);
        app.handle_mouse_click(5, header + 2);
        assert_eq!(app.home_selected, 1);
        app.handle_mouse_click(5, end + 2);
        assert_eq!(app.screen, Screen::Home);
        assert_eq!(app.home_selected, 1);
        render_to_text(&app, 60, 24);
        assert_eq!(app.home_table_bounds.get().0, 2);
    }

    #[test]
    fn clicking_a_header_sorts_by_it_and_toggles_direction() {
        let mut app = app_with_repos(3);
        render_to_text(&app, 120, 12); // populate the recorded bounds

        let bounds = app.home_col_bounds.borrow().clone();
        let x_of = |c: usize| bounds[c].0;

        // Name column: first click ascending, second click reverses.
        app.handle_mouse_click(x_of(0), 2);
        assert_eq!(app.sort_mode(), crate::app::SORT_NAME_ASC);
        app.handle_mouse_click(x_of(0), 2);
        assert_eq!(app.sort_mode(), crate::app::SORT_NAME_DESC);

        // A different column starts at its own default direction — "most
        // changed first" for Dirty rather than blindly ascending.
        app.handle_mouse_click(x_of(3), 2);
        assert_eq!(app.sort_mode(), crate::app::SORT_DIRTY_DESC);
        app.handle_mouse_click(x_of(3), 2);
        assert_eq!(app.sort_mode(), crate::app::SORT_DIRTY_ASC);

        // Updated defaults to newest-first.
        app.handle_mouse_click(x_of(4), 2);
        assert_eq!(app.sort_mode(), crate::app::SORT_UPDATED_DESC);

        // The Path column is not sortable; clicking it changes nothing.
        let before = app.sort_mode();
        app.handle_mouse_click(x_of(5), 2);
        assert_eq!(app.sort_mode(), before);
    }

    #[test]
    fn the_sorted_column_shows_a_direction_marker() {
        let mut app = app_with_repos(3);
        render_to_text(&app, 120, 12);
        let bounds = app.home_col_bounds.borrow().clone();

        app.handle_mouse_click(bounds[0].0, 2); // Name ascending
        let text = render_to_text(&app, 120, 12);
        assert!(text.contains("Name▲"), "expected ascending marker: {text}");

        app.handle_mouse_click(bounds[0].0, 2); // Name descending
        let text = render_to_text(&app, 120, 12);
        assert!(text.contains("Name▼"), "expected descending marker: {text}");
    }
}

#[cfg(test)]
mod commit_click_tests {
    use super::tests::render_to_text;
    use super::*;
    use crate::app::RepoSnapshot;
    use crate::git::{CommitSummary, Summary, TagInfo};

    /// Screen column (not byte offset) where `needle` starts in `line`.
    ///
    /// `str::find` returns a byte index, and these lines begin with multi-byte
    /// box-drawing glyphs — comparing that against an x-coordinate silently
    /// compares two different units, and happens to agree often enough to let
    /// a wrong hit-test pass.
    pub(super) fn column_of(line: &str, needle: &str) -> Option<u16> {
        let chars: Vec<char> = line.chars().collect();
        let pat: Vec<char> = needle.chars().collect();
        (0..=chars.len().saturating_sub(pat.len()))
            .find(|&i| chars[i..i + pat.len()] == pat[..])
            .map(|i| i as u16)
    }

    fn commit(n: usize) -> CommitSummary {
        CommitSummary {
            hash: format!("c{n:06}"),
            author: format!("author{n}"),
            date: "2026-08-26".to_string(),
            message: format!("commit number {n}"),
            graph: "* ".to_string(),
            refs: vec![],
        }
    }

    fn app_on_commits(n: usize) -> App {
        let mut app = App::new();
        app.repos = vec![crate::config::Repository {
            name: "r".to_string(),
            path: std::path::PathBuf::from("/tmp/repo"),
            group: None,
        }];
        app.repo_index = Some(0);
        app.screen = Screen::Repo;
        app.repo_tab = RepoTab::Commits;
        app.repo_data = Some(RepoSnapshot {
            summary: Summary::default(),
            commits: (0..n).map(commit).collect(),
            commits_err: None,
            branches: vec![],
            branches_err: None,
            tags: (0..n)
                .map(|i| TagInfo {
                    name: format!("v0.{i}.0"),
                    date: "2026-08-26".to_string(),
                    message: String::new(),
                    hash: format!("c{i:06}"),
                })
                .collect(),
            tags_err: None,
            stashes: vec![],
            stashes_err: None,
            working_files: vec![],
            working_err: None,
            contributors: vec![],
            contributors_err: None,
            worktrees: vec![],
            worktrees_err: None,
        });
        app
    }

    /// The marker hit region is expressed as two constants in the click
    /// handler, while the row itself is drawn by the renderer. Check the two
    /// against a real frame: the `[` of every visible row must fall inside the
    /// region the handler treats as the marker, and the region must not spill
    /// onto the graph/hash text that follows.
    #[test]
    fn the_marker_hit_region_covers_exactly_where_the_marker_renders() {
        let app = app_on_commits(5);
        let width = 120usize;
        let text = render_to_text(&app, width as u16, 20);
        let vp = app.list_viewport.get();
        assert!(vp.height > 0, "list did not render");

        let marker_start = vp.x + 2; // App::HIGHLIGHT_WIDTH
        let marker_end = marker_start + 3; // App::MARKER_WIDTH
        for r in 0..5u16 {
            let line: String = text
                .chars()
                .skip((vp.y + r) as usize * width)
                .take(width)
                .collect();
            let open = column_of(&line, "[").unwrap_or_else(|| panic!("no marker: {line:?}"));
            let close = column_of(&line, "]").unwrap();
            assert_eq!(open, marker_start, "row {r}: {line:?}");
            assert_eq!(close, marker_end - 1, "row {r}: {line:?}");
        }
    }

    #[test]
    fn clicking_the_marker_picks_base_then_target() {
        let mut app = app_on_commits(5);
        render_to_text(&app, 120, 20);
        let vp = app.list_viewport.get();
        let marker_x = vp.x + 2;

        app.handle_mouse_click(marker_x, vp.y); // first row
        assert_eq!(app.commit_base.as_deref(), Some("c000000"));
        assert_eq!(app.commit_target, None);

        app.handle_mouse_click(marker_x, vp.y + 2); // third row
        assert_eq!(app.commit_base.as_deref(), Some("c000000"));
        assert_eq!(app.commit_target.as_deref(), Some("c000002"));
    }

    /// Clicking a marked row again must clear it. Without this, picking the
    /// wrong commit would be unfixable by mouse — base is already set, so a
    /// further click would only ever overwrite the target.
    #[test]
    fn clicking_a_marked_row_again_clears_it() {
        let mut app = app_on_commits(5);
        render_to_text(&app, 120, 20);
        let vp = app.list_viewport.get();
        let marker_x = vp.x + 2;

        app.handle_mouse_click(marker_x, vp.y);
        app.handle_mouse_click(marker_x, vp.y + 1);
        assert_eq!(app.commit_base.as_deref(), Some("c000000"));
        assert_eq!(app.commit_target.as_deref(), Some("c000001"));

        app.handle_mouse_click(marker_x, vp.y);
        assert_eq!(app.commit_base, None);
        assert_eq!(app.commit_target.as_deref(), Some("c000001"));

        app.handle_mouse_click(marker_x, vp.y + 1);
        assert_eq!(app.commit_target, None);
    }

    /// The marker click must also move the selection there. Otherwise the row
    /// highlighted and the row marked disagree, and the next keypress acts on
    /// a commit the user is not looking at.
    #[test]
    fn clicking_a_marker_also_selects_that_row() {
        let mut app = app_on_commits(5);
        render_to_text(&app, 120, 20);
        let vp = app.list_viewport.get();
        app.handle_mouse_click(vp.x + 2, vp.y + 3);
        assert_eq!(app.list_selected, 3);
    }

    /// Clicking the row body is plain selection — marking is the marker's job.
    #[test]
    fn clicking_the_row_body_selects_without_marking() {
        let mut app = app_on_commits(5);
        render_to_text(&app, 120, 20);
        let vp = app.list_viewport.get();
        app.handle_mouse_click(vp.x + 40, vp.y + 2);
        assert_eq!(app.list_selected, 2);
        assert_eq!(app.commit_base, None);
        assert_eq!(app.commit_target, None);
    }

    #[test]
    fn tags_mark_by_click_too() {
        let mut app = app_on_commits(4);
        app.repo_tab = RepoTab::Tags;
        render_to_text(&app, 120, 20);
        let vp = app.list_viewport.get();
        app.handle_mouse_click(vp.x + 2, vp.y);
        app.handle_mouse_click(vp.x + 2, vp.y + 1);
        assert_eq!(app.tag_base.as_deref(), Some("v0.0.0"));
        assert_eq!(app.tag_target.as_deref(), Some("v0.1.0"));
    }

    /// Tabs with no comparison have no marker, so a click in the same columns
    /// must fall through to selection rather than doing nothing.
    #[test]
    fn a_tab_without_markers_still_selects_from_a_left_edge_click() {
        let mut app = app_on_commits(4);
        app.repo_tab = RepoTab::Branches;
        app.repo_data.as_mut().unwrap().branches = (0..4)
            .map(|i| crate::git::BranchInfo {
                name: format!("branch-{i}"),
                is_remote: false,
                author: "a".to_string(),
                date: "2026-08-26".to_string(),
                date_unix: 0,
                message: "m".to_string(),
            })
            .collect();
        render_to_text(&app, 120, 20);
        let vp = app.list_viewport.get();
        app.handle_mouse_click(vp.x + 2, vp.y + 2);
        assert_eq!(app.list_selected, 2);
    }

    /// Clicks land on screen rows; the list scrolls. A click after scrolling
    /// must mark the row under the cursor, not the same ordinal from the top.
    #[test]
    fn a_click_after_the_list_scrolls_marks_the_row_under_the_cursor() {
        let mut app = app_on_commits(200);
        app.list_selected = 150;
        render_to_text(&app, 120, 20);
        let vp = app.list_viewport.get();
        assert!(
            vp.offset > 0,
            "list should have scrolled, offset={}",
            vp.offset
        );

        app.handle_mouse_click(vp.x + 2, vp.y);
        let expected = format!("c{:06}", vp.offset);
        assert_eq!(app.commit_base.as_deref(), Some(expected.as_str()));
    }

    /// A click below the last row is empty space, not the last row.
    #[test]
    fn a_click_past_the_last_row_does_nothing() {
        let mut app = app_on_commits(3);
        render_to_text(&app, 120, 20);
        let vp = app.list_viewport.get();
        app.list_selected = 0;
        app.handle_mouse_click(vp.x + 2, vp.y + 8);
        assert_eq!(app.commit_base, None);
        assert_eq!(app.list_selected, 0);
    }

    /// The tab bar sits above the list; a click there must still switch tabs
    /// rather than being swallowed by the new row handling.
    #[test]
    fn the_tab_bar_still_switches_tabs() {
        let mut app = app_on_commits(3);
        render_to_text(&app, 120, 20);
        app.handle_mouse_click(30, 1);
        assert_eq!(app.repo_tab, RepoTab::Branches);
    }
}

#[test]
fn title_navigation_popup_mouse_routes_all_destinations_in_both_languages() {
    use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
    let click = |app: &mut App, x: u16, y: u16| {
        app.handle_mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: x,
            row: y,
            modifiers: crossterm::event::KeyModifiers::empty(),
        });
    };
    for language in [
        crate::config::Language::English,
        crate::config::Language::Japanese,
    ] {
        for (row, expected) in [
            (0, Screen::Settings),
            (1, Screen::Workspace),
            (2, Screen::GlobalMembers),
        ] {
            let mut app = App::new();
            app.set_language_for_test(language);
            tests::render(&app, 80, 24);
            let (x1, _, _) = app.nav_button_bounds.borrow()[2];
            click(&mut app, x1, 0);
            tests::render(&app, 80, 24);
            let popup = app.nav_popup_rect.get();
            click(&mut app, popup.x + 1, popup.y + 1 + row);
            assert_eq!(app.screen, expected);
            assert!(!app.nav_popup);
        }
        let mut app = App::new();
        app.set_language_for_test(language);
        tests::render(&app, 80, 24);
        let (x1, _, _) = app.nav_button_bounds.borrow()[2];
        click(&mut app, x1, 0);
        tests::render(&app, 80, 24);
        let popup = app.nav_popup_rect.get();
        click(&mut app, popup.x + 1, popup.y + 4);
        let text = tests::render_to_text(&app, 80, 24);
        let dense: String = text.chars().filter(|c| !c.is_whitespace()).collect();
        let prompt = match language {
            crate::config::Language::English => "Searchcommit",
            crate::config::Language::Japanese => "コミットメッセージを検索",
        };
        assert!(dense.contains(prompt), "{text}");
    }
}

#[test]
fn title_navigation_respects_modal_and_popup_outer_click_boundaries() {
    use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
    let click = |app: &mut App, x: u16, y: u16| {
        app.handle_mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: x,
            row: y,
            modifiers: crossterm::event::KeyModifiers::empty(),
        });
    };
    let mut app = App::new();
    tests::render(&app, 80, 24);
    let (x1, _, _) = app.nav_button_bounds.borrow()[2];
    click(&mut app, x1, 0);
    tests::render(&app, 80, 24);
    let popup = app.nav_popup_rect.get();
    click(&mut app, popup.x.saturating_sub(1), popup.y);
    assert!(!app.nav_popup);
    app.open_tool_menu("/tmp/repo".into());
    click(&mut app, x1, 0);
    assert!(!app.nav_popup);
}

#[test]
fn title_navigation_does_not_register_buttons_past_terminal_edge() {
    let app = App::new();
    tests::render(&app, 20, 8);
    assert!(
        app.nav_button_bounds
            .borrow()
            .iter()
            .all(|(x1, x2, _)| *x2 <= 20 && *x1 < *x2)
    );
    assert!(!tests::render_to_text(&app, 20, 8).contains("Navigate"));
}

#[test]
fn commit_search_loading_clears_previous_mouse_viewport() {
    use crate::app::CommitSearchState;
    let mut app = App::new();
    app.commit_search_viewport.set(ListViewport {
        x: 1,
        y: 2,
        width: 10,
        height: 3,
        offset: 4,
    });
    app.commit_search = Some(CommitSearchState {
        query: "needle".into(),
        hits: vec![],
        selected: 0,
        loading: true,
    });
    app.screen = Screen::CommitSearch;
    tests::render(&app, 80, 24);
    assert_eq!(app.commit_search_viewport.get(), ListViewport::default());
}

#[cfg(test)]
mod footer_and_help_tests {
    use super::commit_click_tests::column_of;
    use super::tests::render_to_text;
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent};

    /// Does the rendered frame contain this text?
    ///
    /// A double-width glyph occupies two cells, and the frame dump emits a
    /// blank for the second — so `"戻る"` appears as `"戻 る"` and a plain
    /// `contains` never matches. Comparing with whitespace removed sidesteps
    /// that without having to model cell widths.
    /// Screen column where `needle` begins, tolerating the blank cell that
    /// follows every double-width glyph in a frame dump (`"状態"` appears as
    /// `"状 態"`). Both sides are compared with whitespace removed, and the
    /// match is mapped back to the column the first character occupies.
    pub(super) fn text_column_of(line: &str, needle: &str) -> Option<u16> {
        let cols: Vec<(usize, char)> = line
            .chars()
            .enumerate()
            .filter(|(_, c)| !c.is_whitespace())
            .collect();
        let dense: String = cols.iter().map(|(_, c)| *c).collect();
        let pat: String = needle.chars().filter(|c| !c.is_whitespace()).collect();
        let at = dense
            .char_indices()
            .position(|(b, _)| dense[b..].starts_with(&pat))?;
        Some(cols[at].0 as u16)
    }

    pub(super) fn frame_contains(rendered: &str, needle: &str) -> bool {
        let strip = |s: &str| s.chars().filter(|c| !c.is_whitespace()).collect::<String>();
        strip(rendered).contains(&strip(needle))
    }

    fn home_app() -> App {
        let mut app = App::new();
        app.repos = vec![crate::config::Repository {
            name: "alpha".to_string(),
            path: std::path::PathBuf::from("/tmp/repo"),
            group: None,
        }];
        app.screen = Screen::Home;
        app
    }

    /// The whole point of wrapping: `q quit` used to fall off the end of the
    /// one-row footer on any terminal narrower than 154 columns, which is to
    /// say all of them.
    #[test]
    fn hints_that_do_not_fit_one_row_wrap_onto_a_second() {
        let app = home_app();
        let hints = app.footer_hints();
        let rows = wrap_hints(&hints, 100, MAX_HINT_ROWS);
        assert_eq!(rows.len(), 2, "expected a second row at 100 columns");
        let shown: usize = rows.iter().map(|r| r.len()).sum();
        assert_eq!(
            shown,
            hints.len(),
            "every hint should be visible at 100 cols"
        );

        let text = render_to_text(&app, 100, 24);
        for key in ["o", "q", "s"] {
            let hint = hints.iter().find(|(k, _)| k == key).unwrap();
            assert!(
                text.contains(&hint.1),
                "hint {key:?} ({}) missing from a 100-column frame",
                hint.1
            );
        }
    }

    #[test]
    fn a_wide_terminal_still_uses_a_single_row() {
        let app = home_app();
        let rows = wrap_hints(&app.footer_hints(), 200, MAX_HINT_ROWS);
        assert_eq!(rows.len(), 1);
    }

    /// Two rows is the cap. Below the width where even that fits, the user has
    /// to be told hints are missing rather than being shown a clean lie.
    #[test]
    fn a_very_narrow_terminal_marks_the_hints_it_could_not_show() {
        let app = home_app();
        let hints = app.footer_hints();
        let rows = wrap_hints(&hints, 30, MAX_HINT_ROWS);
        assert_eq!(rows.len(), MAX_HINT_ROWS);
        let shown: usize = rows.iter().map(|r| r.len()).sum();
        assert!(shown < hints.len(), "30 columns should not fit every hint");

        let text = render_to_text(&app, 30, 24);
        assert!(text.contains("… ?"), "no overflow marker in:\n{text}");
    }

    /// Wrapping must measure display columns; counting chars would let a row of
    /// Japanese hints occupy twice its budget and overflow anyway.
    #[test]
    fn wrapping_measures_display_width_not_character_count() {
        use unicode_width::UnicodeWidthStr;
        let hints: Vec<(String, String)> = vec![
            ("a".to_string(), "日本語テスト".to_string()),
            ("b".to_string(), "日本語テスト".to_string()),
        ];
        // Each item is 1 + 1 + 12 = 14 columns; two plus a separator is 30.
        assert_eq!(hints[0].1.width(), 12);
        assert_eq!(wrap_hints(&hints, 30, 2).len(), 1);
        assert_eq!(wrap_hints(&hints, 29, 2).len(), 2);
    }

    fn help_app() -> App {
        let mut app = home_app();
        app.screen = Screen::Help;
        app
    }

    /// The help is longer than any ordinary terminal. Before it scrolled, the
    /// lines past the fold could not be reached at all.
    #[test]
    fn the_help_scrolls_to_lines_a_short_terminal_cannot_show() {
        let mut app = help_app();
        let first = render_to_text(&app, 100, 24);
        assert!(
            first.contains("Keys for:"),
            "expected the context header first"
        );
        assert!(
            !first.contains("close the menu"),
            "the last section should be below the fold at 24 rows"
        );

        app.handle_key(KeyEvent::from(KeyCode::Char('G')));
        let last = render_to_text(&app, 100, 24);
        assert!(
            last.contains("close the menu"),
            "G should reach the last line:\n{last}"
        );

        app.handle_key(KeyEvent::from(KeyCode::Char('g')));
        let back = render_to_text(&app, 100, 24);
        assert!(back.contains("Keys for:"), "g should return to the top");
    }

    /// The keys the user needs are the ones for the screen they pressed `?`
    /// on. They used to be four sections below the fold; now they open the
    /// page, and everything else is still one scroll away.
    #[test]
    fn the_help_leads_with_the_screen_it_was_opened_from() {
        let mut app = home_app();
        app.screen = Screen::Workspace;
        app.handle_key(KeyEvent::from(KeyCode::Char('?')));
        assert_eq!(app.screen, Screen::Help);

        // 24 rows: roughly what is left of a small terminal.
        let first = render_to_text(&app, 100, 24);
        for needle in [
            "Worktrees across repositories (Home W)",
            "edit the purpose note",
            "previous / next problem",
            // the keys that work everywhere stay in view too
            "toggle this help",
        ] {
            assert!(
                first.contains(needle),
                "{needle:?} should be above the fold on Worktrees:\n{first}"
            );
        }
        assert!(
            !first.contains("blame gutter"),
            "another screen's keys should not be in the way:\n{first}"
        );
        assert!(
            first.contains("Help — Worktrees across repositories (Home W)"),
            "the title should name the screen being described:\n{first}"
        );

        // Everything else is below, not gone.
        app.handle_key(KeyEvent::from(KeyCode::Char('G')));
        let last = render_to_text(&app, 100, 24);
        assert!(last.contains("close the menu"), "{last}");

        // `?` still toggles back to where it came from.
        app.handle_key(KeyEvent::from(KeyCode::Char('?')));
        assert_eq!(app.screen, Screen::Workspace);
    }

    /// docs/reference.md is the inventory of bindings. Splitting the help per
    /// screen must not lose a section — or a row — on the way.
    #[test]
    fn no_documented_section_or_key_leaves_the_help() {
        let app = help_app();
        let text = render_to_text(&app, 130, 130);
        for needle in [
            // every section, including the four screens the flat list never
            // mentioned at all
            "Global",
            "Repositories",
            "Repository detail",
            "Diff",
            "Settings",
            "Global Members",
            "Worktrees across repositories (Home W)",
            "Commit search (Home S)",
            "Repository finder (Home A)",
            "Branch log",
            "Tools menu (O)",
            // rows that only appear once, one per section
            "q / Ctrl+C",
            "wheel / click",
            "P / F",
            "click header",
            "apply / drop the selected stash",
            "blame gutter",
            "Tab / 1 / 2",
            "switch pane between members and repo list",
            "* / f",
            "open the hit's diff inside its repository",
            "import the selected repositories",
            "back to the repository",
            "t / e / l / g",
            "wait for the editor",
        ] {
            assert!(
                text.contains(needle),
                "{needle:?} missing from the help:\n{text}"
            );
        }
    }

    #[test]
    fn the_help_title_reports_the_visible_range_only_when_it_scrolls() {
        let app = help_app();
        let short = render_to_text(&app, 100, 24);
        assert!(
            short.contains("(1-"),
            "expected an x-y/total counter:\n{short}"
        );
        // Tall enough for every line plus borders, title bar and footer.
        let tall = render_to_text(&app, 100, 130);
        assert!(
            !tall.contains("(1-"),
            "no counter when nothing is hidden:\n{tall}"
        );
    }

    #[test]
    fn scrolling_stops_at_the_end_instead_of_running_off() {
        let mut app = help_app();
        for _ in 0..500 {
            app.handle_key(KeyEvent::from(KeyCode::Char('j')));
        }
        let text = render_to_text(&app, 100, 24);
        assert!(
            text.contains("close the menu"),
            "over-scrolling should rest on the last page:\n{text}"
        );
    }

    #[test]
    fn leaving_the_help_resets_it_to_the_top() {
        // Enter the help the way a user does, so help_return is set.
        let mut app = home_app();
        app.handle_key(KeyEvent::from(KeyCode::Char('?')));
        assert_eq!(app.screen, Screen::Help);
        app.handle_key(KeyEvent::from(KeyCode::Char('G')));
        app.handle_key(KeyEvent::from(KeyCode::Esc));
        assert_eq!(app.screen, Screen::Home);
        assert_eq!(app.help_scroll.get(), 0);
    }

    /// Every body line goes through `tt`, so a Japanese UI must not leave
    /// English prose in the help — it was 54 of 58 lines before.
    #[test]
    fn the_help_body_is_translated() {
        let mut app = help_app();
        app.handle_key(KeyEvent::from(KeyCode::Char('l'))); // no-op on Help
        let en = render_to_text(&app, 120, 130);
        assert!(en.contains("back one screen"));

        let mut app = help_app();
        app.set_language_for_test(crate::config::Language::Japanese);
        let ja = render_to_text(&app, 120, 130);
        assert!(
            !ja.contains("back one screen"),
            "English prose left in the Japanese help:\n{ja}"
        );
        for expected in [
            "1つ前の画面へ戻る",
            "blame表示の切替",
            "全リポジトリ横断メンバー",
            "リポジトリ詳細",
            "ツールメニュー",
            "用途メモを編集",
        ] {
            assert!(
                frame_contains(&ja, expected),
                "{expected:?} missing from the Japanese help:\n{ja}"
            );
        }
    }

    /// The key column is padded so descriptions line up. In Japanese the
    /// descriptions are twice as wide, and an unpadded key column would let
    /// each row start wherever its key happened to end.
    #[test]
    fn help_descriptions_start_at_the_same_column_in_both_languages() {
        let width = 120usize;
        for lang in [
            crate::config::Language::English,
            crate::config::Language::Japanese,
        ] {
            let mut app = help_app();
            app.set_language_for_test(lang);
            let text = render_to_text(&app, width as u16, 70);
            let lines: Vec<String> = (0..70)
                .map(|r| text.chars().skip(r * width).take(width).collect())
                .collect();

            // Every key row is "  <key padded to 14> <description>", so the
            // description always begins at the same column.
            let mut desc_cols = Vec::new();
            for key in ["q / Ctrl+C", "Esc / h", "Tab / 1 / 2", "b"] {
                let line = lines
                    .iter()
                    .find(|l| column_of(l, &format!("  {key} ")) == Some(1))
                    .unwrap_or_else(|| panic!("no row for {key:?} in {lang:?}"));
                let after: String = line.chars().skip(17).collect();
                let lead = after.len() - after.trim_start().len();
                desc_cols.push(17 + lead);
            }
            assert!(
                desc_cols.windows(2).all(|w| w[0] == w[1]),
                "descriptions not aligned for {lang:?}: {desc_cols:?}"
            );
        }
    }
}

#[cfg(test)]
mod width_tests {
    use super::footer_and_help_tests::{frame_contains, text_column_of};
    use super::tests::render_to_text;
    use super::*;
    use crate::config::Language;

    fn app(lang: Language) -> App {
        let mut app = App::new();
        app.repos = vec![crate::config::Repository {
            name: "alpha".to_string(),
            path: std::path::PathBuf::from("/tmp/repo"),
            group: None,
        }];
        app.set_language_for_test(lang);
        app
    }

    #[test]
    fn onboarding_keeps_all_actions_visible_or_yields_to_the_table() {
        for lang in [Language::English, Language::Japanese] {
            let mut a = app(lang);
            a.onboarding_visible = true;
            let title = a.tt("Start here", "まずはこの4つから");
            let text = render_to_text(&a, 110, 24);
            assert!(frame_contains(&text, &title));
            for key in ["Enter", "Esc", "?"] {
                assert!(text.contains(key), "{text}");
            }
            for (width, height) in [(40, 24), (110, 10)] {
                let text = render_to_text(&a, width, height);
                assert!(!frame_contains(&text, &title), "{text}");
                assert!(text.contains("alph"), "{text}");
            }
        }
    }

    /// `未コミット` is 10 columns and the Dirty cell was a fixed 8, so the
    /// header rendered as `未コミッ` — a truncated word, with no ellipsis to
    /// say so.
    #[test]
    fn no_home_header_is_cut_off_by_its_own_column() {
        for lang in [Language::English, Language::Japanese] {
            let mut a = app(lang);
            a.screen = Screen::Home;
            let text = render_to_text(&a, 120, 20);
            for label in [
                "Dirty",
                "未コミット",
                "Branch",
                "ブランチ",
                "Last commit",
                "最終コミット",
            ] {
                let expected_in_this_lang = (lang == Language::Japanese) != label.is_ascii();
                if !expected_in_this_lang {
                    continue;
                }
                assert!(
                    frame_contains(&text, label),
                    "{label:?} truncated in {lang:?}:\n{text}"
                );
            }
        }
    }

    /// At 80 columns the full Japanese tab bar overflowed and `7 ワークツリー`
    /// disappeared entirely — no hint that a seventh tab existed.
    #[test]
    fn every_tab_stays_visible_on_a_narrow_terminal() {
        for lang in [Language::English, Language::Japanese] {
            let mut a = app(lang);
            a.screen = Screen::Repo;
            a.repo_index = Some(0);
            for width in [80u16, 100, 200] {
                let text = render_to_text(&a, width, 20);
                for n in 1..=7 {
                    assert!(
                        text.contains(&n.to_string()),
                        "tab {n} missing at {width} cols in {lang:?}:\n{text}"
                    );
                }
                let labels = repo_tab_labels(&a, width);
                assert_eq!(labels.len(), 7);
                assert!(
                    tab_bar_width(&labels) <= (width - 2) as usize,
                    "tab bar {} wide does not fit {width} cols in {lang:?}: {labels:?}",
                    tab_bar_width(&labels)
                );
            }
        }
    }

    #[test]
    fn a_wide_terminal_keeps_the_full_tab_labels() {
        let mut a = app(Language::Japanese);
        a.screen = Screen::Repo;
        a.repo_index = Some(0);
        let text = render_to_text(&a, 120, 20);
        assert!(frame_contains(&text, "コントリビューター"), "{text}");
        assert!(frame_contains(&text, "ワークツリー"), "{text}");
    }

    /// Clicking a tab must land on the tab under the cursor. The old handler
    /// used fixed column ranges derived from the English labels, so in
    /// Japanese — where every label is a different width — it picked the wrong
    /// one.
    #[test]
    fn clicking_a_tab_selects_the_tab_actually_drawn_there() {
        for lang in [Language::English, Language::Japanese] {
            for width in [80u16, 120] {
                let mut a = app(lang);
                a.screen = Screen::Repo;
                a.repo_index = Some(0);
                let text = render_to_text(&a, width, 20);
                let labels = repo_tab_labels(&a, width);
                // Row 0 is the title bar and row 1 the box border; the tab
                // labels are drawn on row 2.
                let tab_row: String = text
                    .chars()
                    .skip(2 * width as usize)
                    .take(width as usize)
                    .collect();

                for (i, tab) in RepoTab::all().iter().enumerate() {
                    // Click where this label's text really is on screen.
                    let at = text_column_of(&tab_row, &labels[i]).unwrap_or_else(|| {
                        panic!(
                            "label {:?} not on the tab row in {lang:?}: {tab_row:?}",
                            labels[i]
                        )
                    });
                    a.handle_mouse_click(at, 2);
                    assert_eq!(
                        a.repo_tab, *tab,
                        "click at x={at} ({:?}) selected the wrong tab at {width} cols in {lang:?}",
                        labels[i]
                    );
                }
            }
        }
    }
}

#[cfg(test)]
mod polish_tests {
    use super::footer_and_help_tests::frame_contains;
    use super::tests::render_to_text;
    use super::*;
    use crate::app::RepoSnapshot;

    #[test]
    fn truncate_middle_keeps_both_ends_and_marks_the_gap() {
        assert_eq!(truncate_middle("/short/path", 40), "/short/path");
        let long = "/home/someone/Library/Application Support/com.git-dashboard.git-dashboard";
        let out = truncate_middle(long, 40);
        use unicode_width::UnicodeWidthStr;
        assert_eq!(out.width(), 40);
        assert!(out.contains('…'));
        // The tail names the directory, so it is the half worth keeping.
        assert!(out.ends_with("git-dashboard"), "{out}");
        assert!(out.starts_with("/home"), "{out}");
    }

    #[test]
    fn truncate_middle_counts_display_columns() {
        use unicode_width::UnicodeWidthStr;
        let s = "/日本語/とても/長い/パス/設定";
        let out = truncate_middle(s, 15);
        assert!(out.width() <= 15, "{out} is {} wide", out.width());
        assert!(out.contains('…'));
    }

    #[test]
    fn truncate_middle_degrades_rather_than_panicking_on_no_room() {
        for max in 0..3 {
            let out = truncate_middle("/a/very/long/path", max);
            use unicode_width::UnicodeWidthStr;
            assert!(out.width() <= max.max(1), "max={max} gave {out:?}");
        }
    }

    /// The settings screen showed the config path chopped at the frame edge,
    /// with nothing to say it had been.
    #[test]
    fn the_settings_config_path_is_elided_not_chopped() {
        let mut app = App::new();
        app.screen = Screen::Settings;
        let width = 60usize;
        let narrow = render_to_text(&app, width as u16, 24);
        assert!(narrow.contains('…'), "no elision marker:\n{narrow}");
        // The frame dump is a flat grid, so slice it back into rows: the
        // elided path must sit inside the box, not run over its right border.
        let path_row: String = (0..24)
            .map(|r| {
                narrow
                    .chars()
                    .skip(r * width)
                    .take(width)
                    .collect::<String>()
            })
            .find(|l| l.contains('…'))
            .expect("no row holds the elided path");
        assert!(
            path_row.trim_end().ends_with('│'),
            "path spills past the box border: {path_row:?}"
        );
    }

    #[test]
    fn the_contributors_title_has_no_stray_trailing_space() {
        let mut app = App::new();
        app.repos = vec![crate::config::Repository {
            name: "r".to_string(),
            path: std::path::PathBuf::from("/tmp/repo"),
            group: None,
        }];
        app.repo_index = Some(0);
        app.screen = Screen::Repo;
        app.repo_tab = RepoTab::Contributors;
        app.repo_data = Some(RepoSnapshot {
            summary: crate::git::Summary::default(),
            commits: vec![],
            commits_err: None,
            branches: vec![],
            branches_err: None,
            tags: vec![],
            tags_err: None,
            stashes: vec![],
            stashes_err: None,
            working_files: vec![],
            working_err: None,
            contributors: vec![],
            contributors_err: None,
            worktrees: vec![],
            worktrees_err: None,
        });
        let text = render_to_text(&app, 100, 20);
        assert!(
            frame_contains(&text, "[Alltime]─"),
            "expected the border to follow the period label directly:\n{text}"
        );
    }
}

#[cfg(test)]
mod activity_tests {
    use super::footer_and_help_tests::frame_contains;
    use super::tests::render_to_text;
    use super::*;
    use crate::app::{Activity, SPINNER_FRAMES};
    use crate::config::Language;

    fn app_with(n: usize) -> App {
        let mut app = App::new();
        app.repos = (0..n)
            .map(|i| crate::config::Repository {
                name: format!("repo-{i}"),
                path: std::path::PathBuf::from(format!("/tmp/r{i}")),
                group: None,
            })
            .collect();
        app.screen = Screen::Home;
        app.busy.clear();
        app
    }

    #[test]
    fn a_busy_repository_shows_a_spinner_and_the_operation() {
        let mut app = app_with(3);
        app.busy.insert(1, Activity::Fetch);
        let text = render_to_text(&app, 120, 16);
        let row = row_region(&text, 120, 4..5);
        assert!(row.contains("repo-1"), "wrong region: {row:?}");
        assert!(
            row.contains("fetch"),
            "no operation label in the row: {row:?}"
        );
        assert!(
            SPINNER_FRAMES.iter().any(|f| row.contains(f)),
            "no spinner frame in the row: {row:?}"
        );
    }

    #[test]
    fn each_operation_names_itself() {
        for (act, label) in [
            (Activity::Pull, "pull"),
            (Activity::Fetch, "fetch"),
            (Activity::Refresh, "refresh"),
        ] {
            let mut app = app_with(2);
            app.busy.insert(0, act);
            let text = render_to_text(&app, 120, 16);
            let row = row_region(&text, 120, 3..4);
            assert!(row.contains(label), "{label} missing from the row: {row:?}");
        }
    }

    /// Scoped to the table rows: the footer legitimately carries a
    /// `P/F pull/fetch all` hint, and matching on the whole frame would find
    /// that instead of a row and pass regardless.
    fn row_region(text: &str, width: usize, rows: std::ops::Range<usize>) -> String {
        rows.map(|r| text.chars().skip(r * width).take(width).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn an_idle_repository_shows_no_operation_in_its_row() {
        let app = app_with(2);
        let text = render_to_text(&app, 120, 16);
        let rows = row_region(&text, 120, 3..5);
        assert!(rows.contains("repo-0"), "wrong region: {rows:?}");
        for label in ["pull", "fetch", "refresh"] {
            assert!(!rows.contains(label), "{label} shown while idle: {rows:?}");
        }
    }

    #[test]
    fn the_title_bar_counts_running_jobs_in_both_languages() {
        let mut app = app_with(3);
        app.busy.insert(0, Activity::Pull);
        app.busy.insert(2, Activity::Pull);
        let en = render_to_text(&app, 120, 16);
        assert!(en.contains("2 jobs running"), "{en}");

        app.set_language_for_test(Language::Japanese);
        let ja = render_to_text(&app, 120, 16);
        assert!(frame_contains(&ja, "実行中2件"), "{ja}");
    }

    #[test]
    fn english_says_one_job_not_one_jobs() {
        let mut app = app_with(2);
        app.busy.insert(0, Activity::Fetch);
        let text = render_to_text(&app, 120, 16);
        assert!(text.contains("1 job running"), "{text}");
        assert!(!text.contains("1 jobs"), "{text}");
    }

    #[test]
    fn nothing_is_shown_when_nothing_is_running() {
        let app = app_with(2);
        let text = render_to_text(&app, 120, 16);
        assert!(!text.contains("running"), "{text}");
        assert_eq!(app.busy_count(), 0);
    }

    /// Screens that already track their own load state must count too, or the
    /// title bar would claim idle while a diff is still being fetched.
    #[test]
    fn screen_level_loads_count_towards_the_running_total() {
        let mut app = app_with(1);
        assert_eq!(app.busy_count(), 0);
        app.repo_loading = true;
        assert_eq!(app.busy_count(), 1);
        app.global_members_loading = true;
        assert_eq!(app.busy_count(), 2);
        app.busy.insert(0, Activity::Pull);
        assert_eq!(app.busy_count(), 3);
    }

    /// Driven by elapsed time, so it keeps turning at a steady rate however
    /// often the screen is redrawn.
    #[test]
    fn the_spinner_advances_with_time() {
        let app = app_with(1);
        let first = app.spinner();
        std::thread::sleep(std::time::Duration::from_millis(
            crate::app::SPINNER_INTERVAL_MS as u64 + 30,
        ));
        assert_ne!(first, app.spinner(), "spinner did not advance");
    }

    #[test]
    fn every_spinner_frame_is_one_column_wide() {
        use unicode_width::UnicodeWidthStr;
        for f in SPINNER_FRAMES {
            assert_eq!(f.width(), 1, "{f:?} would shift the cell it sits in");
        }
    }
}
