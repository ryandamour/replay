use std::collections::BTreeMap;
use std::fmt::Write;
use std::fs;
use std::io::IsTerminal;
use std::path::Path;

use chrono::{Local, NaiveDate};
use walkdir::WalkDir;

use crate::cluster::{self, RetryPattern, SessionThread, TopicCluster};
use crate::error::Result;
use crate::storage::find_replay_dir;
use crate::types::StoredMessage;

/// Filter mode for which messages to include in analysis.
pub enum Filter {
    LastDuration(chrono::Duration),
    LastSessions(usize),
    Today,
    All,
}

impl Default for Filter {
    fn default() -> Self {
        Filter::LastDuration(chrono::Duration::days(30))
    }
}

/// Compute the date cutoff string for partition pruning.
/// Returns None if we can't prune (e.g. session-based filter or All).
fn date_cutoff(filter: &Filter) -> Option<String> {
    let now = Local::now();
    match filter {
        Filter::LastDuration(duration) => {
            Some((now - *duration).format("%Y-%m-%d").to_string())
        }
        Filter::Today => Some(now.format("%Y-%m-%d").to_string()),
        Filter::LastSessions(_) | Filter::All => None,
    }
}

/// Partition pruning predicate: skip `date=` directories outside the range.
fn should_enter(entry: &walkdir::DirEntry, cutoff: Option<&str>) -> bool {
    if !entry.file_type().is_dir() {
        return true;
    }
    let name = entry.file_name().to_str().unwrap_or("");
    if let Some(date) = name.strip_prefix("date=") {
        if let Some(cutoff) = cutoff {
            return date >= cutoff;
        }
    }
    true
}

fn parse_message(path: &Path, replay_dir: &Path) -> Option<StoredMessage> {
    let rel = path.strip_prefix(replay_dir.join("messages")).ok()?;
    let components: Vec<&str> = rel
        .components()
        .map(|c| c.as_os_str().to_str().unwrap_or(""))
        .collect();

    if components.len() != 4 {
        return None;
    }

    components[0].strip_prefix("user=")?;
    let date = components[1].strip_prefix("date=")?;
    let hour = components[2].strip_prefix("hour=")?;
    let filename = components[3].strip_suffix(".md")?;

    let underscore_pos = filename.find('_')?;
    let session_id = &filename[..underscore_pos];
    let minute_second_ms = &filename[underscore_pos + 1..];

    let content = fs::read_to_string(path).ok()?;
    let prompt = extract_prompt(&content);

    Some(StoredMessage {
        date: date.to_string(),
        hour: hour.to_string(),
        session_id: session_id.to_string(),
        minute_second_ms: minute_second_ms.to_string(),
        prompt,
    })
}

fn extract_prompt(content: &str) -> String {
    let mut parts = content.splitn(3, "---");
    parts.next();
    parts.next();
    match parts.next() {
        Some(rest) => rest.trim().to_string(),
        None => content.trim().to_string(),
    }
}

/// Apply session-based filter (can't be partition-pruned).
fn apply_session_filter(messages: &mut Vec<StoredMessage>, n: usize) {
    let mut seen_sessions: Vec<String> = Vec::new();
    messages.sort_by(|a, b| b.sort_key().cmp(&a.sort_key()));
    for msg in messages.iter() {
        if !seen_sessions.contains(&msg.session_id) {
            seen_sessions.push(msg.session_id.clone());
        }
    }
    let keep: Vec<String> = seen_sessions.into_iter().take(n).collect();
    messages.retain(|m| keep.contains(&m.session_id));
    messages.sort_by(|a, b| a.sort_key().cmp(&b.sort_key()));
}

pub fn run(filter: Filter, topic: Option<String>) -> Result<()> {
    let cwd = if std::io::stdin().is_terminal() {
        std::env::current_dir()?
    } else {
        let mut input = String::new();
        std::io::Read::read_to_string(&mut std::io::stdin(), &mut input)?;
        if let Ok(parsed) = serde_json::from_str::<crate::types::SessionStartInput>(&input) {
            std::path::PathBuf::from(&parsed.hook.cwd)
        } else {
            std::env::current_dir()?
        }
    };

    let replay_dir = find_replay_dir(&cwd)?;
    let messages_dir = replay_dir.join("messages");

    if !messages_dir.exists() {
        println!("No messages found.");
        return Ok(());
    }

    // Partition pruning: compute cutoff and skip directories during walk
    let cutoff = date_cutoff(&filter);
    let cutoff_ref = cutoff.as_deref();

    let mut messages: Vec<StoredMessage> = WalkDir::new(&messages_dir)
        .into_iter()
        .filter_entry(|e| should_enter(e, cutoff_ref))
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "md"))
        .filter_map(|e| parse_message(e.path(), &replay_dir))
        .collect();

    if messages.is_empty() {
        println!("No messages found.");
        return Ok(());
    }

    messages.sort_by(|a, b| a.sort_key().cmp(&b.sort_key()));

    // Session filter can't be partition-pruned, apply after load
    if let Filter::LastSessions(n) = &filter {
        apply_session_filter(&mut messages, *n);
    }

    // Topic filter: case-insensitive substring match
    if let Some(ref keyword) = topic {
        let keyword_lower = keyword.to_lowercase();
        messages.retain(|m| m.prompt.to_lowercase().contains(&keyword_lower));
    }

    if messages.is_empty() {
        println!("No messages matched.");
        return Ok(());
    }

    // Run analysis passes
    let topics = cluster::cluster_by_topic(&messages);
    let retries = cluster::detect_retries(&messages);
    let threads = cluster::find_cross_session_threads(&messages);

    let mut session_set: Vec<&str> = messages.iter().map(|m| m.session_id.as_str()).collect();
    session_set.sort();
    session_set.dedup();
    let session_count = session_set.len();

    let mut by_date: BTreeMap<String, BTreeMap<String, Vec<&StoredMessage>>> = BTreeMap::new();
    for msg in &messages {
        by_date
            .entry(msg.date.clone())
            .or_default()
            .entry(msg.session_id.clone())
            .or_default()
            .push(msg);
    }

    let mut output = String::new();
    render_full(
        &mut output,
        &messages,
        &by_date,
        session_count,
        &topics,
        &retries,
        &threads,
        topic.as_deref(),
    );

    let is_hook = !std::io::stdin().is_terminal();
    if is_hook {
        let hook_output = serde_json::json!({ "result": output });
        print!("{}", hook_output);
    } else {
        print!("{}", output);
    }

    Ok(())
}

fn render_full(
    out: &mut String,
    messages: &[StoredMessage],
    by_date: &BTreeMap<String, BTreeMap<String, Vec<&StoredMessage>>>,
    session_count: usize,
    topics: &[TopicCluster],
    retries: &[RetryPattern],
    threads: &[SessionThread],
    topic_filter: Option<&str>,
) {
    writeln!(out, "# Replay: Conversation History").unwrap();
    writeln!(out).unwrap();
    writeln!(
        out,
        "The following is a chronological record of user prompts from this project,"
    )
    .unwrap();
    writeln!(
        out,
        "preserved across context compaction. Use this to maintain continuity."
    )
    .unwrap();
    writeln!(out).unwrap();

    if let Some(kw) = topic_filter {
        writeln!(
            out,
            "**Filtered by topic**: \"{}\" — {} messages across {} sessions",
            kw,
            messages.len(),
            session_count
        )
        .unwrap();
    } else {
        writeln!(
            out,
            "**Summary**: {} messages across {} sessions",
            messages.len(),
            session_count
        )
        .unwrap();
    }
    writeln!(out).unwrap();

    // --- Topics ---
    if !topics.is_empty() {
        writeln!(out, "## Topics").unwrap();
        writeln!(out).unwrap();
        for cluster in topics {
            writeln!(
                out,
                "- **{}** ({} messages): {}",
                cluster.label,
                cluster.message_count,
                cluster.keywords.join(", ")
            )
            .unwrap();
        }
        writeln!(out).unwrap();
    }

    // --- Retry patterns ---
    if !retries.is_empty() {
        writeln!(out, "## Retry Patterns Detected").unwrap();
        writeln!(out).unwrap();
        for pattern in retries {
            writeln!(
                out,
                "**Session ...{}** — {} attempts at a similar prompt:",
                pattern.session_id,
                pattern.attempts.len()
            )
            .unwrap();
            for (i, attempt) in pattern.attempts.iter().enumerate() {
                let marker = if i == 0 {
                    "First"
                } else if i == pattern.attempts.len() - 1 {
                    "Last"
                } else {
                    "Then"
                };
                writeln!(
                    out,
                    "  - *{}* [{}]: {}",
                    marker, attempt.time, attempt.prompt_summary
                )
                .unwrap();
            }
            writeln!(out).unwrap();
        }
    }

    // --- Cross-session threads ---
    if !threads.is_empty() {
        writeln!(out, "## Cross-Session Threads").unwrap();
        writeln!(out).unwrap();
        for thread in threads {
            writeln!(
                out,
                "**{}** — spans {} sessions across multiple days:",
                thread.topic,
                thread.sessions.len()
            )
            .unwrap();
            for s in &thread.sessions {
                writeln!(
                    out,
                    "  - Session ...{} ({}): {}",
                    s.session_id, s.date, s.summary
                )
                .unwrap();
            }
            writeln!(out).unwrap();
        }
    }

    // --- Chronological listing ---
    for (date, sessions) in by_date {
        let date_header = if let Ok(parsed) = NaiveDate::parse_from_str(date, "%Y-%m-%d") {
            parsed.format("%Y-%m-%d (%A)").to_string()
        } else {
            date.clone()
        };
        writeln!(out, "## {}", date_header).unwrap();
        writeln!(out).unwrap();

        for (session_id, msgs) in sessions {
            writeln!(
                out,
                "### Session ...{} ({} messages)",
                session_id,
                msgs.len()
            )
            .unwrap();
            writeln!(out).unwrap();

            for msg in msgs {
                let time = format_time(&msg.hour, &msg.minute_second_ms);
                let summary = truncate_prompt(&msg.prompt, 120);
                writeln!(out, "- **[{}]** {}", time, summary).unwrap();
            }
            writeln!(out).unwrap();
        }
    }

    writeln!(out, "---").unwrap();
    writeln!(
        out,
        "*This history was injected by `replay` after context compaction.*"
    )
    .unwrap();
}

fn format_time(hour: &str, minute_second_ms: &str) -> String {
    let parts: Vec<&str> = minute_second_ms.splitn(3, '-').collect();
    if parts.len() >= 2 {
        format!("{}:{}:{}", hour, parts[0], parts[1])
    } else {
        format!("{}:{}", hour, minute_second_ms)
    }
}

fn truncate_prompt(prompt: &str, max_len: usize) -> String {
    let first_line = prompt.lines().next().unwrap_or(prompt);
    if first_line.len() <= max_len {
        first_line.to_string()
    } else {
        format!("{}...", &first_line[..max_len])
    }
}
