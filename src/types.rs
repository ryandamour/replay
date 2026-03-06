use serde::Deserialize;

/// Common fields present in all hook inputs.
#[derive(Debug, Deserialize)]
pub struct HookInput {
    pub session_id: String,
    pub cwd: String,
}

/// Input for the UserPromptSubmit hook event.
#[derive(Debug, Deserialize)]
pub struct UserPromptSubmitInput {
    #[serde(flatten)]
    pub hook: HookInput,
    pub prompt: String,
}

/// Input for the SessionStart hook event.
#[derive(Debug, Deserialize)]
pub struct SessionStartInput {
    #[serde(flatten)]
    pub hook: HookInput,
}

/// A message parsed back from the hive-partitioned file structure.
#[derive(Debug)]
pub struct StoredMessage {
    pub date: String,
    pub hour: String,
    pub session_id: String,
    pub minute_second_ms: String,
    pub prompt: String,
}

impl StoredMessage {
    /// Returns a sort key for chronological ordering: "date/hour/minute_second_ms"
    pub fn sort_key(&self) -> String {
        format!("{}/{}/{}", self.date, self.hour, self.minute_second_ms)
    }
}
