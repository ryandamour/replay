use std::fs;
use std::path::Path;

use crate::error::Result;
use crate::git;

/// Create the `.replay/` directory structure, initialize git, and make an initial commit.
pub fn init_replay_dir(base: &Path) -> Result<()> {
    let replay_dir = base.join(".replay");
    fs::create_dir_all(replay_dir.join("messages"))?;
    fs::create_dir_all(replay_dir.join("analysis"))?;

    fs::write(
        replay_dir.join(".gitignore"),
        ".lock\n",
    )?;

    git::init(&replay_dir)?;
    git::add_and_commit(&replay_dir, "init: scaffold .replay directory")?;

    Ok(())
}
