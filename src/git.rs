use std::path::Path;
use std::process::Command;

use crate::error::{ReplayError, Result};

/// Run a git command inside the `.replay/` directory.
fn git(replay_dir: &Path, args: &[&str]) -> Result<()> {
    let output = Command::new("git")
        .args(args)
        .current_dir(replay_dir)
        .output()
        .map_err(|e| ReplayError::Git(format!("failed to run git: {}", e)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(ReplayError::Git(format!(
            "git {} failed: {}",
            args.join(" "),
            stderr.trim()
        )));
    }
    Ok(())
}

/// Initialize a git repo inside `.replay/` with a local user config.
pub fn init(replay_dir: &Path) -> Result<()> {
    git(replay_dir, &["init"])?;
    git(replay_dir, &["config", "user.email", "replay@local"])?;
    git(replay_dir, &["config", "user.name", "replay"])?;
    Ok(())
}

/// Stage all changes and commit with the given message.
pub fn add_and_commit(replay_dir: &Path, message: &str) -> Result<()> {
    git(replay_dir, &["add", "-A"])?;

    // Check if there's anything to commit
    let output = Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(replay_dir)
        .output()
        .map_err(|e| ReplayError::Git(format!("failed to run git: {}", e)))?;

    let status = String::from_utf8_lossy(&output.stdout);
    if status.trim().is_empty() {
        return Ok(()); // Nothing to commit
    }

    git(replay_dir, &["commit", "-m", message])?;
    Ok(())
}
