use crate::app::{
    App, CiOutcome, FocusPane, RepoTab, Screen, classify_ci_status, column_for_sort_mode,
    short_hash, sort_is_ascending,
};
use crate::colors::Palette;
use crate::git::{CommitRef, DiffRowKind, GitOpState};
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Margin, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, Borders, Cell, Clear, List, ListItem, ListState, Paragraph, Row, Table, TableState,
    Tabs, Wrap,
};

/// Selection marker used by the Home table. Its width shifts every column
/// right, so the click hit-test has to account for it too.
const HIGHLIGHT_SYMBOL: &str = "▸ ";

pub fn draw(frame: &mut Frame, app: &App) {
    let pal = Palette::for_name(app.theme_name());
    let area = frame.area();
    frame.render_widget(
        Block::default().style(Style::default().bg(pal.bg).fg(pal.text)),
        area,
    );

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(2),
        ])
        .split(area);

    draw_title(frame, app, chunks[0], pal);
    match app.screen {
        Screen::Home => draw_home(frame, app, chunks[1], pal),
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

    if app.is_adding_repo() {
        draw_prompt(frame, area, &app.prompt_title(), app.input_buf(), pal);
    }
    if let Some(msg) = app.confirm_message() {
        draw_prompt(frame, area, &msg, "", pal);
    }
}

fn draw_title(frame: &mut Frame, app: &App, area: Rect, pal: Palette) {
    let title = match app.screen {
        Screen::Home => app.tt("Repositories", "リポジトリ一覧"),
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
    let bar = Paragraph::new(Line::from(vec![
        Span::styled(
            format!(" {title} "),
            Style::default()
                .fg(pal.bg)
                .bg(pal.accent)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("  git-dashboard-tui", Style::default().fg(pal.muted)),
    ]));
    frame.render_widget(bar, area);
}

fn draw_footer(frame: &mut Frame, app: &App, area: Rect, pal: Palette) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Length(1)])
        .split(area);
    frame.render_widget(
        Paragraph::new(shortcut_line(&app.footer_hints(), pal))
            .style(Style::default().bg(pal.surface)),
        rows[0],
    );
    let (msg, color) = if let Some(e) = app.error.as_deref() {
        (e, pal.red)
    } else if !app.status.is_empty() {
        (app.status.as_str(), pal.yellow)
    } else if app.is_filtering() {
        (app.input_buf(), pal.accent)
    } else {
        ("", pal.muted)
    };
    frame.render_widget(
        Paragraph::new(msg).style(Style::default().fg(color).bg(pal.surface)),
        rows[1],
    );
}

fn draw_home(frame: &mut Frame, app: &App, area: Rect, pal: Palette) {
    if app.repos.is_empty() {
        let body = vec![
            Line::from(""),
            Line::from(Span::styled(
                app.tt("No repositories registered.", "リポジトリがありません。"),
                Style::default().fg(pal.text).add_modifier(Modifier::BOLD),
            )),
            Line::from(Span::styled(
                app.tt(
                    "Press a  and enter a path  (e.g. ~/work/my-repo)",
                    "a を押してパスを入力  (例: ~/work/my-repo)",
                ),
                Style::default().fg(pal.subtext),
            )),
        ];
        frame.render_widget(
            Paragraph::new(body).block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(pal.border))
                    .title(app.t("repositories")),
            ),
            area,
        );
        return;
    }

    let indices = app.filtered_home();
    // The sorted column carries the direction marker, so the current sort is
    // visible in the table itself rather than only in the title.
    let sorted_col = column_for_sort_mode(app.sort_mode());
    let marker = if sort_is_ascending(app.sort_mode()) {
        "▲"
    } else {
        "▼"
    };
    let header_cells: Vec<Cell> = [
        app.tt("Name", "名前"),
        app.tt("Branch", "ブランチ"),
        app.tt("Sync", "同期"),
        app.tt("Dirty", "未コミット"),
        app.tt("Updated", "更新"),
        app.tt("Path", "パス"),
    ]
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

    let rows: Vec<Row> = indices
        .iter()
        .map(|&i| {
            let repo = &app.repos[i];
            let (branch, sync, dirty, updated, dirty_n, ahead, behind) = match app.home_rows.get(&i)
            {
                Some(r) => (
                    r.branch.clone(),
                    format!("↑{} ↓{}", r.ahead, r.behind),
                    r.dirty.to_string(),
                    r.last_commit.clone(),
                    r.dirty,
                    r.ahead,
                    r.behind,
                ),
                None => (
                    "…".to_string(),
                    "…".to_string(),
                    "…".to_string(),
                    "…".to_string(),
                    0,
                    0,
                    0,
                ),
            };
            let sync_style = if ahead + behind > 0 {
                Style::default().fg(pal.yellow)
            } else {
                Style::default().fg(pal.muted)
            };
            let dirty_style = if dirty_n > 0 {
                Style::default().fg(pal.red)
            } else {
                Style::default().fg(pal.green)
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
            let mut branch_spans = vec![Span::styled(branch, Style::default().fg(pal.accent))];
            if let Some(row_data) = app.home_rows.get(&i) {
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
                        format!("[PR:{prs}]"),
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
                Cell::from(Span::styled(dirty, dirty_style)),
                Cell::from(Span::styled(updated, Style::default().fg(pal.subtext))),
                Cell::from(Span::styled(
                    repo.path.display().to_string(),
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

    let widths = [
        Constraint::Length(26),
        Constraint::Length(30),
        Constraint::Length(10),
        Constraint::Length(8),
        Constraint::Length(18),
        Constraint::Min(10),
    ];
    // Record where each column actually landed so click-to-sort hit-tests
    // against the real layout instead of a second copy of this arithmetic.
    // `Table` lays its columns out inside the block, after the highlight
    // symbol, with `column_spacing` between them.
    {
        let inner = area.inner(Margin::new(1, 1));
        let sym_w = HIGHLIGHT_SYMBOL.chars().count() as u16;
        let cols_area = Rect {
            x: inner.x.saturating_add(sym_w),
            width: inner.width.saturating_sub(sym_w),
            ..inner
        };
        let cols = Layout::horizontal(widths).spacing(1).split(cols_area);
        *app.home_col_bounds.borrow_mut() = cols.iter().map(|r| (r.x, r.x + r.width)).collect();
    }

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

fn draw_repo(frame: &mut Frame, app: &App, area: Rect, pal: Palette) {
    let tabs = RepoTab::all();
    let labels: Vec<Line> = tabs
        .iter()
        .map(|t| {
            let s = match t {
                RepoTab::Status => app.tt("1 Status", "1 状態"),
                RepoTab::Commits => app.tt("2 Commits", "2 コミット"),
                RepoTab::Branches => format!("3 {}", app.t("tab_branches")),
                RepoTab::Tags => app.tt("4 Tags", "4 タグ"),
                RepoTab::Stash => format!("5 {}", app.tt("Stash", "Stash")),
                RepoTab::Contributors => app.tt("6 Contributors", "6 コントリビューター"),
                RepoTab::Worktrees => app.tt("7 Worktrees", "7 ワークツリー"),
            };
            Line::from(s)
        })
        .collect();
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

    if app.repo_loading && app.repo_data.is_none() {
        frame.render_widget(
            Paragraph::new(app.t("analyzing_repo_data"))
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
            Span::styled(
                format!("{:<20}", format!("↑{} ↓{}", s.ahead, s.behind)),
                Style::default().fg(pal.yellow),
            ),
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
                        format!("{:<10}", format!("{} open", prs)),
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
                        format!("  {:<6}", "CI:"),
                        Style::default().fg(pal.muted),
                    ));
                    spans.push(Span::styled(ci_icon, ci_style));
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
    let title = match (&app.commit_base, &app.commit_target) {
        (Some(b), Some(t)) => format!("Commits  {b}...{t}  (enter to compare)"),
        (Some(b), None) => format!("Commits  base={b}  (space to pick target)"),
        _ => format!(
            "{} ({}/{})",
            app.tt("Commits", "コミット"),
            vis.len(),
            data.commits.len()
        ),
    };
    render_items(
        frame,
        split[0],
        pal,
        items,
        app.list_selected,
        title,
        app.list_error(),
        &app.tt("No commits to show.", "表示するコミットがありません。"),
    );

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
        "{} ({}/{}) [{period_label}] {}",
        app.tt("Contributors", "貢献者"),
        vis.len(),
        data.contributors.len(),
        if app.active_only {
            app.tt("[active members]", "[在籍メンバーのみ]")
        } else {
            String::new()
        }
    );
    render_items(
        frame,
        area,
        pal,
        items,
        app.list_selected,
        title,
        app.list_error(),
        &app.tt("No contributors.", "貢献者がいません。"),
    );
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
    render_items(
        frame,
        area,
        pal,
        items,
        app.list_selected,
        format!("{} ({})", app.t("tab_branches"), vis.len()),
        app.list_error(),
        &app.tt("No branches.", "ブランチがありません。"),
    );
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
    let title = match (&app.tag_base, &app.tag_target) {
        (Some(b), Some(t)) => format!("Tags  {b}...{t}  (enter to diff)"),
        (Some(b), None) => format!("Tags  base={b}  (space to pick target)"),
        _ => format!("Tags ({})  (space to mark base/target)", vis.len()),
    };
    render_items(
        frame,
        area,
        pal,
        items,
        app.list_selected,
        title,
        app.list_error(),
        &app.tt("No tags.", "タグがありません。"),
    );
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
    render_items(
        frame,
        area,
        pal,
        items,
        app.list_selected,
        format!("Stash ({})", vis.len()),
        app.list_error(),
        &app.tt("No stashes.", "stash はありません。"),
    );
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
            app.t("fetching_diff"),
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
            Span::styled(
                app.t("config_file_location"),
                Style::default().fg(pal.muted),
            ),
            Span::raw("  "),
            Span::raw(crate::config::get_config_dir().display().to_string()),
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
            render_items(
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
            );
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
            render_items(
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
            );
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
            Paragraph::new(app.tt("Searching commits...", "コミットを検索中..."))
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
    render_items(
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
}

fn draw_global_members(frame: &mut Frame, app: &App, area: Rect, pal: Palette) {
    // The aggregation walks git log in every repository on a worker thread, so
    // the screen opens before any results exist.
    if app.global_members_loading && app.global_members.is_empty() {
        frame.render_widget(
            Paragraph::new(app.tt("Aggregating members...", "メンバーを集計しています..."))
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
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(4), Constraint::Min(1)])
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

    render_items(
        frame,
        chunks[1],
        pal,
        items,
        finder.selected_idx,
        list_title,
        None,
        &app.tt(
            "No git repositories found in this folder. Press 'r' to scan another folder.",
            "このフォルダ内に Git リポジトリは見つかりませんでした。'r' で別フォルダをスキャンしてください。",
        ),
    );
}

fn draw_help(frame: &mut Frame, app: &App, area: Rect, pal: Palette) {
    let lines = vec![
        Line::from(Span::styled(
            app.tt("Global", "全体"),
            Style::default().fg(pal.accent).add_modifier(Modifier::BOLD),
        )),
        Line::from("  q / Ctrl+C     quit"),
        Line::from("  Esc / h        back one screen"),
        Line::from("  ?              toggle this help (returns here)"),
        Line::from("  /              filter current list"),
        Line::from("  g / G          first / last"),
        Line::from("  t / T          open terminal in repo directory"),
        Line::from(""),
        Line::from(Span::styled(
            app.t("repositories"),
            Style::default().fg(pal.accent).add_modifier(Modifier::BOLD),
        )),
        Line::from("  j k            move    enter open    a/d add/delete"),
        Line::from("  A              scan folder and bulk import git repositories"),
        Line::from("  [ / ]          switch repository group filter"),
        Line::from("  P / F          bulk pull / bulk fetch all filtered repos"),
        Line::from("  p / f          pull / fetch single repo"),
        Line::from("  t              open terminal in repository"),
        Line::from("  M              open Global Members view (cross-repo)"),
        Line::from("  S              search commit messages across all repositories"),
        Line::from("  n              toggle: only repos needing attention (failing CI,"),
        Line::from("                 unresolved conflict, or a mid-operation merge/rebase)"),
        Line::from("  o / e          sort repos / rename alias"),
        Line::from("  r / s          reload / settings"),
        Line::from(""),
        Line::from(Span::styled(
            app.tt("Global Members View", "全リポジトリ横断メンバー画面"),
            Style::default().fg(pal.accent).add_modifier(Modifier::BOLD),
        )),
        Line::from("  Tab / h / l    switch pane between members and repo list"),
        Line::from("  Enter          jump directly into selected repository"),
        Line::from("  space / t      toggle member active / inactive"),
        Line::from("  m              filter active members only"),
        Line::from("  /              search members or repositories"),
        Line::from(""),
        Line::from(Span::styled(
            app.tt(
                "Repository & Contributors",
                "リポジトリ & コントリビューター",
            ),
            Style::default().fg(pal.accent).add_modifier(Modifier::BOLD),
        )),
        Line::from(
            "  1 Status  2 Commits  3 Branches  4 Tags  5 Stash  6 Contributors  7 Worktrees",
        ),
        Line::from("  enter          commit/file/stash diff, branch log, or shell in Worktree"),
        Line::from("  p / f          pull / fetch this repository"),
        Line::from(
            "  space          mark commit/tag base+target, or toggle active in Contributors",
        ),
        Line::from("  w              cycle time span filter (All / 1w / 1m / 3m)"),
        Line::from("  m              filter active members only (in Contributors tab)"),
        Line::from("  i              always open builtin TUI diff"),
        Line::from("  c              set external diff (empty = builtin, e.g. hunk)"),
        Line::from("  r              reload without leaving the tab"),
        Line::from(""),
        Line::from(Span::styled(
            app.tt("Settings", "設定画面"),
            Style::default().fg(pal.accent).add_modifier(Modifier::BOLD),
        )),
        Line::from("  Tab / 1 / 2    switch between Repositories and Members tabs"),
        Line::from("  g              edit repository group"),
        Line::from("  space / t      toggle member active / inactive"),
        Line::from("  a / e / d      add / edit aliases / delete member or repo"),
        Line::from("  l / c          toggle language / change diff tool"),
        Line::from("  i              cycle Home auto-refresh interval (off/30s/1m/5m)"),
        Line::from("  T              toggle theme (Catppuccin Mocha / Latte)"),
        Line::from(""),
        Line::from(Span::styled(
            app.t("diff"),
            Style::default().fg(pal.accent).add_modifier(Modifier::BOLD),
        )),
        Line::from("  Tab / h l      files ↔ hunks ↔ diff"),
        Line::from("  n / p          next/prev hunk (wraps; highlights current)"),
        Line::from("  [ / ]          previous/next changed file"),
        Line::from("  w              toggle ignore-whitespace"),
        Line::from("  f              toggle full-file context"),
        Line::from("  b              toggle blame gutter (hash + author per line)"),
    ];
    frame.render_widget(
        Paragraph::new(lines).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(pal.border)),
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
    render_items(
        frame,
        area,
        pal,
        items,
        app.list_selected,
        title,
        app.list_error(),
        &empty,
    );
}

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
) {
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
        return;
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
        return;
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
        .highlight_symbol("▸ ");
    let mut state = ListState::default();
    state.select(Some(selected.min(n.saturating_sub(1))));
    frame.render_stateful_widget(list, area, &mut state);
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

fn shortcut_line<'a>(hints: &[(String, String)], pal: Palette) -> Line<'a> {
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
mod tests {
    use super::*;
    use crate::app::{RepoSnapshot, Screen};
    use crate::git::{Summary, WorktreeInfo};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn render(app: &App, width: u16, height: u16) {
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
            render(&app, 80, 24);
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
        render(&app, 80, 24);
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
            ("Updated", 4),
        ] {
            let found = header
                .find(label)
                .unwrap_or_else(|| panic!("header {label:?} not rendered in: {header:?}"));
            let (start, end) = bounds[col];
            assert!(
                found as u16 >= start && (found as u16) < end,
                "{label} renders at x={found} but column {col} was recorded as {start}..{end}"
            );
            // And the hit-test agrees for that x.
            assert_eq!(
                app.home_column_at(found as u16),
                Some(col),
                "hit-test for {label}"
            );
        }
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
