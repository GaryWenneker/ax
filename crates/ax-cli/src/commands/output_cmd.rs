//! `ax output` — read or set the MCP reply format.

use ax_usage::{load_output_format, parse_output_format, save_output_format};

pub fn run(format: Option<String>) -> Result<(), String> {
    let Some(format) = format else {
        let current = load_output_format();
        println!("{} ({})", current.label(), current.as_str());
        return Ok(());
    };
    let parsed = parse_output_format(&format)?;
    save_output_format(parsed)?;
    println!("MCP output format set to {} ({})", parsed.label(), parsed.as_str());
    Ok(())
}
