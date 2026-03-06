use std::fs;
use std::io::{self, Read};
use std::path::Path;

use chrono::Local;
use fs2::FileExt as _;

use crate::error::{ReplayError, Result};
use crate::init::init_replay_dir;
use crate::storage::{find_replay_dir, get_username, message_path};
use crate::types::UserPromptSubmitInput;
use crate::{git, JSON_OUTPUT};

pub fn run() -> Result<()> {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input)?;
    let parsed: UserPromptSubmitInput = serde_json::from_str(&input)?;

    let cwd = Path::new(&parsed.hook.cwd);

    // Find or auto-init .replay/
    let replay_dir = match find_replay_dir(cwd) {
        Ok(dir) => dir,
        Err(_) => {
            init_replay_dir(cwd)?;
            cwd.join(".replay")
        }
    };

    // Acquire file lock
    let lock_path = replay_dir.join(".lock");
    let lock_file = fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(false)
        .open(&lock_path)?;
    lock_file
        .lock_exclusive()
        .map_err(|e| ReplayError::Lock(e.to_string()))?;

    // Get username (errors are non-fatal for hooks — fall back to "unknown")
    let username = get_username(cwd).unwrap_or_else(|_| "unknown".to_string());

    let now = Local::now();
    let path = message_path(&replay_dir, &username, &now, &parsed.hook.session_id);

    // Ensure parent directories exist
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    // Write markdown file
    let content = format!(
        "---\nsession: {}\nuser: {}\ntimestamp: {}\n---\n\n{}\n",
        parsed.hook.session_id,
        username,
        now.to_rfc3339(),
        parsed.prompt,
    );
    fs::write(&path, content)?;

    // Git commit
    git::add_and_commit(
        &replay_dir,
        &format!("capture: {}", now.format("%Y-%m-%d %H:%M:%S")),
    )?;

    // Unlock (dropped automatically, but explicit for clarity)
    fs2::FileExt::unlock(&lock_file).ok();

    // Must produce valid JSON for hook protocol, but empty result so we don't inject context
    print!("{}", JSON_OUTPUT);

    Ok(())
}
