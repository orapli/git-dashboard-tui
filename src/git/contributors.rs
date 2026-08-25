use super::exec::run_git_cmd;
use super::status::{CONTRIBUTOR_LOG_LIMIT, format_timestamp, split_fields};
use super::types::{Activity, Contributor, DailyActivity, TimeSpan};
use crate::config::Member;
use chrono::{Datelike, Duration, Local, TimeZone, Timelike, Utc};
use std::collections::HashMap;
use std::path::Path;

pub fn process_contributor_log(log_data: &str, members: &[Member]) -> Vec<Contributor> {
    let mut stats: HashMap<String, (usize, i64, i64, std::collections::HashSet<String>)> =
        HashMap::new();
    let mut total_commits = 0;

    for line in log_data.lines() {
        let [raw_name, raw_email, ts] = split_fields::<3>(line);
        if raw_name.is_empty() && raw_email.is_empty() {
            continue;
        }
        let raw_name = raw_name.to_string();
        let raw_email = raw_email.to_string();
        let timestamp: i64 = ts.parse().unwrap_or(0);

        let mut resolved_name = raw_name.clone();

        for m in members {
            let matches_canonical = m.canonical_name.eq_ignore_ascii_case(&raw_name);
            let matches_alias = m.aliases.iter().any(|alias| {
                alias.eq_ignore_ascii_case(&raw_name) || alias.eq_ignore_ascii_case(&raw_email)
            });

            if matches_canonical || matches_alias {
                resolved_name = m.canonical_name.clone();
                break;
            }
        }

        let entry = stats.entry(resolved_name).or_insert((
            0,
            timestamp,
            timestamp,
            std::collections::HashSet::new(),
        ));
        entry.0 += 1;
        if timestamp < entry.1 {
            entry.1 = timestamp;
        }
        if timestamp > entry.2 {
            entry.2 = timestamp;
        }
        if !raw_email.is_empty() {
            entry.3.insert(raw_email);
        }
        total_commits += 1;
    }

    let mut contributors = Vec::new();
    for (name, (count, first, last, emails)) in stats {
        let pct = if total_commits > 0 {
            (count as f64 / total_commits as f64) * 100.0
        } else {
            0.0
        };

        let first_date = format_timestamp(first);
        let last_date = format_timestamp(last);

        let mut is_member = false;
        let mut is_active = false;
        for m in members {
            if m.canonical_name == name {
                is_member = true;
                is_active = m.is_active;
                break;
            }
        }

        let email_list = emails.into_iter().collect::<Vec<_>>().join(", ");

        contributors.push(Contributor {
            name,
            email: email_list,
            commit_count: count,
            percentage: pct,
            first_commit: first_date,
            last_commit: last_date,
            is_member,
            is_active,
        });
    }

    contributors.sort_by_key(|b| std::cmp::Reverse(b.commit_count));
    contributors
}

pub fn get_contributors(
    repo_path: &Path,
    members: &[Member],
    time_span: TimeSpan,
) -> Result<Vec<Contributor>, String> {
    let mut args = vec![
        "log",
        "-n",
        CONTRIBUTOR_LOG_LIMIT,
        "--format=%an|||%ae|||%at",
    ];
    let since_opt;
    if let Some(since) = time_span.since_arg() {
        since_opt = format!("--since={since}");
        args.push(&since_opt);
    }
    let log_data = match run_git_cmd(repo_path, &args) {
        Ok(data) => data,
        Err(_) => return Ok(Vec::new()), // no commits yet
    };

    Ok(process_contributor_log(&log_data, members))
}

pub fn build_activity(timestamps: &[i64], days: usize, today: chrono::NaiveDate) -> Activity {
    let mut hourly = vec![0; 24];
    let mut weekly = vec![0; 7];

    let span_days: i64 = if days > 0 {
        days as i64
    } else {
        timestamps
            .iter()
            .filter_map(|ts| Utc.timestamp_opt(*ts, 0).single())
            .map(|dt| {
                today
                    .signed_duration_since(dt.with_timezone(&Local).date_naive())
                    .num_days()
            })
            .max()
            .unwrap_or(0)
            + 1
    };
    let span_days = span_days.max(1);

    let bucket_days: i64 = if span_days <= 45 {
        1
    } else if span_days <= 217 {
        7
    } else {
        // ceil div by hand: i64::div_ceil is unstable (values are positive)
        (span_days + 30) / 31
    };
    let num_buckets = (((span_days + bucket_days - 1) / bucket_days).max(1)) as usize;

    // Bucket i (0 = newest) covers [i*bucket, (i+1)*bucket) days ago; the
    // label is the bucket's oldest day, rendered oldest-first
    let mut counts = vec![0usize; num_buckets];
    let mut dates = Vec::with_capacity(num_buckets);
    for i in (0..num_buckets as i64).rev() {
        let start = today - Duration::days((i + 1) * bucket_days - 1);
        let label = if span_days > 366 {
            start.format("%y/%m").to_string()
        } else {
            start.format("%m/%d").to_string()
        };
        dates.push(label);
    }

    for &ts in timestamps {
        let Some(dt) = Utc.timestamp_opt(ts, 0).single() else {
            continue;
        };
        let local_dt = dt.with_timezone(&Local);
        let days_ago = today
            .signed_duration_since(local_dt.date_naive())
            .num_days();
        // git log --since filters by committer date but %at is the author
        // timestamp, so out-of-window stragglers are dropped here too
        if days_ago < 0 || days_ago >= span_days {
            continue;
        }

        let hour = local_dt.hour() as usize;
        if hour < 24 {
            hourly[hour] += 1;
        }
        let weekday = local_dt.weekday().num_days_from_sunday() as usize;
        if weekday < 7 {
            weekly[weekday] += 1;
        }
        let b = (days_ago / bucket_days) as usize;
        if b < num_buckets {
            counts[num_buckets - 1 - b] += 1;
        }
    }

    Activity {
        daily: DailyActivity { dates, counts },
        hourly,
        weekly,
    }
}

/// Commit activity over the last `days` days (0 = entire history).
pub fn get_activity(repo_path: &Path, days: usize) -> Result<Activity, String> {
    let since_arg = format!("--since={days} days ago");
    let mut args = vec!["log", "--format=%at"];
    if days > 0 {
        args.push(&since_arg);
    }
    let log_data = match run_git_cmd(repo_path, &args) {
        Ok(data) => data,
        Err(_) => {
            return Ok(Activity {
                daily: DailyActivity {
                    dates: vec![],
                    counts: vec![],
                },
                hourly: vec![0; 24],
                weekly: vec![0; 7],
            });
        }
    };

    let timestamps: Vec<i64> = log_data
        .lines()
        .filter_map(|line| line.trim().parse().ok())
        .collect();
    Ok(build_activity(&timestamps, days, Local::now().date_naive()))
}
