//! Measures the `tools/list` payload the default catalog ships every turn.
//!
//! Expanding the default catalog (audit finding C6) trades tokens for
//! discoverability. This test records the actual cost so the trade stays visible
//! instead of drifting silently, and fails if it grows past a ceiling we did not
//! agree to.
//!
//! Run: cargo test -p ax-mcp --test catalog_payload_size -- --nocapture

use ax_mcp::tools::ToolHandler;

/// Rough token estimate. MCP payloads are JSON, which tokenizes at roughly
/// 3.5 characters per token; this is for order-of-magnitude reporting, not
/// billing.
fn approx_tokens(chars: usize) -> usize {
    chars / 4
}

/// One catalog measurement: tool names and the serialized payload size.
async fn measure() -> (Vec<(String, usize)>, usize) {
    let listed = ToolHandler::list_tools(true).await;
    let chars = serde_json::to_string(&listed).expect("serialize").len();
    let tools = listed["tools"]
        .as_array()
        .expect("tools array")
        .iter()
        .map(|t| {
            let name = t["name"].as_str().unwrap_or("?").to_string();
            let size = serde_json::to_string(t).map(|s| s.len()).unwrap_or(0);
            (name, size)
        })
        .collect();
    (tools, chars)
}

/// Both catalogs are measured in one test on purpose: `list_tools` reads the
/// process-wide `AX_MCP_TOOLS`, so two tests setting it would race inside the
/// shared test binary and each could report the other's catalog.
#[tokio::test]
async fn catalog_payload_is_recorded_and_bounded() {
    std::env::remove_var("AX_MCP_TOOLS");
    let (default_tools, default_chars) = measure().await;

    std::env::set_var("AX_MCP_TOOLS", "all");
    let (full_tools, full_chars) = measure().await;
    std::env::remove_var("AX_MCP_TOOLS");

    println!(
        "default catalog: {} tools, {default_chars} chars, ~{} tokens",
        default_tools.len(),
        approx_tokens(default_chars)
    );
    for (name, size) in &default_tools {
        println!("  {name}: {size} chars");
    }
    println!(
        "full catalog:    {} tools, {full_chars} chars, ~{} tokens",
        full_tools.len(),
        approx_tokens(full_chars)
    );

    let gated: Vec<&str> = full_tools
        .iter()
        .map(|(n, _)| n.as_str())
        .filter(|n| !default_tools.iter().any(|(d, _)| d == n))
        .collect();
    println!("gated by default: {}", gated.join(", "));

    // The default must still gate something, or the lean filter is a no-op and
    // the "heavy ops stay opt-in" promise is not being kept.
    assert!(
        full_tools.len() > default_tools.len(),
        "AX_MCP_TOOLS=all listed {} tools, same as the default {} — nothing is gated",
        full_tools.len(),
        default_tools.len()
    );

    // Ceiling, not a target. Well clear of today's value so ordinary edits do
    // not trip it, but low enough that adding a dozen tools has to be a decision.
    const MAX_CHARS: usize = 24_000;
    assert!(
        default_chars <= MAX_CHARS,
        "default tools/list payload is {default_chars} chars (~{} tokens), over the \
         {MAX_CHARS} ceiling. This ships on every turn — either trim a description or \
         raise the ceiling deliberately.",
        approx_tokens(default_chars)
    );
}
