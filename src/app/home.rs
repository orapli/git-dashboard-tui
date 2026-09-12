use super::*;
use crate::git::{GitOpState, GithubState};

#[derive(Default, Debug, PartialEq, Eq)]
pub struct HomeCounts {
    pub attention: usize,
    pub dirty: usize,
    pub sync: usize,
    pub unknown: usize,
}

impl App {
    pub fn home_counts(&self) -> HomeCounts {
        let mut counts = HomeCounts::default();
        for i in filter_repo_indices(&self.repos, &self.home_filter, self.group_filter.as_deref()) {
            if let Some(row) = self.home_rows.get(&i) {
                counts.attention += usize::from(needs_attention(row));
                counts.dirty += usize::from(row.dirty > 0);
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
        let Some(row) = self.home_rows.get(&i) else {
            return vec![self.tt(
                "Local status unavailable / loading — r: reload",
                "ローカル状態は未取得・取得中 — r: 再読み込み",
            )];
        };
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
        let mut lines = vec![
            reasons.join(" / "),
            format!(
                "Enter: {}  t: {}  C: CI",
                self.tt("details / diff", "詳細・差分"),
                self.tt("shell", "シェル")
            ),
            format!(
                "{}: {local} — r: {} / F: git fetch",
                self.tt("Local checked", "ローカル取得"),
                self.tt("reload local status", "ローカル再読込")
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
                dirty: 2,
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
                sync: 1,
                unknown: 2
            }
        );
        assert_eq!(app.filtered_home(), vec![0]);
        app.home_filter = "repo-1".into();
        assert_eq!(
            app.home_counts(),
            HomeCounts {
                attention: 0,
                dirty: 1,
                sync: 1,
                unknown: 0
            }
        );
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
        assert!(lines.len() <= 5, "the panel only has five rows: {lines:#?}");
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
