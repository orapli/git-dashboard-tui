use super::exec::{
    GIT_NETWORK_TIMEOUT, check_safe_ref, git_command_for, run_git_cmd, run_with_timeout,
};
use super::types::StashEntry;
use crate::config::Language;
use std::path::Path;

pub fn pull_repository(repo_path: &Path, lang: Language) -> Result<String, String> {
    // 1. Check whether a remote is configured
    let remote_output = run_git_cmd(repo_path, &["remote"]).unwrap_or_default();
    if remote_output.trim().is_empty() {
        return Err(crate::i18n::t(lang, "git_no_remote_pull").to_string());
    }

    // 2. Check that an upstream tracking branch is configured
    if run_git_cmd(repo_path, &["rev-parse", "--abbrev-ref", "@{u}"]).is_err() {
        return Err(crate::i18n::t(lang, "git_no_upstream").to_string());
    }

    let output = run_with_timeout(git_command_for(repo_path, &["pull"]), GIT_NETWORK_TIMEOUT)
        .map_err(|e| format!("git pull: {e}"))?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    if output.status.success() {
        if stdout.trim().is_empty() {
            Ok(crate::i18n::t(lang, "git_already_up_to_date").to_string())
        } else {
            Ok(stdout.trim().to_string())
        }
    } else {
        Err(if stderr.trim().is_empty() {
            stdout.trim().to_string()
        } else {
            stderr.trim().to_string()
        })
    }
}

pub fn fetch_repository(repo_path: &Path, lang: Language) -> Result<String, String> {
    // 1. Check whether a remote is configured
    let remote_output = run_git_cmd(repo_path, &["remote"]).unwrap_or_default();
    if remote_output.trim().is_empty() {
        return Err(crate::i18n::t(lang, "git_no_remote_fetch").to_string());
    }

    let output = run_with_timeout(git_command_for(repo_path, &["fetch"]), GIT_NETWORK_TIMEOUT)
        .map_err(|e| format!("git fetch: {e}"))?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    if output.status.success() {
        Ok(crate::i18n::t(lang, "git_fetch_success").to_string())
    } else {
        Err(if stderr.trim().is_empty() {
            stdout.trim().to_string()
        } else {
            stderr.trim().to_string()
        })
    }
}

// Helper to format file sizes

pub fn get_stash_list(repo_path: &Path) -> Result<Vec<StashEntry>, String> {
    let output = run_git_cmd(repo_path, &["stash", "list", "--format=%gd|%cr|%an|%s"])?;
    let mut stashes = Vec::new();
    for line in output.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let parts: Vec<&str> = line.splitn(4, '|').collect();
        if parts.len() == 4 {
            stashes.push(StashEntry {
                ref_name: parts[0].trim().to_string(),
                date_relative: parts[1].trim().to_string(),
                author: parts[2].trim().to_string(),
                message: parts[3].trim().to_string(),
            });
        }
    }
    Ok(stashes)
}

pub fn apply_stash(repo_path: &Path, ref_name: &str) -> Result<String, String> {
    check_safe_ref(ref_name)?;
    run_git_cmd(repo_path, &["stash", "apply", ref_name])
}

pub fn drop_stash(repo_path: &Path, ref_name: &str) -> Result<String, String> {
    check_safe_ref(ref_name)?;
    run_git_cmd(repo_path, &["stash", "drop", ref_name])
}
