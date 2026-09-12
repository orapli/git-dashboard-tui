use super::*;
use crate::clipboard::{self, ClipboardError};
use crate::config::CustomCommand;
use crate::handoff::{
    self, EditStage, MAX_CUSTOM_COMMANDS, PLACEHOLDERS, TemplateError, ToolContext, ToolEditor,
};
use std::path::Path;

/// Sentinel `DiffView::target` uses for "the working tree", written by
/// `activate_repo_item`. It is not a revision, so `{hash}` must not offer it.
const WORKING_TREE: &str = "WORKING_TREE";

/// Build an `ExternalDiff` from a command template and the context the user
/// is looking at. The program name is returned unresolved — `resolve_program`
/// is what vets it, and it must keep doing so.
pub fn build_command(template: &str, ctx: &ToolContext) -> Result<ExternalDiff, TemplateError> {
    let (program, args) = handoff::expand(template, ctx)?;
    Ok(ExternalDiff {
        program,
        args,
        cwd: ctx.repo.clone(),
    })
}

/// The editor command with only a repository root for context, as every
/// caller outside the tool menu wants it.
pub fn editor_command(template: &str, path: &Path) -> Result<ExternalDiff, TemplateError> {
    build_command(template, &ToolContext::for_repo(path))
}

/// First token of a template, i.e. the program it would run. `None` when the
/// template is empty or cannot be split.
fn template_program(template: &str) -> Option<String> {
    shell_words::split(template)
        .ok()?
        .into_iter()
        .next()
        .filter(|s| !s.is_empty())
}

/// The line the diff view is currently on, as a file line number.
///
/// The topmost visible row may be a hunk header, which has no number, so the
/// first numbered row at or below it is the honest answer to "which line am I
/// looking at".
fn current_diff_line(diff: &DiffView) -> Option<usize> {
    diff.lines
        .iter()
        .skip(diff.scroll)
        .find_map(|l| l.new_no.or(l.old_no))
}

impl App {
    pub fn take_work_tool(&mut self) -> Option<WorkTool> {
        self.pending_work_tool.take()
    }

    pub fn open_tool_menu(&mut self, path: PathBuf) {
        self.tool_menu = Some(path);
    }

    pub(super) fn queue_shell(&mut self, path: PathBuf) {
        if git::parse_ssh_repo(&path).is_some() {
            self.error = Some(self.tt(
                "Local tools are unavailable for SSH repositories",
                "SSHリポジトリではローカルツールを起動できません",
            ));
        } else if !path.is_dir() {
            self.error = Some(self.tt(
                "Repository path does not exist",
                "リポジトリのパスが存在しません",
            ));
        } else {
            self.pending_terminal = Some(path);
        }
    }

    /// What the placeholders in a command template mean *right now*.
    ///
    /// Only the repository that is actually open can supply a file or a
    /// commit: on Home or the worktree list the highlighted row is often a
    /// different repository from the one the diff view last showed, and
    /// handing that repository's file path to a tool launched in another one
    /// would be worse than having no value at all.
    pub(super) fn tool_context(&self, path: &Path) -> ToolContext {
        let mut ctx = ToolContext::for_repo(path);
        let current = self
            .repo_index
            .filter(|&i| self.repos.get(i).is_some_and(|r| r.path == path));
        let Some(current) = current else {
            ctx.branch = self
                .repos
                .iter()
                .position(|r| r.path == path)
                .and_then(|i| self.home_rows.get(&i))
                .map(|row| row.branch.clone())
                .filter(|b| !b.is_empty());
            return ctx;
        };
        ctx.branch = self
            .repo_data
            .as_ref()
            .map(|d| d.summary.current_branch.clone())
            .or_else(|| self.home_rows.get(&current).map(|row| row.branch.clone()))
            .filter(|b| !b.is_empty());

        match self.screen {
            Screen::Diff => {
                if let Some(diff) = self.diff.as_ref() {
                    if let Some(file) = diff.files.get(diff.file_idx) {
                        ctx.file = Some(handoff::absolute_file(path, &file.path));
                    }
                    ctx.line = current_diff_line(diff);
                    if diff.target != WORKING_TREE {
                        ctx.hash = Some(diff.target.clone());
                    }
                }
            }
            Screen::Repo => {
                let item = self.selected_item_index();
                if let (Some(item), Some(data)) = (item, self.repo_data.as_ref()) {
                    match self.repo_tab {
                        RepoTab::Status => {
                            ctx.file = data
                                .working_files
                                .get(item)
                                .map(|f| handoff::absolute_file(path, &f.path));
                        }
                        RepoTab::Commits => {
                            ctx.hash = data.commits.get(item).map(|c| c.hash.clone());
                        }
                        RepoTab::Branches => {
                            if let Some(b) = data.branches.get(item) {
                                ctx.branch = Some(b.name.clone());
                            }
                        }
                        RepoTab::Tags => {
                            ctx.hash = data.tags.get(item).map(|t| t.hash.clone());
                        }
                        RepoTab::Stash => {
                            ctx.hash = data.stashes.get(item).map(|s| s.ref_name.clone());
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
        ctx
    }

    /// Why a template could not be turned into a command, in the user's
    /// language, naming the placeholder and where its value would come from.
    fn template_error_message(&self, err: &TemplateError) -> String {
        let placeholder = err.placeholder().unwrap_or_default();
        match err {
            TemplateError::Empty => self.tt("Command is empty", "コマンドが空です"),
            TemplateError::Split(e) => format!(
                "{}: {e}",
                self.tt("Cannot parse command", "コマンドを解析できません")
            ),
            TemplateError::ProgramPlaceholder => self.tt(
                "A placeholder cannot choose which program runs",
                "実行するプログラム名にプレースホルダは使えません",
            ),
            TemplateError::Missing(_) => format!(
                "{placeholder} {} {}",
                self.tt("has no value here.", "はこの画面では値がありません。"),
                self.placeholder_source(placeholder)
            ),
            TemplateError::ControlChar(_) => format!(
                "{placeholder} {}",
                self.tt(
                    "contains a control character and was refused",
                    "に制御文字が含まれるため実行しませんでした"
                )
            ),
            TemplateError::OptionLike(_) => format!(
                "{placeholder} {}",
                self.tt(
                    "starts with '-' and would be read as an option — refused",
                    "が '-' で始まりオプションとして解釈されるため実行しませんでした"
                )
            ),
        }
    }

    /// Where the value for a placeholder comes from, so a refusal says what
    /// to do instead of only what went wrong.
    fn placeholder_source(&self, placeholder: &str) -> String {
        match placeholder {
            "{file}" | "{line}" => self.tt(
                "Open a file in the diff view first.",
                "先に diff でファイルを開いてください。",
            ),
            "{branch}" => self.tt(
                "Open the repository first.",
                "先にリポジトリを開いてください。",
            ),
            "{hash}" => self.tt(
                "Select a commit, tag or stash first.",
                "先にコミット・タグ・stash を選んでください。",
            ),
            _ => String::new(),
        }
    }

    /// `false` (with the error already set) when local tools cannot run
    /// against this path at all.
    fn local_tool_ready(&mut self, path: &Path) -> bool {
        if git::parse_ssh_repo(path).is_some() || !path.is_dir() {
            self.error = Some(self.tt(
                "Local tools require an existing local directory",
                "ローカルツールには存在するローカルディレクトリが必要です",
            ));
            return false;
        }
        true
    }

    /// Expand a template against the current context, vet the program, and
    /// queue it. Closes the menu only on success, so a refusal leaves the
    /// user where they can fix it.
    fn launch_template(&mut self, path: &Path, template: &str, wait: bool) {
        if !self.local_tool_ready(path) {
            return;
        }
        let ctx = self.tool_context(path);
        let built = build_command(template, &ctx).map_err(|e| self.template_error_message(&e));
        match built.and_then(|mut c| {
            c.program = crate::resolve_program(&c.program)?
                .to_string_lossy()
                .into_owned();
            Ok(c)
        }) {
            Ok(command) => {
                self.pending_work_tool = Some(WorkTool { command, wait });
                self.tool_menu = None;
            }
            Err(e) => self.error = Some(e),
        }
    }

    /// The custom entries the `O` menu shows, capped so the popup stays a
    /// glanceable list. A hand-edited prefs.json with more is not an error;
    /// the extras are simply not offered, and the menu says so.
    pub fn menu_custom_commands(&self) -> &[CustomCommand] {
        let n = self.prefs.custom_commands.len().min(MAX_CUSTOM_COMMANDS);
        &self.prefs.custom_commands[..n]
    }

    pub fn custom_commands(&self) -> &[CustomCommand] {
        &self.prefs.custom_commands
    }

    pub fn tool_menu_lines(&self) -> Vec<String> {
        let Some(path) = &self.tool_menu else {
            return vec![];
        };
        if git::parse_ssh_repo(path).is_some() {
            return vec![
                path.display().to_string(),
                self.tt(
                    "Local shell/editor/clients are unsupported over SSH.",
                    "SSHのシェル・エディタ・Gitクライアント起動は非対応です。",
                ),
                self.tt("Esc: close", "Esc: 閉じる"),
            ];
        }
        let availability = |program: &str| {
            if crate::resolve_program(program).is_ok() {
                String::new()
            } else {
                self.tt(" [not installed / not on PATH]", " [未導入・PATHなし]")
            }
        };
        let template_availability = |template: &str| match template_program(template) {
            Some(program) => availability(&program),
            None => self.tt(" [invalid command]", " [コマンドが不正]"),
        };
        let mut lines = vec![
            path.display().to_string(),
            self.tt("t  Shell", "t  シェル"),
            format!(
                "e  {}: {}{}",
                self.tt("Editor", "エディタ"),
                self.prefs.editor_command,
                template_availability(&self.prefs.editor_command)
            ),
            format!("l  lazygit{}", availability("lazygit")),
            format!("g  GitUI{}", availability("gitui")),
        ];
        for (i, cmd) in self.menu_custom_commands().iter().enumerate() {
            lines.push(format!(
                "{}  {}: {}{}",
                i + 1,
                cmd.label,
                cmd.command,
                template_availability(&cmd.command)
            ));
        }
        let hidden = self
            .prefs
            .custom_commands
            .len()
            .saturating_sub(MAX_CUSTOM_COMMANDS);
        if hidden > 0 {
            lines.push(format!(
                "   {} {}",
                self.tt(
                    "further custom commands are not shown:",
                    "件の追加コマンドは表示されません:"
                ),
                hidden
            ));
        }
        lines.push(self.tt("c  Configure editor command", "c  エディタコマンドを設定"));
        lines.push(format!(
            "w  {}: {}",
            self.tt("Wait for editor", "エディタ終了待機"),
            if self.prefs.editor_wait {
                self.tt("on (terminal editor)", "オン（端末内）")
            } else {
                self.tt("off (GUI)", "オフ（GUI）")
            }
        ));
        lines.push(format!(
            "x  {} ({}/{})",
            self.tt("Custom commands", "追加コマンド"),
            self.prefs.custom_commands.len(),
            MAX_CUSTOM_COMMANDS
        ));
        lines.push(self.tt(
            "y  Copy this path to the clipboard",
            "y  このパスをクリップボードにコピー",
        ));
        lines.push(format!(
            "{} {}",
            self.tt("Placeholders:", "プレースホルダ:"),
            PLACEHOLDERS.join(" ")
        ));
        lines.push(self.tt(
            "A placeholder with no value in this view cancels the launch.",
            "この画面で値のないプレースホルダがあると起動を中止します。",
        ));
        lines.push(self.tt(
            "Install clients on PATH. Esc: close",
            "クライアントをPATH上に導入してください。Esc: 閉じる",
        ));
        lines
    }

    pub(super) fn handle_tool_menu(&mut self, key: KeyEvent) {
        let Some(path) = self.tool_menu.clone() else {
            return;
        };
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => {
                self.tool_menu = None;
            }
            KeyCode::Char('c') => {
                self.input = Some(InputKind::EditorCommand);
                self.input_buf = self.prefs.editor_command.clone();
            }
            KeyCode::Char('w') => {
                self.prefs.editor_wait = !self.prefs.editor_wait;
                self.persist_prefs();
            }
            KeyCode::Char('x') => {
                self.tool_menu = None;
                self.open_tool_editor();
            }
            KeyCode::Char('y') => {
                let label = self.tt("path", "パス");
                let value = path.display().to_string();
                self.yank(&label, &value);
            }
            KeyCode::Char('t') => {
                self.tool_menu = None;
                self.queue_shell(path);
            }
            KeyCode::Char('e') => {
                let template = self.prefs.editor_command.clone();
                self.launch_template(&path, &template, self.prefs.editor_wait);
            }
            KeyCode::Char(c @ ('l' | 'g')) => {
                if !self.local_tool_ready(&path) {
                    return;
                }
                let command = ExternalDiff {
                    program: if c == 'l' { "lazygit" } else { "gitui" }.into(),
                    args: vec![],
                    cwd: path,
                };
                match crate::resolve_program(&command.program) {
                    Ok(program) => {
                        self.pending_work_tool = Some(WorkTool {
                            command: ExternalDiff {
                                program: program.to_string_lossy().into_owned(),
                                ..command
                            },
                            // Both are full-screen terminal clients: the TUI
                            // has to stand down while they own the terminal.
                            wait: true,
                        });
                        self.tool_menu = None;
                    }
                    Err(e) => self.error = Some(e),
                }
            }
            KeyCode::Char(d @ '1'..='9') => {
                let idx = d as usize - '1' as usize;
                let Some(cmd) = self.menu_custom_commands().get(idx).cloned() else {
                    return;
                };
                self.launch_template(&path, &cmd.command, cmd.wait);
            }
            _ => {}
        }
    }

    // ---- custom command editor -------------------------------------------

    /// Open the editor for `prefs.custom_commands` (`x` in Settings or in the
    /// `O` menu). Config is written through `config::write_atomic`, like every
    /// other preference change, via `persist_prefs`.
    pub fn open_tool_editor(&mut self) {
        let selected = self
            .prefs
            .custom_commands
            .len()
            .saturating_sub(1)
            .min(self.tool_editor.as_ref().map_or(0, |e| e.selected));
        self.tool_editor = Some(ToolEditor {
            selected,
            ..ToolEditor::default()
        });
    }

    pub fn tool_editor_lines(&self) -> Vec<String> {
        let Some(editor) = self.tool_editor.as_ref() else {
            return vec![];
        };
        match editor.stage {
            EditStage::Label => {
                return vec![
                    self.tt("Label shown in the O menu:", "O メニューに表示する名前:"),
                    format!("{}█", editor.buf),
                    self.tt("Enter: next   Esc: cancel", "Enter: 次へ   Esc: 取消"),
                ];
            }
            EditStage::Command => {
                return vec![
                    format!(
                        "{} {}",
                        self.tt("Command for", "コマンド:"),
                        editor.draft_label
                    ),
                    format!("{}█", editor.buf),
                    format!(
                        "{} {}",
                        self.tt("Placeholders:", "プレースホルダ:"),
                        PLACEHOLDERS.join(" ")
                    ),
                    self.tt("Enter: save   Esc: cancel", "Enter: 保存   Esc: 取消"),
                ];
            }
            EditStage::List => {}
        }
        let mut lines = Vec::new();
        if self.prefs.custom_commands.is_empty() {
            lines.push(self.tt("No custom commands yet.", "追加コマンドはまだありません。"));
        }
        for (i, cmd) in self.prefs.custom_commands.iter().enumerate() {
            lines.push(format!(
                "{}{} {} [{}]  {}{}",
                if i == editor.selected { "▸ " } else { "  " },
                if i < MAX_CUSTOM_COMMANDS {
                    (i + 1).to_string()
                } else {
                    "-".to_string()
                },
                if cmd.wait {
                    self.tt("wait", "待機")
                } else {
                    self.tt("no wait", "非待機")
                },
                cmd.label,
                cmd.command,
                if i < MAX_CUSTOM_COMMANDS {
                    String::new()
                } else {
                    self.tt(" (not in the menu)", "（メニュー対象外）")
                }
            ));
        }
        lines.push(String::new());
        lines.push(self.tt(
            "a: add   e: edit   d: delete   w: wait on/off   Esc: close",
            "a: 追加   e: 編集   d: 削除   w: 待機切替   Esc: 閉じる",
        ));
        lines
    }

    pub(super) fn handle_tool_editor(&mut self, key: KeyEvent) {
        let Some((stage, selected)) = self.tool_editor.as_ref().map(|e| (e.stage, e.selected))
        else {
            return;
        };
        if stage != EditStage::List {
            match key.code {
                KeyCode::Esc => self.cancel_tool_edit(),
                KeyCode::Enter => self.commit_tool_edit(),
                KeyCode::Backspace => {
                    if let Some(editor) = self.tool_editor.as_mut() {
                        editor.buf.pop();
                    }
                }
                KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                    if let Some(editor) = self.tool_editor.as_mut() {
                        editor.buf.push(c);
                    }
                }
                _ => {}
            }
            return;
        }
        let len = self.prefs.custom_commands.len();
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => self.tool_editor = None,
            KeyCode::Down | KeyCode::Char('j') => {
                self.set_tool_edit_selection(move_index(selected, len, 1))
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.set_tool_edit_selection(move_index(selected, len, -1))
            }
            KeyCode::Char('a') => {
                if len >= MAX_CUSTOM_COMMANDS {
                    self.error = Some(self.tt(
                        "The O menu holds six custom commands; delete one first",
                        "追加コマンドは6件までです。先に削除してください",
                    ));
                    return;
                }
                self.begin_tool_edit(None, String::new());
            }
            KeyCode::Char('e') => {
                let Some(label) = self
                    .prefs
                    .custom_commands
                    .get(selected)
                    .map(|c| c.label.clone())
                else {
                    return;
                };
                self.begin_tool_edit(Some(selected), label);
            }
            KeyCode::Char('d') => {
                if selected < len {
                    self.prefs.custom_commands.remove(selected);
                    self.set_tool_edit_selection(selected.min(len.saturating_sub(2)));
                    self.persist_prefs();
                }
            }
            KeyCode::Char('w') => {
                let Some(cmd) = self.prefs.custom_commands.get_mut(selected) else {
                    return;
                };
                cmd.wait = !cmd.wait;
                self.persist_prefs();
            }
            _ => {}
        }
    }

    fn set_tool_edit_selection(&mut self, index: usize) {
        if let Some(editor) = self.tool_editor.as_mut() {
            editor.selected = index;
        }
    }

    /// Start typing an entry: `target` is the index being replaced, or `None`
    /// to append a new one.
    fn begin_tool_edit(&mut self, target: Option<usize>, label: String) {
        if let Some(editor) = self.tool_editor.as_mut() {
            editor.target = target;
            editor.buf = label;
            editor.draft_label.clear();
            editor.stage = EditStage::Label;
        }
    }

    fn cancel_tool_edit(&mut self) {
        if let Some(editor) = self.tool_editor.as_mut() {
            editor.stage = EditStage::List;
            editor.buf.clear();
            editor.draft_label.clear();
            editor.target = None;
        }
    }

    /// Enter in the label step moves to the command step; in the command step
    /// it validates and saves.
    fn commit_tool_edit(&mut self) {
        let Some((stage, text, target, draft_label)) = self.tool_editor.as_ref().map(|e| {
            (
                e.stage,
                e.buf.trim().to_string(),
                e.target,
                e.draft_label.clone(),
            )
        }) else {
            return;
        };
        match stage {
            EditStage::Label => {
                if text.is_empty() {
                    self.error = Some(self.tt("A label is required", "名前を入力してください"));
                    return;
                }
                let existing = target
                    .and_then(|i| self.prefs.custom_commands.get(i))
                    .map(|c| c.command.clone())
                    .unwrap_or_default();
                if let Some(editor) = self.tool_editor.as_mut() {
                    editor.draft_label = text;
                    editor.buf = existing;
                    editor.stage = EditStage::Command;
                }
            }
            EditStage::Command => {
                // Validated against a bare context: this catches what is wrong
                // in *every* view (no program, a placeholder as the program,
                // an unbalanced quote) without rejecting a template whose
                // `{file}` merely has no value while it is being typed.
                if let Err(e) = handoff::expand(&text, &ToolContext::for_repo("/"))
                    && !matches!(e, TemplateError::Missing(_))
                {
                    self.error = Some(self.template_error_message(&e));
                    return;
                }
                let wait = target
                    .and_then(|i| self.prefs.custom_commands.get(i))
                    .is_some_and(|c| c.wait);
                let entry = CustomCommand {
                    label: draft_label,
                    command: text,
                    wait,
                };
                let index = match target {
                    Some(i) if i < self.prefs.custom_commands.len() => {
                        self.prefs.custom_commands[i] = entry;
                        i
                    }
                    _ => {
                        if self.prefs.custom_commands.len() >= MAX_CUSTOM_COMMANDS {
                            return;
                        }
                        self.prefs.custom_commands.push(entry);
                        self.prefs.custom_commands.len() - 1
                    }
                };
                self.set_tool_edit_selection(index);
                self.cancel_tool_edit();
                self.persist_prefs();
            }
            EditStage::List => {}
        }
    }

    // ---- yank ------------------------------------------------------------

    /// Copy the identifier that matters for the current selection.
    ///
    /// The dashboard's whole premise is that the real work happens elsewhere,
    /// and retyping a hash into that shell by hand is the tax on every
    /// hand-off.
    pub(super) fn yank_current(&mut self) {
        let Some((label, value)) = self.yank_target() else {
            self.status = self.tt(
                "Nothing to copy here",
                "ここにはコピーできる項目がありません",
            );
            return;
        };
        self.yank(&label, &value);
    }

    fn yank(&mut self, label: &str, value: &str) {
        match clipboard::copy(value) {
            Ok(copied) => {
                // Deliberately does not claim the clipboard was written: OSC
                // 52 is one-way and several terminals (and tmux without
                // `set-clipboard on`) drop it silently. The caveat comes
                // before the value because the footer is one line: a long
                // path must truncate away the value, never the caveat.
                self.status = format!(
                    "{} {label} {}: {copied}",
                    self.tt("Copied", "コピー送信"),
                    self.tt(
                        "(OSC 52 — your terminal may ignore it)",
                        "（OSC 52・無視する端末もあります）"
                    )
                );
            }
            Err(ClipboardError::Empty) => {
                self.error = Some(self.tt("Nothing to copy", "コピーできる文字列がありません"));
            }
            Err(ClipboardError::TooLong(n)) => {
                self.error = Some(format!(
                    "{} ({n} > {})",
                    self.tt("Too long to copy", "長すぎるためコピーしません"),
                    clipboard::MAX_CLIPBOARD_BYTES
                ));
            }
        }
    }

    /// What `y` copies on the current screen: the identifier a user would
    /// otherwise retype into the shell they just jumped to.
    fn yank_target(&self) -> Option<(String, String)> {
        let repo_path = self.current_work_path();
        match self.screen {
            Screen::Diff => {
                let diff = self.diff.as_ref()?;
                let path = repo_path?;
                if let Some(file) = diff.files.get(diff.file_idx) {
                    return Some((
                        self.tt("file", "ファイル"),
                        handoff::absolute_file(&path, &file.path)
                            .display()
                            .to_string(),
                    ));
                }
                if diff.target != WORKING_TREE {
                    return Some((self.tt("commit", "コミット"), diff.target.clone()));
                }
                Some((self.tt("path", "パス"), path.display().to_string()))
            }
            Screen::Repo => {
                let path = repo_path?;
                let fallback = || Some((self.tt("path", "パス"), path.display().to_string()));
                let (Some(item), Some(data)) =
                    (self.selected_item_index(), self.repo_data.as_ref())
                else {
                    return fallback();
                };
                match self.repo_tab {
                    RepoTab::Status => data.working_files.get(item).map(|f| {
                        (
                            self.tt("file", "ファイル"),
                            handoff::absolute_file(&path, &f.path).display().to_string(),
                        )
                    }),
                    RepoTab::Commits => data
                        .commits
                        .get(item)
                        .map(|c| (self.tt("commit", "コミット"), c.hash.clone())),
                    RepoTab::Branches => data
                        .branches
                        .get(item)
                        .map(|b| (self.tt("branch", "ブランチ"), b.name.clone())),
                    RepoTab::Tags => data
                        .tags
                        .get(item)
                        .map(|t| (self.tt("tag", "タグ"), t.name.clone())),
                    RepoTab::Stash => data
                        .stashes
                        .get(item)
                        .map(|s| (self.tt("stash", "stash"), s.ref_name.clone())),
                    // The email, not the display name: it is what
                    // `git log --author=` wants.
                    RepoTab::Contributors => data.contributors.get(item).map(|c| {
                        (
                            self.tt("author", "作者"),
                            if c.email.is_empty() {
                                c.name.clone()
                            } else {
                                c.email.clone()
                            },
                        )
                    }),
                    RepoTab::Worktrees => data
                        .worktrees
                        .get(item)
                        .map(|w| (self.tt("worktree", "worktree"), w.path.clone())),
                }
                .or_else(fallback)
            }
            _ => repo_path.map(|p| (self.tt("path", "パス"), p.display().to_string())),
        }
    }

    pub fn after_external_work(&mut self, path: &Path) {
        if self.screen == Screen::Workspace {
            self.reload_workspace();
        }
        let canonical = path.canonicalize().ok();
        let indices: Vec<_> = self
            .repos
            .iter()
            .enumerate()
            .filter_map(|(i, repo)| {
                let matches = repo.path == path
                    || canonical
                        .as_ref()
                        .is_some_and(|p| repo.path.canonicalize().ok().as_ref() == Some(p));
                matches.then_some(i)
            })
            .collect();
        for i in indices {
            self.refresh_home_row(i);
            if self.repo_index == Some(i) {
                self.reload_repo(i);
                if self.screen == Screen::Diff {
                    self.reload_diff_files();
                }
            }
        }
    }
}

#[cfg(test)]
mod tests;
