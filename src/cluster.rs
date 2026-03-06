use std::collections::{HashMap, HashSet};

use crate::types::StoredMessage;

const STOP_WORDS: &[&str] = &[
    "the", "a", "an", "and", "or", "but", "in", "on", "at", "to", "for",
    "of", "with", "by", "from", "is", "it", "its", "this", "that", "are",
    "was", "be", "been", "have", "has", "had", "do", "does", "did", "will",
    "would", "could", "should", "may", "might", "can", "not", "no", "so",
    "if", "then", "than", "too", "very", "just", "about", "up", "out",
    "how", "all", "each", "every", "both", "few", "more", "most", "other",
    "some", "such", "only", "own", "same", "into", "also", "get", "got",
    "make", "made", "use", "used", "using", "set", "add", "new", "now",
    "way", "like", "want", "need", "try", "let", "please", "help", "me",
    "my", "i", "you", "your", "we", "our", "they", "them", "their",
    "what", "which", "when", "where", "why", "there", "here", "still",
    "back", "don", "doesn", "didn", "can", "won", "shouldn", "wouldn",
    "sure", "look", "see", "file", "code", "change", "update", "run",
    "work", "one", "two", "thing", "right", "well", "going", "take",
];

/// A cluster of semantically related prompts.
pub struct TopicCluster {
    pub label: String,
    pub keywords: Vec<String>,
    pub message_count: usize,
}

/// A sequence of similar prompts indicating a retry/struggle pattern.
pub struct RetryPattern {
    pub session_id: String,
    pub attempts: Vec<RetryAttempt>,
}

pub struct RetryAttempt {
    pub time: String,
    pub prompt_summary: String,
}

/// Sessions across different dates that share a common thread.
pub struct SessionThread {
    pub topic: String,
    pub sessions: Vec<ThreadedSession>,
}

pub struct ThreadedSession {
    pub session_id: String,
    pub date: String,
    pub summary: String,
}

/// Extract significant words from a prompt.
fn extract_keywords(text: &str) -> HashSet<String> {
    let stop: HashSet<&str> = STOP_WORDS.iter().copied().collect();
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric() && c != '_')
        .filter(|w| w.len() > 2 && !stop.contains(w))
        .map(|w| w.to_string())
        .collect()
}

/// Jaccard similarity between two keyword sets.
fn jaccard(a: &HashSet<String>, b: &HashSet<String>) -> f64 {
    if a.is_empty() && b.is_empty() {
        return 0.0;
    }
    let intersection = a.intersection(b).count();
    let union = a.union(b).count();
    intersection as f64 / union as f64
}

/// Cluster messages by topic similarity using greedy assignment.
pub fn cluster_by_topic(messages: &[StoredMessage]) -> Vec<TopicCluster> {
    if messages.is_empty() {
        return Vec::new();
    }

    let keywords_per_msg: Vec<HashSet<String>> =
        messages.iter().map(|m| extract_keywords(&m.prompt)).collect();

    // Greedy clustering: assign each message to the best existing cluster,
    // or start a new one if similarity is below threshold.
    struct RawCluster {
        keywords: HashSet<String>,
        indices: Vec<usize>,
    }

    let mut clusters: Vec<RawCluster> = Vec::new();
    let threshold = 0.12;

    for (i, kw) in keywords_per_msg.iter().enumerate() {
        if kw.is_empty() {
            continue;
        }

        let mut best_idx = None;
        let mut best_sim = threshold;

        for (ci, cluster) in clusters.iter().enumerate() {
            let sim = jaccard(kw, &cluster.keywords);
            if sim > best_sim {
                best_sim = sim;
                best_idx = Some(ci);
            }
        }

        if let Some(ci) = best_idx {
            for k in kw {
                clusters[ci].keywords.insert(k.clone());
            }
            clusters[ci].indices.push(i);
        } else {
            clusters.push(RawCluster {
                keywords: kw.clone(),
                indices: vec![i],
            });
        }
    }

    // Convert to TopicCluster, keeping only clusters with 2+ messages
    let mut result: Vec<TopicCluster> = clusters
        .into_iter()
        .filter(|c| c.indices.len() >= 2)
        .map(|c| {
            // Rank keywords by frequency across cluster messages
            let mut freq: HashMap<&str, usize> = HashMap::new();
            for &idx in &c.indices {
                for k in &keywords_per_msg[idx] {
                    *freq.entry(k.as_str()).or_default() += 1;
                }
            }
            let mut sorted: Vec<_> = freq.into_iter().collect();
            sorted.sort_by(|a, b| b.1.cmp(&a.1));
            let top: Vec<String> = sorted.iter().take(5).map(|(k, _)| k.to_string()).collect();
            let label = top.first().cloned().unwrap_or_default();

            TopicCluster {
                label,
                keywords: top,
                message_count: c.indices.len(),
            }
        })
        .collect();

    result.sort_by(|a, b| b.message_count.cmp(&a.message_count));
    result
}

/// Detect retry patterns: consecutive prompts in the same session with high word overlap.
pub fn detect_retries(messages: &[StoredMessage]) -> Vec<RetryPattern> {
    let threshold = 0.30;

    // Group message indices by session, preserving chronological order
    let mut by_session: Vec<(&str, Vec<usize>)> = Vec::new();
    let mut session_order: Vec<&str> = Vec::new();
    let mut session_map: HashMap<&str, usize> = HashMap::new();

    for (i, msg) in messages.iter().enumerate() {
        if let Some(&idx) = session_map.get(msg.session_id.as_str()) {
            by_session[idx].1.push(i);
        } else {
            let idx = by_session.len();
            session_map.insert(&msg.session_id, idx);
            session_order.push(&msg.session_id);
            by_session.push((&msg.session_id, vec![i]));
        }
    }

    let mut patterns: Vec<RetryPattern> = Vec::new();

    for (session_id, indices) in &by_session {
        if indices.len() < 2 {
            continue;
        }

        let keywords: Vec<HashSet<String>> = indices
            .iter()
            .map(|&i| extract_keywords(&messages[i].prompt))
            .collect();

        let mut retry_group: Vec<RetryAttempt> = Vec::new();

        for w in 0..keywords.len() - 1 {
            let sim = jaccard(&keywords[w], &keywords[w + 1]);

            if sim >= threshold {
                if retry_group.is_empty() {
                    retry_group.push(attempt_from(&messages[indices[w]]));
                }
                retry_group.push(attempt_from(&messages[indices[w + 1]]));
            } else if retry_group.len() >= 2 {
                patterns.push(RetryPattern {
                    session_id: session_id.to_string(),
                    attempts: std::mem::take(&mut retry_group),
                });
            } else {
                retry_group.clear();
            }
        }

        if retry_group.len() >= 2 {
            patterns.push(RetryPattern {
                session_id: session_id.to_string(),
                attempts: retry_group,
            });
        }
    }

    patterns
}

fn attempt_from(msg: &StoredMessage) -> RetryAttempt {
    RetryAttempt {
        time: format_msg_time(msg),
        prompt_summary: first_line(&msg.prompt, 100),
    }
}

/// Find sessions across different dates that share a common topic thread.
pub fn find_cross_session_threads(messages: &[StoredMessage]) -> Vec<SessionThread> {
    // Build per-session keyword profiles
    struct Profile {
        session_id: String,
        date: String,
        keywords: HashSet<String>,
        first_prompt: String,
    }

    let mut profiles_map: HashMap<&str, Profile> = HashMap::new();

    for msg in messages {
        let profile =
            profiles_map
                .entry(&msg.session_id)
                .or_insert_with(|| Profile {
                    session_id: msg.session_id.clone(),
                    date: msg.date.clone(),
                    keywords: HashSet::new(),
                    first_prompt: first_line(&msg.prompt, 80),
                });
        for kw in extract_keywords(&msg.prompt) {
            profile.keywords.insert(kw);
        }
    }

    let profiles: Vec<Profile> = profiles_map.into_values().collect();
    if profiles.len() < 2 {
        return Vec::new();
    }

    let mut threads: Vec<SessionThread> = Vec::new();
    let mut used: HashSet<&str> = HashSet::new();
    let threshold = 0.15;

    for i in 0..profiles.len() {
        if used.contains(profiles[i].session_id.as_str()) {
            continue;
        }

        let mut members: Vec<usize> = vec![i];

        for j in (i + 1)..profiles.len() {
            if used.contains(profiles[j].session_id.as_str()) {
                continue;
            }
            // Only link sessions on different dates
            if profiles[i].date == profiles[j].date {
                continue;
            }
            let sim = jaccard(&profiles[i].keywords, &profiles[j].keywords);
            if sim >= threshold {
                members.push(j);
                used.insert(&profiles[j].session_id);
            }
        }

        if members.len() >= 2 {
            used.insert(&profiles[i].session_id);

            // Find common keywords across all thread members
            let mut common = profiles[members[0]].keywords.clone();
            for &mi in &members[1..] {
                common = common
                    .intersection(&profiles[mi].keywords)
                    .cloned()
                    .collect();
            }
            let mut common_sorted: Vec<_> = common.into_iter().collect();
            common_sorted.sort();
            let topic = if common_sorted.is_empty() {
                "related work".to_string()
            } else {
                common_sorted.into_iter().take(3).collect::<Vec<_>>().join(", ")
            };

            let mut sessions: Vec<ThreadedSession> = members
                .iter()
                .map(|&mi| ThreadedSession {
                    session_id: profiles[mi].session_id.clone(),
                    date: profiles[mi].date.clone(),
                    summary: profiles[mi].first_prompt.clone(),
                })
                .collect();
            sessions.sort_by(|a, b| a.date.cmp(&b.date));

            threads.push(SessionThread { topic, sessions });
        }
    }

    threads
}

fn format_msg_time(msg: &StoredMessage) -> String {
    let parts: Vec<&str> = msg.minute_second_ms.splitn(3, '-').collect();
    if parts.len() >= 2 {
        format!("{}:{}:{}", msg.hour, parts[0], parts[1])
    } else {
        format!("{}:{}", msg.hour, msg.minute_second_ms)
    }
}

fn first_line(s: &str, max_len: usize) -> String {
    let line = s.lines().next().unwrap_or(s);
    if line.len() <= max_len {
        line.to_string()
    } else {
        format!("{}...", &line[..max_len])
    }
}
