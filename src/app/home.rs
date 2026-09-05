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
                        !matches!(info.ci_state, GithubState::Ready | GithubState::NoRuns)
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
        if row
            .ci_status
            .as_deref()
            .is_some_and(|s| classify_ci_status(s) == CiOutcome::Failure)
        {
            reasons.push(self.tt(
                "CI failure (repository-wide latest run)",
                "CI失敗（リポジトリ全体の最新実行）",
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
                    },
                    match s {
                        GithubState::Ready => "取得済み",
                        GithubState::NoRuns => "実行なし",
                        GithubState::Unknown => "未取得",
                        GithubState::Unauthenticated => "未認証 (gh auth login)",
                        GithubState::Failed => "取得失敗",
                        GithubState::Unavailable => "gh未導入",
                        GithubState::Unsupported => "SSH非対応",
                    },
                )
            };
            let stale = info.ci_fetched_at > 0
                && (chrono::Utc::now().timestamp() - info.ci_fetched_at
                    >= git::github::REFRESH_SECS
                    || !matches!(info.ci_state, GithubState::Ready | GithubState::NoRuns));
            let time = if info.ci_fetched_at > 0 {
                git::format_timestamp(info.ci_fetched_at)
            } else {
                "—".into()
            };
            lines.push(format!(
                "CI {} [{}]: {} / {} {}{}",
                self.tt("repository-wide latest", "全体の最新"),
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
            lines.push(format!(
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
            ));
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
            "repository-wide latest",
            "other-branch",
            "fetch failed",
            "stale cache",
            "100+",
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
}
