use std::path::{Path, PathBuf};
use std::process::Command;

use chrono::{DateTime, Local};

use crate::error::{ReplayError, Result};

/// Walk up from `cwd` looking for a `.replay/` directory.
pub fn find_replay_dir(cwd: &Path) -> Result<PathBuf> {
    let mut dir = cwd.to_path_buf();
    loop {
        let candidate = dir.join(".replay");
        if candidate.is_dir() {
            return Ok(candidate);
        }
        if !dir.pop() {
            return Err(ReplayError::NotInitialized(cwd.to_path_buf()));
        }
    }
}

/// Build the hive-partitioned path for a message file.
///
/// `.replay/messages/user=<username>/date=YYYY-MM-DD/hour=HH/<session_last8>_MM-SS-ms.md`
pub fn message_path(
    replay_dir: &Path,
    username: &str,
    now: &DateTime<Local>,
    session_id: &str,
) -> PathBuf {
    let date = now.format("%Y-%m-%d").to_string();
    let hour = now.format("%H").to_string();
    let session_last8 = if session_id.len() >= 8 {
        &session_id[session_id.len() - 8..]
    } else {
        session_id
    };
    let filename = format!(
        "{}_{}.md",
        session_last8,
        now.format("%M-%S-%3f")
    );

    replay_dir
        .join("messages")
        .join(format!("user={}", username))
        .join(format!("date={}", date))
        .join(format!("hour={}", hour))
        .join(filename)
}

/// Resolve the git username from `git config user.name` run in `cwd`.
pub fn get_username(cwd: &Path) -> Result<String> {
    let output = Command::new("git")
        .args(["config", "user.name"])
        .current_dir(cwd)
        .output()
        .map_err(|e| ReplayError::Git(format!("failed to run git: {}", e)))?;

    if !output.status.success() {
        return Err(ReplayError::Git(
            "git config user.name is not set".to_string(),
        ));
    }

    let name = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if name.is_empty() {
        return Err(ReplayError::Git(
            "git config user.name is empty".to_string(),
        ));
    }

    Ok(name)
}
