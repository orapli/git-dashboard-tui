# Security Policy

## Reporting a vulnerability

Please report security issues privately using
[GitHub Security Advisories](https://github.com/orapli/git-dashboard-tui/security/advisories/new)
rather than a public issue. Include the version (or commit), a minimal reproduction, and the
impact you believe it has. We'll acknowledge the report and work with you on a fix and
disclosure timeline.

## Threat model

git-dashboard-tui runs `git` against repositories you register — which may include
repositories you cloned but did not write. **A repository's own content and configuration are
treated as untrusted input.** The interesting attack surface is not the dashboard's UI; it's
that merely *displaying* a repository must not let that repository run code on your machine.

### What the tool defends against

- **Code execution via a hostile `.git/config`.** Git honours a repository's local config, and
  several keys name a command to run. Every invocation therefore overrides them:
  `core.fsmonitor=` (would run on `git status`), `core.sshCommand=ssh`, `uploadpack.packObjectsHook=`,
  and `protocol.ext.allow=never` (blocks `ext::<command>` URLs). Diff and log subcommands
  additionally get `--no-ext-diff --no-textconv`, so a `diff.external` setting or a `textconv`
  gitattributes driver can't fire either.

  Note that git's own `safe.directory` protection does **not** cover this: it only guards
  repositories owned by a *different* user. A repository you cloned into your own home
  directory is fully trusted by git, which is exactly the case this tool has to handle.

- **Argument and option injection.** Refs taken from git's own output are passed back to git,
  so they're rejected if they begin with `-` or contain control characters, and ref/path
  arguments are terminated with `--`. For `ssh://` locators, the destination is validated
  against a character allowlist and rejected if it could be read as a flag: OpenSSH has no
  `--` option terminator, so `ssh://-oProxyCommand=…/x` would otherwise be arbitrary command
  execution.

- **Output forgery over SSH.** When several git commands share one SSH connection, their
  outputs are separated by a delimiter. Because that output contains repository-controlled
  text (author names, file names), the delimiter is an unpredictable per-call nonce rather
  than a fixed string, and a mismatch between the expected and observed boundary count fails
  the whole batch instead of silently pairing each command with another's output.

- **Denial of service through unbounded work.** Every command runs under a hard timeout
  (30s for analysis, 120s for network operations). Output is capped at 64 MiB per stream and
  drained on dedicated threads so a full pipe can't deadlock the reader. Blame results, diff
  rows, contributor log walks, and file reads are all bounded, so a repository with a
  pathologically large file or history can't exhaust memory.

- **Credential prompts hanging a background thread.** `GIT_TERMINAL_PROMPT=0`,
  `GIT_ASKPASS=echo`, `GCM_INTERACTIVE=never`, and `BatchMode=yes` for SSH mean a repository
  needing credentials fails fast instead of blocking forever on a prompt no one can see.

### What is explicitly out of scope

- **Repository *hooks* are not disabled.** This tool does not run any command that executes
  hooks during read-only analysis, but `pull` and `fetch` — which it does offer — can. If you
  run `pull`/`fetch` against a repository, you are trusting that repository's hooks exactly as
  much as you would running the same command in your own shell.

- **Your global and system git configuration is trusted.** Only the *repository's* dangerous
  config keys are overridden. Settings in `~/.gitconfig` are yours, and are left alone.

- **The `t` shell jump runs your shell.** That's the feature. It starts `$SHELL` with its
  working directory set to the selected repository; anything your shell profile does on
  startup happens as usual.

- **An external diff tool you configure is trusted.** Setting one (`c` in Settings) means that
  program runs with its working directory set to the repository being viewed. Relative program
  paths are rejected for that reason — otherwise `./tool` would execute a binary shipped *by
  the repository you're inspecting* — but an absolute path or a `PATH` lookup is honoured as
  given.

- **Configuration files are trusted.** `config.json`, `members.json`, and `prefs.json` are
  treated as yours. They are shared with the `git-dashboard` GUI application, so if you sync
  them between machines, treat them with the same care as any other dotfile.

## Data handling

- **Everything runs locally.** There is no telemetry, no phone-home, and no external service.
  The only network traffic is git talking to remotes you configured, plus the `gh` CLI if you
  have it installed (used for PR counts and CI status).
- **Nothing is transmitted anywhere by the dashboard itself.** The analysis cache under the
  config directory stays on your machine. It contains repository metadata — branch names,
  commit subjects, author names and email addresses — so treat that directory the same way you
  would treat the repositories it describes.
