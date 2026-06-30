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

pub struct StdioTransport;

impl StdioTransport {
    pub fn read_request() -> Result<JsonRpcRequest, io::Error> {
        let stdin = io::stdin();
        let mut handle = stdin.lock();
        let mut line = String::new();
        handle.read_line(&mut line)?;
        if line.trim().is_empty() {
            return Err(io::Error::new(io::ErrorKind::WouldBlock, "empty line"));
        }
        serde_json::from_str(&line).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))
    }

    pub fn send_response(response: &JsonRpcResponse) -> Result<(), io::Error> {
        let mut stdout = io::stdout().lock();
        let json = serde_json::to_string(response).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;
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
}
