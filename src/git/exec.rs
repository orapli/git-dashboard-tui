use std::path::Path;
use std::process::Command;

pub fn check_safe_ref(r: &str) -> Result<(), String> {
    if r.starts_with('-') {
        return Err(format!("Git ref cannot start with '-': {}", r));
    }
    // A ref carrying a newline would forge an extra line in any batched remote
    // script, and control characters cannot appear in a legitimate refname.
    if r.chars().any(|c| c.is_control()) {
        return Err("Git ref contains a control character".to_string());
    }
    Ok(())
}

pub fn check_safe_ref_opt(r: Option<&str>) -> Result<(), String> {
    if let Some(s) = r {
        check_safe_ref(s)?;
    }
    Ok(())
}

/// Create a Command that never flashes a console window on Windows.
/// GUI-subsystem apps otherwise spawn a visible console for every child
/// process, which makes the screen flicker on each git invocation.
pub fn quiet_command(program: &str) -> Command {
    #[allow(unused_mut)]
    let mut cmd = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

/// A destination is passed to `ssh` as a bare positional argument, and OpenSSH
/// has no `--` terminator: anything starting with `-` would be read as an
/// option, so `ssh://-oProxyCommand=.../x` would execute an arbitrary command.
/// Only the characters a real destination can contain are accepted.
fn is_safe_ssh_host(host: &str) -> bool {
    !host.is_empty()
        && !host.starts_with('-')
        && host.chars().all(|c| {
            c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | '@' | ':' | '[' | ']')
        })
}

pub fn parse_ssh_repo(repo_path: &Path) -> Option<(String, String)> {
    let s = repo_path.to_str()?;
    let rest = s.strip_prefix("ssh://")?;
    let (host, path) = rest.split_once('/')?;
    if path.is_empty() || !is_safe_ssh_host(host) {
        return None;
    }
    // The path is shell-quoted before it reaches the remote shell, but a
    // newline in it would still forge a line in the batch script below.
    if path.chars().any(|c| c.is_control()) {
        return None;
    }
    Some((host.to_string(), format!("/{path}")))
}

/// Quote a string for the remote POSIX shell (ssh joins argv with spaces and
/// hands the result to a shell, so pretty-format strings etc. must be quoted).
pub fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// Config overrides applied to every git invocation. Besides the display
/// settings, these neutralise the config keys that let a *repository* run code
/// on the machine analysing it: `.git/config` of any repo added to the
/// dashboard is honoured by git, and `safe.directory` only guards repos owned
/// by another user — a repo cloned into the user's own home is fully trusted.
/// `-c` beats the repository's own config, so an empty value disables the hook.
pub const GIT_CONFIG_ARGS: &[&str] = &[
    "color.ui=never",
    "core.quotepath=false",
    "diff.color=never",
    "log.showSignature=false",
    // Executable hooks reachable from read-only commands:
    "core.fsmonitor=",             // spawned by `git status`
    "core.sshCommand=ssh",         // spawned by any remote operation
    "uploadpack.packObjectsHook=", // spawned while serving a fetch
    "protocol.ext.allow=never",    // `ext::<command>` submodule/remote URLs
];

/// `diff.external` and the `textconv`/`diff` gitattributes drivers also run
/// repository-controlled commands, but unlike the keys above they cannot be
/// disabled with an empty `-c` value — git would try to execute the empty
/// string. They are suppressed with the per-subcommand flags instead, which is
/// why these have to be injected after the subcommand name rather than before.
fn diff_safety_flags(subcommand: &str) -> &'static [&'static str] {
    match subcommand {
        "diff" | "diff-tree" | "diff-index" | "diff-files" | "log" | "show" | "whatchanged" => {
            &["--no-ext-diff", "--no-textconv"]
        }
        "blame" => &["--no-textconv"],
        _ => &[],
    }
}

/// Insert the flags from [`diff_safety_flags`] right after the subcommand.
pub fn with_diff_safety_flags(args: &[&str]) -> Vec<String> {
    let Some((sub, rest)) = args.split_first() else {
        return Vec::new();
    };
    let mut out = Vec::with_capacity(args.len() + 2);
    out.push((*sub).to_string());
    out.extend(diff_safety_flags(sub).iter().map(|f| (*f).to_string()));
    out.extend(rest.iter().map(|a| (*a).to_string()));
    out
}

fn apply_git_config(cmd: &mut Command) {
    for c in GIT_CONFIG_ARGS {
        cmd.arg("-c").arg(c);
    }
}

/// Build the git invocation for a repository: plain `git` with current_dir for
/// local paths, or `ssh <host> git -C <path> ...` for `ssh://` locators.
pub fn git_command_for(repo_path: &Path, args: &[&str]) -> Command {
    if let Some((host, remote_path)) = parse_ssh_repo(repo_path) {
        let mut cmd = quiet_command("ssh");
        cmd.arg("-o")
            .arg("BatchMode=yes") // never prompt for passwords (key auth only)
            .arg("-o")
            .arg("ConnectTimeout=8")
            .arg(host)
            .arg("git");
        for c in GIT_CONFIG_ARGS {
            cmd.arg("-c").arg(shell_quote(c));
        }
        cmd.arg("--no-pager")
            .arg("-C")
            .arg(shell_quote(&remote_path));
        for a in with_diff_safety_flags(args) {
            cmd.arg(shell_quote(&a));
        }
        cmd
    } else {
        let mut cmd = quiet_command("git");
        apply_git_config(&mut cmd);
        cmd.arg("--no-pager")
            .args(with_diff_safety_flags(args))
            .current_dir(repo_path)
            .env("LC_ALL", "C")
            .env("LANG", "C")
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_OPTIONAL_LOCKS", "0")
            // Never let a credential helper pop an interactive dialog from a
            // worker thread (Git Credential Manager on Windows ignores
            // GIT_TERMINAL_PROMPT); failing fast beats hanging forever
            .env("GCM_INTERACTIVE", "never")
            .env("GIT_ASKPASS", "echo")
            .env("GIT_PAGER", "cat")
            .env("PAGER", "cat");
        cmd
    }
}

/// Hard timeout for analysis git invocations. ssh has ConnectTimeout, but an
/// established-yet-stalled session, a credential helper, or a hung network
/// filesystem would otherwise block a worker thread forever — a few of those
pub const GIT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);
pub const GIT_NETWORK_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(120);

/// Upper bound on how much of a command's output is retained. Git output is
/// attacker-influenced in size (a generated 4 GiB diff is a valid repository),
/// and the whole thing is buffered in memory before parsing.
pub const MAX_GIT_OUTPUT: usize = 64 * 1024 * 1024;

/// How long to wait for a drain thread's buffer once the child has exited.
/// Normally instantaneous; the bound only matters when a *grandchild* inherited
/// the pipe and keeps its write end open (see `run_with_timeout`).
const DRAIN_GRACE: std::time::Duration = std::time::Duration::from_secs(3);

/// Read up to `cap` bytes, then discard the rest so the writer never blocks on
/// a full pipe.
fn drain_capped(mut r: impl std::io::Read, cap: usize) -> Vec<u8> {
    use std::io::Read;
    let mut buf = Vec::new();
    let _ = r.by_ref().take(cap as u64).read_to_end(&mut buf);
    if buf.len() >= cap {
        let _ = std::io::copy(&mut r, &mut std::io::sink());
    }
    buf
}

/// Run a command with a hard timeout, killing the process on expiry.
/// stdout/stderr are drained on dedicated threads — waiting for exit before
/// reading would deadlock once a pipe buffer (64 KiB) fills on large output.
///
/// Those threads are **never joined unboundedly**: `read_to_end` returns only
/// once every holder of the pipe's write end closes it, and killing the child
/// does not close the copies a grandchild inherited (`git pull` →
/// `git-remote-https`, an fsmonitor daemon, …). Joining would therefore make
/// the "hard" timeout unbounded and wedge the worker thread forever, so the
/// buffers are collected over a channel with a grace period and the threads are
/// left detached to exit on their own.
pub fn run_with_timeout(
    mut cmd: Command,
    timeout: std::time::Duration,
) -> Result<std::process::Output, String> {
    use std::process::Stdio;
    use std::sync::mpsc;

    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = cmd
        .spawn()
        .map_err(|e| format!("コマンドの起動に失敗しました: {e}"))?;

    let stdout_pipe = child.stdout.take();
    let stderr_pipe = child.stderr.take();
    let (out_tx, out_rx) = mpsc::channel();
    let (err_tx, err_rx) = mpsc::channel();
    std::thread::spawn(move || {
        let buf = stdout_pipe.map_or_else(Vec::new, |s| drain_capped(s, MAX_GIT_OUTPUT));
        let _ = out_tx.send(buf);
    });
    std::thread::spawn(move || {
        let buf = stderr_pipe.map_or_else(Vec::new, |s| drain_capped(s, MAX_GIT_OUTPUT));
        let _ = err_tx.send(buf);
    });

    // The first `try_wait` after spawn practically always finds the child still
    // running, so whatever the nap is, it becomes a *floor* under every single
    // git invocation. A flat 25 ms one therefore cost ~25 ms per call against
    // ~0.5 ms of real git work, and at ~17 invocations per repository per
    // refresh that sleeping dominated the dashboard's refresh time. Starting
    // short collects the fast commands — the overwhelming majority — almost
    // immediately, and doubling up to a small cap keeps a genuinely
    // long-running command from spinning the poll loop.
    const POLL_MIN: std::time::Duration = std::time::Duration::from_micros(200);
    const POLL_MAX: std::time::Duration = std::time::Duration::from_millis(2);

    let start = std::time::Instant::now();
    let mut poll = POLL_MIN;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {
                if start.elapsed() > timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(format!(
                        "コマンドが{}秒でタイムアウトしました（リモートまたはファイルシステムが応答していません）",
                        timeout.as_secs()
                    ));
                }
                std::thread::sleep(poll);
                poll = (poll * 2).min(POLL_MAX);
            }
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("child process wait failed: {e}"));
            }
        }
    };

    let stdout = out_rx.recv_timeout(DRAIN_GRACE).unwrap_or_default();
    let stderr = err_rx.recv_timeout(DRAIN_GRACE).unwrap_or_default();
    Ok(std::process::Output {
        status,
        stdout,
        stderr,
    })
}

/// Strip terminal control sequences from anything git hands back.
///
/// A registered repository is not trusted — that is the whole reason this
/// module neutralises `core.fsmonitor` and friends — and its *content* reaches
/// the screen too. File contents, commit messages, branch names and author
/// names are all attacker-chosen text, and ratatui writes strings to the
/// terminal as given. A line containing `ESC[2J` therefore clears the screen;
/// `ESC]52;c;...BEL` writes the system clipboard on terminals that support it;
/// SGR sequences repaint arbitrary regions. None of that should be reachable
/// by committing a file.
///
/// `\t` and `\n` survive: porcelain formats use tabs as field separators
/// (`git diff --name-status`, `git ls-tree --long`), so removing them here
/// would break parsing. Tabs are expanded where *content* is displayed
/// instead — see `expand_tabs`.
///
/// Every other C0 control, DEL, and the C1 range are replaced by a visible
/// placeholder rather than dropped: a line that silently loses characters
/// misrepresents the file, whereas a dot says "something unprintable is here".
pub fn strip_control_sequences(input: &str, keep_sgr: bool) -> String {
    const PLACEHOLDER: char = '·';
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\t' | '\n' => out.push(c),
            '\u{1b}' => match chars.peek() {
                // CSI: parameters, then a final byte in @..~.
                Some('[') => {
                    chars.next();
                    let mut seq = String::from("\u{1b}[");
                    for x in chars.by_ref() {
                        seq.push(x);
                        if ('\u{40}'..='\u{7e}').contains(&x) {
                            break;
                        }
                    }
                    // Only colour/attribute sequences may be kept, and only
                    // where the caller asked git for them (`--color=always`,
                    // whose output the graph renderer parses itself).
                    if keep_sgr && seq.ends_with('m') {
                        out.push_str(&seq);
                    }
                }
                // OSC: runs until BEL or ST, and carries the payload that
                // makes clipboard and hyperlink injection possible. Consume
                // the terminator too, or the payload leaks out as text.
                Some(']') => {
                    chars.next();
                    while let Some(x) = chars.next() {
                        if x == '\u{7}' {
                            break;
                        }
                        if x == '\u{1b}' && chars.peek() == Some(&'\\') {
                            chars.next();
                            break;
                        }
                    }
                }
                // Two-character escapes (ESC c resets the terminal), and a
                // trailing lone ESC.
                Some(_) => {
                    chars.next();
                }
                None => {}
            },
            c if (c as u32) < 0x20 || c == '\u{7f}' => out.push(PLACEHOLDER),
            c if ('\u{80}'..='\u{9f}').contains(&c) => out.push(PLACEHOLDER),
            c => out.push(c),
        }
    }
    out
}

/// Expand tabs to the next `width` column, for text shown as file content.
///
/// ratatui writes a tab to the terminal verbatim, and the terminal moves the
/// cursor to its own next tab stop — which the layout knows nothing about, so
/// a tab-indented file (Go, Make, C) drew over the pane beside it.
pub fn expand_tabs(line: &str, width: usize) -> String {
    if !line.contains('\t') {
        return line.to_string();
    }
    let mut out = String::with_capacity(line.len() + width);
    let mut col = 0usize;
    for c in line.chars() {
        if c == '\t' {
            let n = width - (col % width);
            out.extend(std::iter::repeat_n(' ', n));
            col += n;
        } else {
            out.push(c);
            col += 1;
        }
    }
    out
}

/// Columns per tab when showing file content. Four rather than eight because
/// the diff panes are narrow and every level of indentation costs width.
pub const TAB_WIDTH: usize = 4;

// Helper to run git commands
pub fn run_git_cmd(repo_path: &Path, args: &[&str]) -> Result<String, String> {
    let output = run_with_timeout(git_command_for(repo_path, args), GIT_TIMEOUT)?;

    if !output.status.success() {
        return Err(strip_control_sequences(
            &String::from_utf8_lossy(&output.stderr),
            false,
        ));
    }

    Ok(strip_control_sequences(
        &String::from_utf8_lossy(&output.stdout),
        false,
    ))
}

/// As [`run_git_cmd`], but keeps the SGR colour sequences produced by
/// `--color=always`. Used only by the commit-graph commands, whose output the
/// renderer parses for git's own per-lane colours; every other escape is still
/// removed.
pub fn run_git_cmd_ansi(repo_path: &Path, args: &[&str]) -> Result<String, String> {
    let output = run_with_timeout(git_command_for(repo_path, args), GIT_TIMEOUT)?;

    if !output.status.success() {
        return Err(strip_control_sequences(
            &String::from_utf8_lossy(&output.stderr),
            false,
        ));
    }

    Ok(strip_control_sequences(
        &String::from_utf8_lossy(&output.stdout),
        true,
    ))
}

/// Build the sentinel delimiting per-command output when several git commands
/// share one SSH connection. It must be **unpredictable**: batched commands
/// print repository-controlled text (author names, file names), so a fixed
/// literal could be forged by a commit author named after it, shifting every
/// subsequent result by one and silently attributing one command's output to
/// another. A per-call nonce makes the sentinel unguessable by repo content.
fn batch_sentinel() -> String {
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hasher};
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let mut h = RandomState::new().build_hasher();
    h.write_u64(n);
    format!("___GITDASH_END_{:016x}_{:x}___", h.finish(), n)
}

/// Run several git commands against a repository, returning one result per
/// command in order. For **SSH** repositories all commands share a single ssh
/// connection: each `ssh` invocation is a full TCP + crypto handshake, and one
/// repository analysis otherwise opens a dozen of them (the dominant cost on
/// remote repos, and the only multiplexing option that works on Windows where
/// OpenSSH ControlMaster is unsupported). Local repositories have no handshake
/// cost, so they simply run sequentially.
///
/// A per-command non-zero exit maps to `Err(output)`, mirroring `run_git_cmd`.
pub fn run_git_batch(repo_path: &Path, commands: &[&[&str]]) -> Vec<Result<String, String>> {
    let Some((host, remote_path)) = parse_ssh_repo(repo_path) else {
        // Local: no connection to amortize; behave exactly like N run_git_cmd
        return commands.iter().map(|c| run_git_cmd(repo_path, c)).collect();
    };

    // Remote shell script: each command's merged stdout+stderr, then a sentinel
    // line carrying its exit code. printf's `\n` guarantees the sentinel starts
    // on its own line even when a command's output has no trailing newline.
    let qpath = shell_quote(&remote_path);
    let sep = batch_sentinel();
    let mut script = String::new();
    for cmd in commands {
        script.push_str("LC_ALL=C git");
        for c in GIT_CONFIG_ARGS {
            script.push_str(" -c ");
            script.push_str(&shell_quote(c));
        }
        script.push_str(" --no-pager -C ");
        script.push_str(&qpath);
        for a in with_diff_safety_flags(cmd) {
            script.push(' ');
            script.push_str(&shell_quote(&a));
        }
        script.push_str(" 2>&1; printf '\\n%s%d\\n' '");
        script.push_str(&sep);
        script.push_str("' \"$?\"\n");
    }

    let mut ssh = quiet_command("ssh");
    ssh.arg("-o")
        .arg("BatchMode=yes")
        .arg("-o")
        .arg("ConnectTimeout=8")
        .arg(&host)
        .arg(&script);

    let timeout = (GIT_TIMEOUT * commands.len().max(1) as u32).min(GIT_NETWORK_TIMEOUT);
    match run_with_timeout(ssh, timeout) {
        Ok(out) => {
            // Same reasoning as `run_git_cmd`: this output is repository
            // content, and it reaches the screen.
            let stdout = strip_control_sequences(&String::from_utf8_lossy(&out.stdout), false);
            let stderr = strip_control_sequences(&String::from_utf8_lossy(&out.stderr), false);
            split_batch_output(&stdout, &sep, commands.len(), stderr.trim())
        }
        Err(e) => commands.iter().map(|_| Err(e.clone())).collect(),
    }
}

/// Split sentinel-delimited batch stdout into per-command results. Each command
/// emitted its output followed by a line `<sep><exit-code>`. Segments missing
/// their sentinel (e.g. the connection dropped mid-stream) become `Err`.
pub fn split_batch_output(
    stdout: &str,
    sep: &str,
    expected: usize,
    conn_err: &str,
) -> Vec<Result<String, String>> {
    let mut results = Vec::with_capacity(expected);
    let mut buf = String::new();
    for line in stdout.lines() {
        if let Some(code_str) = line.strip_prefix(sep) {
            let code: i32 = code_str.trim().parse().unwrap_or(-1);
            // Drop the trailing newline(s) printf/line-joining introduced;
            // callers trim or split, so exact trailing whitespace is moot.
            let content = buf.trim_end_matches('\n').to_string();
            buf.clear();
            if code == 0 {
                results.push(Ok(content));
            } else {
                results.push(Err(content));
            }
        } else {
            buf.push_str(line);
            buf.push('\n');
        }
    }
    // More boundaries than commands means the stream is not what we sent —
    // truncating would silently pair each command with another one's output,
    // so the whole batch is reported as failed instead.
    if results.len() > expected {
        return (0..expected)
            .map(|_| Err("バッチ出力の区切りが一致しません".to_string()))
            .collect();
    }
    while results.len() < expected {
        results.push(Err(if conn_err.is_empty() {
            "SSH接続に失敗しました".to_string()
        } else {
            conn_err.to_string()
        }));
    }
    results
}

#[cfg(test)]
mod wait_tests {
    use super::*;

    /// Waiting for the child used to be a flat 25 ms sleep between `try_wait`
    /// polls, which put a ~25 ms floor under every git invocation — 10 calls
    /// could not finish in under 250 ms no matter how trivial the commands.
    /// The bound here is 100 ms: 10 ms per invocation is over an order of
    /// magnitude more than a `true` costs in practice (~1 ms, spawn included),
    /// so a loaded CI machine has ample slack, while still being 2.5× below
    /// the old floor — the flat-sleep version cannot pass this.
    #[test]
    #[cfg(unix)]
    fn short_commands_are_not_charged_a_fixed_poll_interval() {
        const RUNS: u32 = 10;
        let start = std::time::Instant::now();
        for _ in 0..RUNS {
            let out = run_with_timeout(Command::new("true"), std::time::Duration::from_secs(10))
                .expect("`true` should run");
            assert!(out.status.success());
        }
        let elapsed = start.elapsed();
        assert!(
            elapsed < std::time::Duration::from_millis(100),
            "{RUNS} trivial commands took {elapsed:?}; the wait loop is charging a fixed interval again"
        );
    }
}

#[cfg(test)]
mod sanitize_tests {
    use super::*;

    /// The reason this exists: repository content reaches the terminal, and a
    /// terminal executes what it is sent. `ESC[2J` clears the screen.
    #[test]
    fn a_file_cannot_clear_the_screen() {
        let hostile = "before\u{1b}[2Jafter";
        assert_eq!(strip_control_sequences(hostile, false), "beforeafter");
    }

    /// OSC carries a payload — `ESC]52;c;<base64>BEL` writes the system
    /// clipboard on terminals that support it. Dropping only the introducer
    /// would leave the payload on screen as text; the terminator has to go too.
    #[test]
    fn osc_sequences_are_consumed_payload_and_all() {
        assert_eq!(
            strip_control_sequences("a\u{1b}]52;c;cGF5bG9hZA==\u{7}b", false),
            "ab"
        );
        // String Terminator form.
        assert_eq!(
            strip_control_sequences("a\u{1b}]0;title\u{1b}\\b", false),
            "ab"
        );
    }

    #[test]
    fn colour_sequences_are_dropped_unless_the_caller_asked_git_for_them() {
        let coloured = "\u{1b}[31mred\u{1b}[0m";
        assert_eq!(strip_control_sequences(coloured, false), "red");
        assert_eq!(strip_control_sequences(coloured, true), coloured);
    }

    /// Even when SGR is kept for the commit graph, nothing else may pass.
    #[test]
    fn keeping_colour_still_blocks_cursor_and_erase_sequences() {
        let mixed = "\u{1b}[32m*\u{1b}[0m\u{1b}[2J\u{1b}[10;10Hx\u{1b}]52;c;p\u{7}";
        let out = strip_control_sequences(mixed, true);
        assert_eq!(out, "\u{1b}[32m*\u{1b}[0mx");
    }

    /// Porcelain formats separate fields with tabs (`git diff --name-status`,
    /// `git ls-tree --long`), so stripping them here would break parsing.
    /// They are expanded where content is displayed instead.
    #[test]
    fn tabs_and_newlines_survive_for_the_parsers() {
        assert_eq!(
            strip_control_sequences("M\tsrc/a.rs\nA\tsrc/b.rs\n", false),
            "M\tsrc/a.rs\nA\tsrc/b.rs\n"
        );
    }

    /// A dropped character would misrepresent the file; a visible mark says
    /// something unprintable is there.
    #[test]
    fn other_control_characters_become_a_visible_mark() {
        assert_eq!(
            strip_control_sequences("a\u{0}b\u{7}c\u{7f}d", false),
            "a·b·c·d"
        );
        // Lone CR would rewind the cursor over what was already drawn.
        assert_eq!(strip_control_sequences("a\rb", false), "a·b");
    }

    #[test]
    fn c1_controls_are_marked_too() {
        assert_eq!(strip_control_sequences("a\u{9b}b\u{85}c", false), "a·b·c");
    }

    #[test]
    fn a_truncated_escape_at_end_of_input_does_not_leak() {
        assert_eq!(strip_control_sequences("text\u{1b}", false), "text");
        assert_eq!(strip_control_sequences("text\u{1b}[", false), "text");
        assert_eq!(strip_control_sequences("text\u{1b}[31", false), "text");
    }

    #[test]
    fn ordinary_text_is_untouched() {
        for s in ["", "plain", "日本語のテキスト", "emoji 🎉 ok", "a\nb\n"] {
            assert_eq!(strip_control_sequences(s, false), s);
        }
    }

    #[test]
    fn tabs_expand_to_the_next_stop_not_a_fixed_run() {
        assert_eq!(expand_tabs("\tx", 4), "    x");
        assert_eq!(expand_tabs("a\tx", 4), "a   x");
        assert_eq!(expand_tabs("abc\tx", 4), "abc x");
        assert_eq!(expand_tabs("abcd\tx", 4), "abcd    x");
        assert_eq!(expand_tabs("\t\tx", 4), "        x");
    }

    #[test]
    fn expanding_leaves_tab_free_lines_alone() {
        assert_eq!(expand_tabs("no tabs here", 4), "no tabs here");
        assert_eq!(expand_tabs("", 4), "");
    }
}
