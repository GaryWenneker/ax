//! FnRefCandidate system for function-as-value capture.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaptureMode {
    CallArg,
    FieldAssign,
    StructInit,
    FnTable,
    MethodArg,
    StringCallable,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FnRefCandidate {
    pub name: String,
    pub line: i32,
    pub column: i32,
    pub mode: CaptureMode,
    pub explicit_ref: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skip_gate: Option<bool>,
}
