use super::*;
use crate::git::{GitOpState, GithubState, LastFetch, UpstreamState};

#[derive(Default, Debug, PartialEq, Eq)]
pub struct HomeCounts {
    pub attention: usize,
    /// Repositories with at least one *tracked* file changed. Untracked
    /// files are counted separately: five stray build artefacts are not the
    /// same news as five edited source files.
    pub dirty: usize,
    pub untracked: usize,
    pub sync: usize,
    pub unknown: usize,
    /// Repositories whose row could not be read at all.
    pub failed: usize,
}

impl App {
    pub fn home_counts(&self) -> HomeCounts {
        let mut counts = HomeCounts::default();
        for i in filter_repo_indices(&self.repos, &self.home_filter, self.group_filter.as_deref()) {
            if let Some(row) = self.home_rows.get(&i) {
                counts.attention += usize::from(needs_attention(row));
                if row.error.is_some() {
                    // A row that failed to load has no numbers to contribute.
                    // Letting its zeros fall through would count it as clean
                    // and in sync, which is the very lie this row reports.
                    counts.failed += 1;
                    continue;
                }
                let (tracked, untracked) = dirty_split(row).unwrap_or((row.dirty, 0));
                counts.dirty += usize::from(tracked > 0);
                counts.untracked += usize::from(untracked > 0);
                counts.sync += usize::from(row.ahead > 0 || row.behind > 0);
                let now = chrono::Utc::now().timestamp();
                let unverified = row.github.as_ref().map_or(
                    row.ci_status.is_some() || row.open_prs.is_some(),
                    |info| {
                        !info.ci_state.is_settled()
                            || info.pr_state != GithubState::Ready
                            || info.ci_fetched_at == 0
                            || info.pr_fetched_at == 0
                            || now - info.ci_fetched_at >= git::github::REFRESH_SECS
                            || now - info.pr_fetched_at >= git::github::REFRESH_SECS
                    },
                );
                counts.unknown += usize::from(unverified);
            } else {
                counts.unknown += 1;
            }
        }
        counts
    }

    pub fn home_context(&self) -> Vec<String> {
        let Some(&i) = self.filtered_home().get(self.home_selected) else {
            return vec![];
        };
        // The Home table only has room for a shortened path, so the panel is
        // where the full one lives.
        let path_line = match self.repos.get(i) {
            Some(repo) => format!("{}: {}", self.tt("Path", "パス"), repo.path.display()),
            None => String::new(),
        };
        let Some(row) = self.home_rows.get(&i) else {
            return vec![
                self.tt(
                    "Local status unavailable / loading — r: reload",
                    "ローカル状態は未取得・取得中 — r: 再読み込み",
                ),
                path_line,
            ];
        };
        // A failed row's other fields are all defaults, so none of the usual
        // lines below would say anything true about it.
        if let Some(err) = &row.error {
            return vec![
                format!(
                    "⚠ {}: {err}",
                    self.tt(
                        "cannot read this repository",
                        "このリポジトリを読み取れません"
                    )
                ),
                path_line,
                format!(
                    "r: {}  s: {}",
                    self.tt("retry", "再試行"),
                    self.tt(
                        "settings — fix or remove this registration",
                        "設定 — 登録の修正・削除"
                    )
                ),
            ];
        }
        let mut reasons = Vec::new();
        if row.conflicts > 0 {
            reasons.push(format!(
                "{} {}",
                row.conflicts,
                self.tt("unresolved conflicts", "件の未解決コンフリクト")
            ));
        }
        if row.op_state != GitOpState::None {
            reasons.push(format!(
                "{} {}",
                row.op_state.label(),
                self.tt("in progress", "中")
            ));
        }
        if ci_failed_on_branch(row) {
            reasons.push(self.tt("CI failure on this branch", "このブランチのCI失敗"));
        }
        if branch_pr_changes_requested(row) {
            reasons.push(self.tt(
                "changes requested on this branch's PR",
                "このブランチのPRに変更要求",
            ));
        }
        let reviews = review_requests(row);
        if reviews > 0 {
            reasons.push(format!(
                "{reviews} {}",
                self.tt("PRs awaiting your review", "件のレビュー待ちPR")
            ));
        }
        if reasons.is_empty() {
            reasons.push(self.tt("No known attention reason", "取得済み情報に要対応なし"));
        }
        let local = if row.fetched_at == 0 {
            self.tt("unknown", "不明")
        } else {
            git::format_timestamp(row.fetched_at)
        };
        // Two different ages, deliberately side by side so neither can be
        // mistaken for the other: when the dashboard last read the working
        // tree, and when git last talked to the remote. The second is the age
        // of the ahead/behind counts, which reloading here cannot refresh —
        // only a fetch can.
        let remote = match row.last_fetch {
            LastFetch::At(ts) => git::format_timestamp(ts),
            LastFetch::Never => self.tt("never fetched", "フェッチ履歴なし"),
            LastFetch::Unavailable => self.tt("unavailable (remote repo)", "取得不可（リモート）"),
            LastFetch::Unknown => self.tt("not recorded", "未記録"),
        };
        let upstream_note = match row.upstream {
            UpstreamState::NoRemote => {
                format!(" [{}]", self.tt("no remote", "リモートなし"))
            }
            UpstreamState::NoUpstream => {
                format!(" [{}]", self.tt("no upstream branch", "上流ブランチなし"))
            }
            UpstreamState::Tracking | UpstreamState::Unknown => String::new(),
        };
        let mut lines = vec![
            reasons.join(" / "),
            path_line,
            format!(
                "{}: {local}  |  {}: {remote}{upstream_note}",
                self.tt("Local read (working tree)", "ローカル読取（作業ツリー）"),
                self.tt("Remote fetched (↑↓)", "リモート取得（↑↓）")
            ),
            format!(
                "Enter: {}  t: {}  C: CI  r: {}  F: git fetch",
                self.tt("details / diff", "詳細・差分"),
                self.tt("shell", "シェル"),
                self.tt("reload local", "ローカル再読込")
            ),
        ];
        if let Some(info) = &row.github {
            let state = |s| {
                self.tt(
                    match s {
                        GithubState::Ready => "ready",
                        GithubState::NoRuns => "no runs",
                        GithubState::Unknown => "not fetched",
                        GithubState::Unauthenticated => "unauthenticated (gh auth login)",
                        GithubState::Failed => "fetch failed",
                        GithubState::Unavailable => "gh not installed",
                        GithubState::Unsupported => "SSH unsupported",
                        GithubState::Detached => "no branch (detached HEAD)",
                        GithubState::TimedOut => "timed out (slow or unreachable)",
                    },
                    match s {
                        GithubState::Ready => "取得済み",
                        GithubState::NoRuns => "実行なし",
                        GithubState::Unknown => "未取得",
                        GithubState::Unauthenticated => "未認証 (gh auth login)",
                        GithubState::Failed => "取得失敗",
                        GithubState::Unavailable => "gh未導入",
                        GithubState::Unsupported => "SSH非対応",
                        GithubState::Detached => "ブランチなし（detached HEAD）",
                        GithubState::TimedOut => "タイムアウト（低速・未到達）",
                    },
                )
            };
            let stale = info.ci_fetched_at > 0
                && (chrono::Utc::now().timestamp() - info.ci_fetched_at
                    >= git::github::REFRESH_SECS
                    || !info.ci_state.is_settled());
            let time = if info.ci_fetched_at > 0 {
                git::format_timestamp(info.ci_fetched_at)
            } else {
                "—".into()
            };
            // The run list is queried scoped to this branch, so the branch in
            // brackets is what the state is about — not whichever branch
            // happened to push to the repository most recently.
            lines.push(format!(
                "CI {} [{}]: {} / {} {}{}",
                self.tt("latest on branch", "ブランチ最新"),
                info.ci_branch.as_deref().unwrap_or("—"),
                state(info.ci_state),
                self.tt("fetched", "取得"),
                time,
                if stale {
                    self.tt(" (stale cache)", "（古いキャッシュ）")
                } else {
                    String::new()
                }
            ));
            let count = info
                .open_prs
                .map(|n| {
                    if n >= 100 {
                        "100+".into()
                    } else {
                        n.to_string()
                    }
                })
                .unwrap_or_else(|| "—".into());
            let stale_pr = info.pr_fetched_at > 0
                && (chrono::Utc::now().timestamp() - info.pr_fetched_at
                    >= git::github::REFRESH_SECS
                    || info.pr_state != GithubState::Ready);
            // The panel is seven rows tall, border included, so this has to
            // stay one line: the branch's own pull request and the review
            // queue are appended to it rather than given rows of their own.
            let mut pr_line = format!(
                "PR: {count} / {}{} — {}",
                state(info.pr_state),
                if stale_pr {
                    self.tt(" (stale cache)", "（古いキャッシュ）")
                } else {
                    String::new()
                },
                if info.pr_fetched_at > 0 {
                    git::format_timestamp(info.pr_fetched_at)
                } else {
                    "—".into()
                }
            );
            if let Some(pr) = &info.branch_pr {
                let mut flags = Vec::new();
                if pr.is_draft {
                    flags.push(self.tt("draft", "下書き"));
                }
                flags.push(if pr.changes_requested() {
                    self.tt("changes requested", "変更要求")
                } else if pr.approved() {
                    self.tt("approved", "承認済み")
                } else {
                    match pr.review_decision.as_deref() {
                        Some(d) if d.eq_ignore_ascii_case("REVIEW_REQUIRED") => {
                            self.tt("review required", "レビュー必要")
                        }
                        // A decision GitHub added after this build: show it
                        // rather than pretending there is none.
                        Some(other) => other.to_lowercase().replace('_', " "),
                        None => self.tt("no review decision", "レビュー判定なし"),
                    }
                });
                pr_line.push_str(&format!(" | #{} {}", pr.number, flags.join(", ")));
            } else if info.pr_state == GithubState::Ready {
                pr_line.push_str(&format!(
                    " | {}",
                    self.tt("no PR for this branch", "このブランチのPRなし")
                ));
            }
            if reviews > 0 {
                pr_line.push_str(&format!(
                    " | {reviews} {}",
                    self.tt("to review", "件要レビュー")
                ));
            } else if !matches!(info.review_state, GithubState::Ready | GithubState::Unknown) {
                pr_line.push_str(&format!(
                    " | {}: {}",
                    self.tt("review lookup", "レビュー照会"),
                    state(info.review_state)
                ));
            }
            lines.push(pr_line);
        } else {
            lines.push(self.tt(
                "GitHub: no GitHub remote / not fetched",
                "GitHub: 対象リモートなし・未取得",
            ));
        }
        lines
    }

    pub(super) fn open_home_ci(&mut self) {
        let url = self
            .filtered_home()
            .get(self.home_selected)
            .and_then(|i| self.home_rows.get(i))
            .and_then(|r| r.github.as_ref())
            .and_then(|g| g.last_run_url.clone());
        // Only a GitHub HTTPS URL can become an opener argument.
        if let Some(url) =
            url.filter(|u| u.starts_with("https://github.com/") && !u.chars().any(char::is_control))
        {
            #[cfg(target_os = "macos")]
            let (program, args) = ("open", vec![url]);
            #[cfg(target_os = "windows")]
            let (program, args) = ("rundll32", vec!["url.dll,FileProtocolHandler".into(), url]);
            #[cfg(not(any(target_os = "macos", target_os = "windows")))]
            let (program, args) = ("xdg-open", vec![url]);
            self.pending_external = Some(ExternalDiff {
                program: program.into(),
                args,
                cwd: std::env::temp_dir(),
            });
        } else {
            self.error = Some(self.tt(
                "No GitHub CI run URL available",
                "GitHub CI実行URLがありません",
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn summary_uses_scope_and_counts_repositories_without_normalizing_unknowns() {
        let mut app = App::new();
        app.repos = (0..4)
            .map(|i| Repository {
                name: format!("repo-{i}"),
                path: format!("/tmp/{i}").into(),
                group: Some(if i == 3 { "other" } else { "work" }.into()),
            })
            .collect();
        app.home_rows.clear();
        app.home_rows.insert(
            0,
            HomeRow {
                conflicts: 2,
                op_state: GitOpState::Merge,
                ci_status: Some("failure".into()),
                ..Default::default()
            },
        );
        app.home_rows.insert(
            1,
            HomeRow {
                dirty: 5,
                tracked_changes: 2,
                untracked: 3,
                behind: 3,
                ..Default::default()
            },
        );
        app.group_filter = Some("work".into());
        app.attention_only = true;
        assert_eq!(
            app.home_counts(),
            HomeCounts {
                attention: 1,
                dirty: 1,
                untracked: 1,
                sync: 1,
                unknown: 2,
                failed: 0
            }
        );
        assert_eq!(app.filtered_home(), vec![0]);
        app.home_filter = "repo-1".into();
        assert_eq!(
            app.home_counts(),
            HomeCounts {
                attention: 0,
                dirty: 1,
                untracked: 1,
                sync: 1,
                unknown: 0,
                failed: 0
            }
        );
    }

    /// A registered path that cannot be read must not be counted as a clean,
    /// in-sync repository — and it must not be counted as merely "not
    /// fetched yet" either, which is what an absent row means.
    #[test]
    fn unreadable_repository_counts_as_failed_and_needs_attention() {
        let mut app = App::new();
        app.repos = vec![
            Repository {
                name: "broken".into(),
                path: "/tmp/gone".into(),
                group: None,
            },
            Repository {
                name: "fine".into(),
                path: "/tmp/fine".into(),
                group: None,
            },
        ];
        app.home_rows.clear();
        app.home_rows.insert(
            0,
            HomeRow {
                fetched_at: 1,
                error: Some("not a git repository: /tmp/gone (fatal: ...)".into()),
                ..Default::default()
            },
        );
        app.home_rows.insert(
            1,
            HomeRow {
                fetched_at: 1,
                upstream: UpstreamState::Tracking,
                ..Default::default()
            },
        );
        let counts = app.home_counts();
        assert_eq!(counts.failed, 1);
        assert_eq!(counts.attention, 1);
        assert_eq!(counts.dirty, 0);
        assert_eq!(counts.sync, 0);
        // The healthy row is the only one that may be called "unverified".
        assert_eq!(counts.unknown, 0);

        app.attention_only = true;
        assert_eq!(app.filtered_home(), vec![0]);

        app.set_language_for_test(Language::English);
        let text = app.home_context().join("\n");
        assert!(text.contains("cannot read this repository"), "{text}");
        assert!(text.contains("not a git repository"), "{text}");
        assert!(text.contains("/tmp/gone"), "{text}");
        app.set_language_for_test(Language::Japanese);
        assert!(
            app.home_context()
                .join("\n")
                .contains("このリポジトリを読み取れません")
        );
    }

    /// The two freshness stamps in the panel must be distinguishable: one is
    /// when the working tree was read, the other is how old the ahead/behind
    /// numbers are.
    #[test]
    fn context_separates_local_read_from_remote_fetch_in_both_languages() {
        let mut app = App::new();
        app.repos = vec![Repository {
            name: "test".into(),
            path: "/tmp/repo".into(),
            group: None,
        }];
        app.home_rows.clear();
        app.home_rows.insert(
            0,
            HomeRow {
                fetched_at: 1,
                upstream: UpstreamState::NoUpstream,
                last_fetch: LastFetch::Never,
                ..Default::default()
            },
        );
        app.set_language_for_test(Language::English);
        let text = app.home_context().join("\n");
        for word in [
            "Local read",
            "Remote fetched",
            "never fetched",
            "no upstream branch",
            "/tmp/repo",
        ] {
            assert!(text.contains(word), "{text}");
        }

        app.set_language_for_test(Language::Japanese);
        let text = app.home_context().join("\n");
        for word in [
            "ローカル読取",
            "リモート取得",
            "フェッチ履歴なし",
            "上流ブランチなし",
        ] {
            assert!(text.contains(word), "{text}");
        }

        app.home_rows.get_mut(&0).unwrap().last_fetch = LastFetch::At(1_700_000_000);
        app.set_language_for_test(Language::English);
        let text = app.home_context().join("\n");
        assert!(!text.contains("never fetched"), "{text}");
        assert!(
            text.contains(&git::format_timestamp(1_700_000_000)),
            "{text}"
        );

        // An `ssh://` repository has no local FETCH_HEAD to stat: say so
        // rather than claiming it was never fetched.
        app.home_rows.get_mut(&0).unwrap().last_fetch = LastFetch::Unavailable;
        assert!(app.home_context().join("\n").contains("unavailable"));
    }

    #[test]
    fn github_context_exposes_stale_other_branch_and_caps_prs() {
        let mut app = App::new();
        app.repos = vec![Repository {
            name: "test".into(),
            path: "/tmp/repo".into(),
            group: None,
        }];
        app.home_rows.clear();
        app.home_rows.insert(
            0,
            HomeRow {
                branch: "main".into(),
                github: Some(git::RemoteCiPrInfo {
                    ci_state: GithubState::Failed,
                    ci_branch: Some("other-branch".into()),
                    ci_fetched_at: 1,
                    open_prs: Some(100),
                    pr_state: GithubState::Ready,
                    ..Default::default()
                }),
                ..Default::default()
            },
        );
        app.set_language_for_test(Language::English);
        let text = app.home_context().join("\n");
        for word in [
            "latest on branch",
            "other-branch",
            "fetch failed",
            "stale cache",
            "100+",
            // The pull-request lookup succeeded and found nothing for this
            // branch — that is an answer, and it is worth saying so.
            "no PR for this branch",
        ] {
            assert!(text.contains(word), "{text}");
        }
        app.open_home_ci();
        assert!(app.take_external().is_none());
        app.home_rows
            .get_mut(&0)
            .unwrap()
            .github
            .as_mut()
            .unwrap()
            .last_run_url = Some("https://github.com/a/b/actions/runs/1".into());
        app.open_home_ci();
        assert_eq!(
            app.take_external().unwrap().args.last().unwrap(),
            "https://github.com/a/b/actions/runs/1"
        );
    }

    /// Every "needs attention" flag has to be explainable in the panel, in
    /// both languages — a repository marked with no stated reason is worse
    /// than one not marked at all. The panel is seven rows tall, border
    /// included, so the new signals share the pull-request line.
    #[test]
    fn github_context_explains_the_branch_pr_and_the_review_queue() {
        let now = chrono::Utc::now().timestamp();
        let mut app = App::new();
        app.repos = vec![Repository {
            name: "test".into(),
            path: "/tmp/repo".into(),
            group: None,
        }];
        app.home_rows.clear();
        app.home_rows.insert(
            0,
            HomeRow {
                branch: "feature".into(),
                github: Some(git::RemoteCiPrInfo {
                    ci_state: GithubState::Detached,
                    ci_fetched_at: now,
                    pr_state: GithubState::Ready,
                    pr_fetched_at: now,
                    open_prs: Some(2),
                    branch_pr: Some(Box::new(git::BranchPr {
                        number: 42,
                        title: "Scope CI to the branch".into(),
                        url: "https://github.com/a/b/pull/42".into(),
                        is_draft: true,
                        review_decision: Some("CHANGES_REQUESTED".into()),
                    })),
                    review_requests: Some(3),
                    review_state: GithubState::Ready,
                    ..Default::default()
                }),
                ..Default::default()
            },
        );
        assert!(needs_attention(app.home_rows.get(&0).unwrap()));
        app.set_language_for_test(Language::English);
        let lines = app.home_context();
        // The selected-repository panel is Constraint::Length(8) in
        // draw_home, i.e. six rows inside its border. A line past that is
        // silently clipped, so the panel's content is capped here rather
        // than discovered by a user losing the last reason.
        assert!(lines.len() <= 6, "the panel only has six rows: {lines:#?}");
        let text = lines.join("\n");
        for word in [
            "changes requested on this branch's PR",
            "3 PRs awaiting your review",
            "no branch (detached HEAD)",
            "#42",
            "draft",
            "3 to review",
        ] {
            assert!(text.contains(word), "{text}");
        }
        // A detached HEAD is a settled answer, not an unverified one.
        assert_eq!(app.home_counts().unknown, 0);
        app.set_language_for_test(Language::Japanese);
        let text = app.home_context().join("\n");
        for word in [
            "このブランチのPRに変更要求",
            "件のレビュー待ちPR",
            "下書き",
            "変更要求",
            "ブランチなし",
        ] {
            assert!(text.contains(word), "{text}");
        }

        // A review lookup that failed must read as "not known", never as
        // "nobody is waiting on you".
        let info = app.home_rows.get_mut(&0).unwrap().github.as_mut().unwrap();
        info.review_requests = None;
        info.review_state = GithubState::TimedOut;
        info.branch_pr = None;
        app.set_language_for_test(Language::English);
        let text = app.home_context().join("\n");
        assert!(text.contains("review lookup: timed out"), "{text}");
        assert!(text.contains("no PR for this branch"), "{text}");
        assert!(!needs_attention(app.home_rows.get(&0).unwrap()));
    }
}
