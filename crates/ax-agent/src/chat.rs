//! Built-in agent chat runner (LLM streaming).

use ax_reasoning::{resolve_offload, ExploreOffloadMeta};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ChatEvent {
    Token { text: String },
    ToolStart { name: String },
    ToolEnd { name: String, output: String },
    System { text: String },
    Done { session_id: String },
    Error { message: String },
}

pub struct ChatRunner {
    pub session_id: String,
    pub agent_mode: String,
    pub profile_id: Option<String>,
}

impl ChatRunner {
    pub fn new(session_id: impl Into<String>, agent_mode: impl Into<String>) -> Self {
        Self {
            session_id: session_id.into(),
            agent_mode: agent_mode.into(),
            profile_id: None,
        }
    }

    pub fn with_profile(mut self, profile_id: impl Into<String>) -> Self {
        self.profile_id = Some(profile_id.into());
        self
    }

    /// Stream a single-turn reply using ax-reasoning offload config.
    pub async fn stream_reply(
        &self,
        prompt: &str,
        context: &str,
    ) -> Result<String, String> {
        let cfg = resolve_offload();
        if !cfg.enabled {
            return Ok(format!(
                "Built-in agent is ready but no LLM endpoint is configured.\n\n\
                 Your prompt: {prompt}\n\n\
                 Configure an endpoint with:\n\
                 `ax offload set-endpoint https://api.openai.com/v1 --key-env OPENAI_API_KEY`\n\n\
                 Or use an external agent (Cursor / Claude Code) in Settings → AI Agents."
            ));
        }

        let meta = ExploreOffloadMeta {
            source: "agent-terminal",
            project: None,
        };

        let answer = ax_reasoning::synthesize_offload(prompt, context, Some(&meta))
            .await
            .ok_or_else(|| {
                String::from("LLM request failed — check your API key and endpoint in ~/.ax/config.json")
            })?;

        Ok(answer)
    }
}

pub fn chunk_text(text: &str, chunk_size: usize) -> Vec<String> {
    if text.len() <= chunk_size {
        return vec![text.to_string()];
    }
    let mut chunks = Vec::new();
    let mut start = 0;
    while start < text.len() {
        let end = (start + chunk_size).min(text.len());
        // Avoid splitting multibyte chars
        let mut end = end;
        while end > start && !text.is_char_boundary(end) {
            end -= 1;
        }
        if end == start {
            end = (start + chunk_size).min(text.len());
        }
        chunks.push(text[start..end].to_string());
        start = end;
    }
    chunks
}
