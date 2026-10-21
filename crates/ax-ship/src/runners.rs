//! Execute impacted tests via per-language runner hints (cargo / pytest / jest / go).

use std::collections::HashMap;
use std::process::Command;

use ax_remote::ShipConfig;
use ax_tia::TiaResult;

/// Run impacted tests grouped by detected runner. Returns true when all succeed
/// (or when there are no tests).
pub fn run_impacted_tests(config: &ShipConfig, tia: &TiaResult) -> bool {
    if tia.tests.is_empty() {
        return true;
    }

    let mut by_runner: HashMap<String, Vec<&str>> = HashMap::new();
    for t in &tia.tests {
        let key = runner_family(
            &t.runner_hint,
            &t.file_path,
            &config.quality_gate.tests.runner,
        );
        by_runner.entry(key).or_default().push(t.name.as_str());
    }

    let mut ok = true;
    for (family, names) in by_runner {
        if !run_family(&family, &names, config) {
            ok = false;
        }
    }
    ok
}

fn runner_family(hint: &str, file_path: &str, configured: &str) -> String {
    let h = hint.to_ascii_lowercase();
    let p = file_path.to_ascii_lowercase();
    if h.starts_with("cargo ") || p.ends_with(".rs") {
        "cargo".into()
    } else if h.starts_with("pytest") || p.ends_with(".py") {
        "pytest".into()
    } else if h.contains("jest")
        || h.contains("vitest")
        || p.ends_with(".ts")
        || p.ends_with(".tsx")
        || p.ends_with(".js")
        || p.ends_with(".jsx")
    {
        if h.contains("jest") || configured.to_ascii_lowercase().contains("jest") {
            "jest".into()
        } else {
            "vitest".into()
        }
    } else if h.starts_with("go test") || p.ends_with("_test.go") || p.ends_with(".go") {
        "go".into()
    } else {
        "configured".into()
    }
}

fn run_family(family: &str, names: &[&str], config: &ShipConfig) -> bool {
    if names.is_empty() {
        return true;
    }
    let status = family_command(family, names, config).status();
    matches!(status, Ok(status) if status.success())
}

fn family_command(family: &str, names: &[&str], config: &ShipConfig) -> Command {
    let filter = names.join("|");
    match family {
        "cargo" => {
            let mut command = Command::new("cargo");
            command.arg("test");
            for name in names {
                command.arg(*name);
            }
            command.args(["--", "--exact"]);
            command
        }
        "pytest" => {
            let mut command = Command::new("pytest");
            command.arg("-k").arg(names.join(" or "));
            command
        }
        "jest" => {
            let mut command = Command::new("npx");
            command.args(["jest", "-t", &filter]);
            command
        }
        "vitest" => {
            let mut command = Command::new("npx");
            command.args(["vitest", "run", "-t", &filter]);
            command
        }
        "go" => {
            let mut command = Command::new("go");
            command.args(["test", "./...", "-run", &filter]);
            command
        }
        _ => configured_command(&config.quality_gate.tests.runner, &filter),
    }
}

fn configured_command(runner: &str, filter: &str) -> Command {
    let mut parts = runner.split_whitespace();
    let program = parts.next().unwrap_or("cargo");
    let mut command = Command::new(program);
    for arg in parts {
        command.arg(arg);
    }
    command.arg("--").arg(filter).arg("--").arg("--exact");
    command
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_rust_hint() {
        assert_eq!(
            runner_family("cargo test foo -- --exact", "src/lib.rs", "cargo test"),
            "cargo"
        );
    }

    #[test]
    fn classifies_pytest() {
        assert_eq!(
            runner_family("pytest -k test_x", "tests/test_x.py", "cargo test"),
            "pytest"
        );
    }

    #[test]
    fn pytest_filter_stays_one_argument() {
        let hostile = "ok; touch /tmp/ax-ship-pwned";
        let command = family_command("pytest", &[hostile], &ShipConfig::default());
        let program = command.get_program().to_string_lossy();
        assert_eq!(program, "pytest");
        let args: Vec<_> = command
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect();
        assert_eq!(args, vec!["-k", hostile]);
    }
}
