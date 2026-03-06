use std::fs;
use std::path::PathBuf;

use crate::error::{ReplayError, Result};

const COMMAND_TEMPLATE: &str = r#"Analyze the conversation history stored by the `replay` plugin and present a summary.

The user may provide additional context after `/replay`:
- A topic keyword like "auth" or "database" → pass as `--topic <keyword>`
- "--all" to load everything → pass `--all`
- "--last 90d" for a custom time range → pass through directly
- "--today" for just today → pass `--today`
- Multiple flags can combine, e.g. "/replay --all auth" → `--all --topic auth`

Run the appropriate command and present the output. Examples:

- `/replay` → run: `replay analyze`
- `/replay auth` → run: `replay analyze --topic auth`
- `/replay --all` → run: `replay analyze --all`
- `/replay --all auth` → run: `replay analyze --all --topic auth`
- `/replay --last 90d migration` → run: `replay analyze --last 90d --topic migration`

After running the command, present the output as-is — it's already formatted markdown.
If there are retry patterns or cross-session threads, call them out briefly.
"#;

fn claude_dir() -> Result<PathBuf> {
    let home = std::env::var("HOME")
        .map_err(|_| ReplayError::Git("HOME not set".to_string()))?;
    Ok(PathBuf::from(home).join(".claude"))
}

pub fn run() -> Result<()> {
    let claude = claude_dir()?;

    // 1. Write slash command
    let commands_dir = claude.join("commands");
    fs::create_dir_all(&commands_dir)?;
    let cmd_path = commands_dir.join("replay.md");
    fs::write(&cmd_path, COMMAND_TEMPLATE)?;
    eprintln!("  wrote {}", cmd_path.display());

    // 2. Merge hooks into settings.json
    let settings_path = claude.join("settings.json");
    let mut settings: serde_json::Value = if settings_path.exists() {
        let content = fs::read_to_string(&settings_path)?;
        serde_json::from_str(&content)?
    } else {
        serde_json::json!({})
    };

    let hooks = settings
        .as_object_mut()
        .ok_or_else(|| ReplayError::Git("settings.json is not an object".to_string()))?
        .entry("hooks")
        .or_insert_with(|| serde_json::json!({}));

    let hooks_obj = hooks
        .as_object_mut()
        .ok_or_else(|| ReplayError::Git("hooks is not an object".to_string()))?;

    // Add UserPromptSubmit hook if replay capture isn't already there
    let capture_hook = serde_json::json!({
        "hooks": [{
            "type": "command",
            "command": "replay capture",
            "async": true
        }]
    });

    let prompt_submit = hooks_obj
        .entry("UserPromptSubmit")
        .or_insert_with(|| serde_json::json!([]));

    if !has_replay_hook(prompt_submit, "replay capture") {
        prompt_submit
            .as_array_mut()
            .ok_or_else(|| ReplayError::Git("UserPromptSubmit is not an array".to_string()))?
            .push(capture_hook);
        eprintln!("  added UserPromptSubmit → replay capture hook");
    } else {
        eprintln!("  UserPromptSubmit → replay capture hook already exists");
    }

    // Add SessionStart hook if replay analyze isn't already there
    let analyze_hook = serde_json::json!({
        "matcher": "compact",
        "hooks": [{
            "type": "command",
            "command": "replay analyze"
        }]
    });

    let session_start = hooks_obj
        .entry("SessionStart")
        .or_insert_with(|| serde_json::json!([]));

    if !has_replay_hook(session_start, "replay analyze") {
        session_start
            .as_array_mut()
            .ok_or_else(|| ReplayError::Git("SessionStart is not an array".to_string()))?
            .push(analyze_hook);
        eprintln!("  added SessionStart → replay analyze hook");
    } else {
        eprintln!("  SessionStart → replay analyze hook already exists");
    }

    // Write settings back
    let formatted = serde_json::to_string_pretty(&settings)?;
    fs::write(&settings_path, formatted + "\n")?;
    eprintln!("  wrote {}", settings_path.display());

    eprintln!("\nreplay installed. Use /replay inside Claude Code.");
    Ok(())
}

/// Check if a hooks array already contains a hook with the given command.
fn has_replay_hook(hooks_array: &serde_json::Value, command: &str) -> bool {
    let Some(arr) = hooks_array.as_array() else {
        return false;
    };
    arr.iter().any(|entry| {
        entry
            .get("hooks")
            .and_then(|h| h.as_array())
            .is_some_and(|hooks| {
                hooks.iter().any(|hook| {
                    hook.get("command")
                        .and_then(|c| c.as_str())
                        .is_some_and(|c| c == command)
                })
            })
    })
}
