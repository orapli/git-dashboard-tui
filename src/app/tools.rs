use super::*;
use std::path::Path;

pub fn editor_command(template: &str, path: &Path) -> Result<ExternalDiff, String> {
    let mut parts = shell_words::split(template)
        .map_err(|e| e.to_string())?
        .into_iter();
    let program = parts
        .next()
        .filter(|s| !s.is_empty())
        .ok_or("Editor command is empty")?;
    let mut args: Vec<String> = parts
        .map(|s| s.replace("{path}", &path.to_string_lossy()))
        .collect();
    if !template.contains("{path}") {
        args.push(path.to_string_lossy().into_owned());
    }
    Ok(ExternalDiff {
        program,
        args,
        cwd: path.to_path_buf(),
    })
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
        let editor = editor_command(&self.prefs.editor_command, path)
            .map(|c| availability(&c.program))
            .unwrap_or_else(|_| self.tt(" [configure with c]", " [cで設定]"));
        vec![
            path.display().to_string(),
            self.tt("t  Shell", "t  シェル"),
            format!(
                "e  {}: {}{editor}",
                self.tt("Editor", "エディタ"),
                self.prefs.editor_command
            ),
            format!("l  lazygit{}", availability("lazygit")),
            format!("g  GitUI{}", availability("gitui")),
            self.tt("c  Configure editor command", "c  エディタコマンドを設定"),
            format!(
                "w  {}: {}",
                self.tt("Wait for editor", "エディタ終了待機"),
                if self.prefs.editor_wait {
                    self.tt("on (terminal editor)", "オン（端末内）")
                } else {
                    self.tt("off (GUI)", "オフ（GUI）")
                }
            ),
            self.tt(
                "Install clients on PATH. Esc: close",
                "クライアントをPATH上に導入してください。Esc: 閉じる",
            ),
        ]
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
            KeyCode::Char('t') => {
                self.tool_menu = None;
                self.queue_shell(path);
            }
            KeyCode::Char('e') | KeyCode::Char('l') | KeyCode::Char('g') => {
                if git::parse_ssh_repo(&path).is_some() || !path.is_dir() {
                    self.error = Some(self.tt(
                        "Local tools require an existing local directory",
                        "ローカルツールには存在するローカルディレクトリが必要です",
                    ));
                    return;
                }
                let command = match key.code {
                    KeyCode::Char('e') => editor_command(&self.prefs.editor_command, &path),
                    code => Ok(ExternalDiff {
                        program: if code == KeyCode::Char('l') {
                            "lazygit"
                        } else {
                            "gitui"
                        }
                        .into(),
                        args: vec![],
                        cwd: path,
                    }),
                };
                match command.and_then(|mut c| {
                    c.program = crate::resolve_program(&c.program)?
                        .to_string_lossy()
                        .into_owned();
                    Ok(c)
                }) {
                    Ok(command) => {
                        self.pending_work_tool = Some(WorkTool {
                            command,
                            wait: key.code != KeyCode::Char('e') || self.prefs.editor_wait,
                        });
                        self.tool_menu = None;
                    }
                    Err(e) => self.error = Some(e),
                }
            }
            _ => {}
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
mod tests {
    use super::*;
    #[test]
    fn editor_preserves_spaces_and_passes_paths_as_arguments() {
        let path = Path::new("/tmp/repo with spaces");
        let c = editor_command("code --wait --goto '{path}'", path).unwrap();
        assert_eq!(c.args, vec!["--wait", "--goto", "/tmp/repo with spaces"]);
        let c = editor_command("'/opt/editor tools/editor' --wait", path).unwrap();
        assert_eq!(c.program, "/opt/editor tools/editor");
        assert_eq!(c.args, vec!["--wait", "/tmp/repo with spaces"]);
        assert!(editor_command("'unclosed", path).is_err());
        assert!(crate::resolve_program("./repo-tool").is_err());
    }

    #[test]
    fn tools_menu_closes_without_changing_the_screen_and_rejects_ssh() {
        let mut a = App::new();
        a.open_tool_menu("ssh://host/work/repo".into());
        a.handle_key(KeyEvent::from(KeyCode::Char('l')));
        assert!(a.take_work_tool().is_none());
        assert!(a.error.is_some());
        a.handle_key(KeyEvent::from(KeyCode::Esc));
        assert!(a.tool_menu.is_none());
        assert_eq!(a.screen, Screen::Home);
    }
}
