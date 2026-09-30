//! Minimal Server-Sent Events parser for graph / MCP / ship streams.

use std::io::{BufRead, BufReader, Read};

use anyhow::Result;

#[derive(Debug, Clone)]
pub struct SseEvent {
    pub event: String,
    pub data: String,
}

/// Blocking SSE consumer over a readable stream (reqwest response bytes).
pub fn consume_sse_blocking<R: Read, F>(reader: R, mut on_event: F) -> Result<()>
where
    F: FnMut(SseEvent) -> bool,
{
    let mut buffered = BufReader::new(reader);
    let mut event = String::from("message");
    let mut data_lines: Vec<String> = Vec::new();
    let mut line = String::new();

    loop {
        line.clear();
        let n = buffered.read_line(&mut line)?;
        if n == 0 {
            if !data_lines.is_empty() {
                let ev = SseEvent {
                    event: std::mem::replace(&mut event, "message".into()),
                    data: data_lines.join("\n"),
                };
                data_lines.clear();
                if !on_event(ev) {
                    break;
                }
            }
            break;
        }
        let trimmed = line.trim_end_matches(['\r', '\n']);
        if trimmed.is_empty() {
            if !data_lines.is_empty() {
                let ev = SseEvent {
                    event: std::mem::replace(&mut event, "message".into()),
                    data: data_lines.join("\n"),
                };
                data_lines.clear();
                if !on_event(ev) {
                    return Ok(());
                }
            }
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("event:") {
            event = rest.trim().to_string();
        } else if let Some(rest) = trimmed.strip_prefix("data:") {
            data_lines.push(rest.trim_start().to_string());
        }
    }
    Ok(())
}
