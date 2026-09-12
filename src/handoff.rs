//! Hand-off to external tools: what "where you are looking" means, and how a
//! command template is turned into an argument vector.
//!
//! The dashboard never edits a repository itself — it hands the work to a
//! shell, an editor or another git client. That hand-off is only useful if it
//! carries the context the user was looking at, so a template may refer to the
//! repository root, the file and line on screen, the branch and the commit.
//!
//! Every substituted value is **attacker-influenced**: a file name, a branch
//! name and a commit subject all come out of a repository the dashboard merely
//! registered, exactly like the text `git::strip_control_sequences` refuses to
//! trust. They are therefore validated (never quoted-and-hoped) before they can
//! reach `Command::args`, and a placeholder with no value in the current view
//! is an error rather than an empty string.

use std::path::{Path, PathBuf};

/// Placeholders a command template may use, in the order the menu lists them.
pub const PLACEHOLDERS: [&str; 5] = ["{path}", "{file}", "{line}", "{branch}", "{hash}"];

/// Upper bound on user-defined commands. The `O` menu is a popup on top of the
/// dashboard, not a scrolling list: past roughly this many entries it stops
/// being readable at a glance, which is the only reason the menu exists.
pub const MAX_CUSTOM_COMMANDS: usize = 6;

/// What the user is currently looking at, as far as an external tool cares.
///
/// `None` means "this view has no such thing" — not "empty string". The
/// distinction is the whole point: `code ""` opens the wrong thing silently,
/// so a template asking for a value this view cannot supply is refused.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ToolContext {
    /// Repository (or worktree) root. The child process also runs with this
    /// as its cwd.
    pub repo: PathBuf,
    /// Absolute path of the file on screen, if one is.
    pub file: Option<PathBuf>,
    /// 1-based line the diff view is showing, if it is showing a file.
    pub line: Option<usize>,
    pub branch: Option<String>,
    /// Commit (or other revision) being viewed.
    pub hash: Option<String>,
}

impl ToolContext {
    pub fn for_repo(repo: impl Into<PathBuf>) -> Self {
        Self {
            repo: repo.into(),
            ..Self::default()
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TemplateError {
    /// The template is empty, or is only whitespace/quotes.
    Empty,
    /// `shell_words` could not split it (an unbalanced quote).
    Split(String),
    /// A placeholder was used as the program name. Which binary runs must
    /// never be decided by repository-controlled text.
    ProgramPlaceholder,
    /// The placeholder has no value in the current view.
    Missing(&'static str),
    /// The value contains a control character (an escape sequence smuggled
    /// through a branch or file name).
    ControlChar(&'static str),
    /// The value starts an argument with `-`, so the child would read it as an
    /// option rather than as a path or a ref.
    OptionLike(&'static str),
}

impl TemplateError {
    /// The placeholder this error is about, for a message that can name it.
    pub fn placeholder(&self) -> Option<&'static str> {
        match self {
            Self::Missing(p) | Self::ControlChar(p) | Self::OptionLike(p) => Some(p),
            _ => None,
        }
    }
}

/// Split `template` and substitute the placeholders from `ctx`.
///
/// Returns the program name (unsubstituted, for `resolve_program` to vet) and
/// its arguments. Nothing is re-split after substitution, so a value
/// containing spaces or quotes stays exactly one argument.
pub fn expand(template: &str, ctx: &ToolContext) -> Result<(String, Vec<String>), TemplateError> {
    let mut parts = shell_words::split(template)
        .map_err(|e| TemplateError::Split(e.to_string()))?
        .into_iter();
    let program = parts
        .next()
        .filter(|s| !s.is_empty())
        .ok_or(TemplateError::Empty)?;
    if PLACEHOLDERS.iter().any(|p| program.contains(p)) {
        return Err(TemplateError::ProgramPlaceholder);
    }
    let mut args = Vec::new();
    for part in parts {
        args.push(substitute(&part, ctx)?);
    }
    // Historical fall-back: a template that names no placeholder at all (the
    // default `code`) is handed the repository root, because that is what the
    // user obviously meant. A template that does use placeholders gets exactly
    // what it asked for — appending a stray path to `jj log` would be nonsense.
    if !PLACEHOLDERS.iter().any(|p| template.contains(p)) {
        args.push(checked_value("{path}", &ctx.repo.to_string_lossy(), true)?);
    }
    Ok((program, args))
}

/// Substitute every known placeholder in one argument.
///
/// Unknown `{...}` text is left alone: a template may legitimately contain
/// braces (a format string for the tool being launched), and rewriting those
/// would be surprising.
fn substitute(token: &str, ctx: &ToolContext) -> Result<String, TemplateError> {
    let mut out = String::new();
    let mut rest = token;
    while let Some(open) = rest.find('{') {
        let Some(close) = rest[open..].find('}').map(|i| open + i) else {
            break;
        };
        let name = &rest[open..=close];
        let Some(value) = value_of(name, ctx) else {
            out.push_str(&rest[..=close]);
            rest = &rest[close + 1..];
            continue;
        };
        // Whether this value supplies the argument's *first* character.
        // `--goto` may start with a dash because the user wrote it; a branch
        // named `-rf` may not, because the child would read it as an option.
        let at_start = out.is_empty() && open == 0;
        out.push_str(&rest[..open]);
        out.push_str(&checked_value(placeholder_name(name), &value, !at_start)?);
        rest = &rest[close + 1..];
    }
    out.push_str(rest);
    Ok(out)
}

/// `&'static str` for a placeholder we recognise, so errors can name it
/// without allocating or borrowing the template.
fn placeholder_name(found: &str) -> &'static str {
    PLACEHOLDERS
        .iter()
        .copied()
        .find(|p| *p == found)
        .unwrap_or("{?}")
}

fn value_of(name: &str, ctx: &ToolContext) -> Option<String> {
    match name {
        "{path}" => Some(ctx.repo.to_string_lossy().into_owned()),
        "{file}" => Some(
            ctx.file
                .as_ref()
                .map(|f| f.to_string_lossy().into_owned())
                .unwrap_or_default(),
        ),
        "{line}" => Some(ctx.line.map(|l| l.to_string()).unwrap_or_default()),
        "{branch}" => Some(ctx.branch.clone().unwrap_or_default()),
        "{hash}" => Some(ctx.hash.clone().unwrap_or_default()),
        _ => None,
    }
}

/// Vet one substituted value. `dash_ok` is false when the value would begin
/// the argument, in which case a leading `-` is option injection.
fn checked_value(name: &'static str, value: &str, dash_ok: bool) -> Result<String, TemplateError> {
    if value.is_empty() {
        return Err(TemplateError::Missing(name));
    }
    if value.chars().any(|c| c.is_control()) {
        return Err(TemplateError::ControlChar(name));
    }
    if !dash_ok && value.starts_with('-') {
        return Err(TemplateError::OptionLike(name));
    }
    Ok(value.to_string())
}

/// Absolute path of a repository-relative file, for `{file}`.
///
/// Kept absolute on purpose: the child's cwd is the repository, but a GUI
/// editor that ignores cwd (or re-uses an existing window opened elsewhere)
/// would otherwise open the wrong file, or none.
pub fn absolute_file(repo: &Path, relative: &str) -> PathBuf {
    repo.join(relative)
}

/// Which text field of a custom command is being typed, if any.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum EditStage {
    #[default]
    List,
    Label,
    Command,
}

/// State of the custom-command editor popup (opened with `x` from Settings or
/// from the `O` menu). Self-contained rather than another `InputKind`, because
/// it is a list *and* a two-field form, which the single-line prompt is not.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ToolEditor {
    pub selected: usize,
    pub stage: EditStage,
    /// Text being typed in `Label`/`Command`.
    pub buf: String,
    /// Entry being replaced; `None` while a new one is being appended.
    pub target: Option<usize>,
    /// Label captured in the first step, kept while the command is typed.
    pub draft_label: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> ToolContext {
        ToolContext {
            repo: PathBuf::from("/w/repo"),
            file: Some(PathBuf::from("/w/repo/src/main.rs")),
            line: Some(120),
            branch: Some("feature/x".into()),
            hash: Some("abc1234".into()),
        }
    }

    #[test]
    fn every_placeholder_expands() {
        let (program, args) =
            expand("editor --goto {file}:{line} {path} {branch} {hash}", &ctx()).unwrap();
        assert_eq!(program, "editor");
        assert_eq!(
            args,
            vec![
                "--goto",
                "/w/repo/src/main.rs:120",
                "/w/repo",
                "feature/x",
                "abc1234"
            ]
        );
    }

    #[test]
    fn a_placeholder_without_a_value_is_refused_not_emptied() {
        let bare = ToolContext::for_repo("/w/repo");
        assert_eq!(
            expand("code {file}", &bare),
            Err(TemplateError::Missing("{file}"))
        );
        assert_eq!(
            expand("code --line {line}", &bare),
            Err(TemplateError::Missing("{line}"))
        );
        assert_eq!(
            expand("git log {branch}", &bare),
            Err(TemplateError::Missing("{branch}"))
        );
        assert_eq!(
            expand("git show {hash}", &bare),
            Err(TemplateError::Missing("{hash}"))
        );
        // {path} is always available, so the same template works.
        assert_eq!(
            expand("code {path}", &bare).unwrap().1,
            vec!["/w/repo".to_string()]
        );
    }

    #[test]
    fn a_value_may_not_turn_into_an_option() {
        let mut c = ctx();
        c.branch = Some("-rf".into());
        assert_eq!(
            expand("tool {branch}", &c),
            Err(TemplateError::OptionLike("{branch}"))
        );
        // A dash the *user* typed is fine, and so is a dashed value that is
        // not at the start of the argument.
        assert_eq!(
            expand("tool --branch={branch}", &c).unwrap().1,
            vec!["--branch=-rf".to_string()]
        );
    }

    #[test]
    fn control_characters_in_a_value_are_refused() {
        let mut c = ctx();
        c.branch = Some("main\u{1b}]52;c;aGk=\u{7}".into());
        assert_eq!(
            expand("tool {branch}", &c),
            Err(TemplateError::ControlChar("{branch}"))
        );
        c.file = Some(PathBuf::from("/w/repo/a\nb.rs"));
        c.branch = Some("main".into());
        assert_eq!(
            expand("tool {file}", &c),
            Err(TemplateError::ControlChar("{file}"))
        );
    }

    #[test]
    fn a_template_without_placeholders_still_gets_the_repository_path() {
        let (program, args) = expand("code --wait", &ctx()).unwrap();
        assert_eq!(program, "code");
        assert_eq!(args, vec!["--wait", "/w/repo"]);
        // ...and one that does use a placeholder does not get it twice.
        assert_eq!(
            expand("code {path}", &ctx()).unwrap().1,
            vec!["/w/repo".to_string()]
        );
    }

    #[test]
    fn quoting_survives_and_unknown_braces_are_literal() {
        let (_, args) = expand("tool --fmt '{n} of {total}' '{path}'", &ctx()).unwrap();
        assert_eq!(args, vec!["--fmt", "{n} of {total}", "/w/repo"]);
        assert_eq!(
            expand("'unclosed {path}", &ctx()),
            Err(TemplateError::Split(
                shell_words::split("'unclosed {path}")
                    .unwrap_err()
                    .to_string()
            ))
        );
    }

    #[test]
    fn the_program_itself_may_not_come_from_a_placeholder() {
        assert_eq!(
            expand("{branch} --version", &ctx()),
            Err(TemplateError::ProgramPlaceholder)
        );
        assert_eq!(expand("   ", &ctx()), Err(TemplateError::Empty));
    }

    #[test]
    fn a_value_with_spaces_stays_one_argument() {
        let mut c = ctx();
        c.file = Some(PathBuf::from("/w/repo/a b/c d.rs"));
        assert_eq!(
            expand("code {file}", &c).unwrap().1,
            vec!["/w/repo/a b/c d.rs".to_string()]
        );
    }
}
