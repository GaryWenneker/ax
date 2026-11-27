//! JSON-RPC transport over stdio.

use std::io::{self, BufRead, Write};

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    #[serde(default)]
    pub id: Option<Value>,
    pub method: String,
    pub params: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

pub const PARSE_ERROR: i32 = -32700;
pub const METHOD_NOT_FOUND: i32 = -32601;
pub const INVALID_PARAMS: i32 = -32602;
pub const INTERNAL_ERROR: i32 = -32603;

/// JSON-RPC notifications omit `id` and must not receive a response (MCP spec).
pub fn is_notification(id: &Option<Value>) -> bool {
    id.is_none()
}

/// Read one JSON-RPC request line from `reader`, skipping blank lines.
///
/// Returns an `UnexpectedEof` error once the input is closed, so the caller
/// can stop instead of reading a closed stream forever. A line that is not
/// valid JSON-RPC is an `InvalidData` error.
pub fn read_request_from<R: BufRead>(reader: &mut R) -> Result<JsonRpcRequest, io::Error> {
    let mut line = String::new();
    loop {
        line.clear();
        if reader.read_line(&mut line)? == 0 {
            return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "stdin closed"));
        }
        if !line.trim().is_empty() {
            break;
        }
    }
    serde_json::from_str(&line)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))
}

pub struct StdioTransport;

impl StdioTransport {
    pub fn read_request() -> Result<JsonRpcRequest, io::Error> {
        let stdin = io::stdin();
        let mut handle = stdin.lock();
        read_request_from(&mut handle)
    }

    pub fn send_response(response: &JsonRpcResponse) -> Result<(), io::Error> {
        let mut stdout = io::stdout().lock();
        let json = serde_json::to_string(response)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;
        writeln!(stdout, "{}", json)?;
        stdout.flush()?;
        Ok(())
    }

    pub fn send_result(id: Value, result: Value) -> Result<(), io::Error> {
        Self::send_response(&JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id: Some(id),
            result: Some(result),
            error: None,
        })
    }

    pub fn send_error(id: Option<Value>, code: i32, message: &str) -> Result<(), io::Error> {
        Self::send_response(&JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id,
            result: None,
            error: Some(JsonRpcError {
                code,
                message: message.to_string(),
                data: None,
            }),
        })
    }

    /// Write a pre-formatted JSON-RPC notification line (already ends with `\n`).
    pub fn send_notification_line(line: &str) -> Result<(), io::Error> {
        let mut stdout = io::stdout().lock();
        stdout.write_all(line.as_bytes())?;
        stdout.flush()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn read_request_reports_end_of_input() {
        let mut empty = Cursor::new(Vec::<u8>::new());
        let err = read_request_from(&mut empty).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::UnexpectedEof);
    }

    #[test]
    fn read_request_skips_blank_lines_and_then_reports_end_of_input() {
        let mut input =
            Cursor::new(b"\n  \n{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"ping\"}\n\n".to_vec());
        let req = read_request_from(&mut input).unwrap();
        assert_eq!(req.method, "ping");
        let err = read_request_from(&mut input).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::UnexpectedEof);
    }
}
