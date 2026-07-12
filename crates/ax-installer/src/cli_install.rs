//! Install external agent CLIs (Claude Code, Codex, Cursor, etc.).

use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use crate::cli_catalog::{
    catalog_entry, detect_cli_available, native_install_url, CliCatalogEntry, CliInstallMethod,
};
use crate::targets::display_name;

pub use crate::cli_catalog::{
    auth_command, build_child_env, cli_bin_name, headless_command, is_cli_on_path,
    resolve_cli_path, resolve_cli_spawn, CommandSpec, CliSpawn,
};

#[derive(Debug, Clone)]
pub struct CliInstallPlan {
    pub target: String,
    pub display_name: String,
    pub bin: String,
    pub method: CliInstallMethod,
    pub manual_url: Option<String>,
    pub manual_hint: Option<String>,
}

#[derive(Debug, Clone)]
pub struct CliInstallOutcome {
    pub target: String,
    pub display_name: String,
    pub ok: bool,
    pub already_installed: bool,
    pub message: String,
}

pub fn cli_install_plan(target: &str) -> Option<CliInstallPlan> {
    let entry = catalog_entry(target)?;
    Some(CliInstallPlan {
        target: entry.id.to_string(),
        display_name: entry.display_name.to_string(),
        bin: entry.bin.to_string(),
        method: entry.install.method,
        manual_url: entry.install.manual_url.map(String::from),
        manual_hint: entry.install.manual_hint.map(String::from),
    })
}

pub fn cli_installable(target: &str) -> bool {
    catalog_entry(target).is_some()
}

pub fn install_cli(
    target: &str,
    on_line: &mut impl FnMut(&str),
) -> Result<CliInstallOutcome, String> {
    let plan = cli_install_plan(target).ok_or_else(|| format!("No CLI installer for {target}"))?;

    if detect_cli_available(target) {
        let message = format!(
            "{} (`{}`) is already available.",
            plan.display_name, plan.bin
        );
        return Ok(CliInstallOutcome {
            target: plan.target,
            display_name: plan.display_name,
            ok: true,
            already_installed: true,
            message,
        });
    }

    if plan.method == CliInstallMethod::Manual {
        let hint = plan
            .manual_hint
            .clone()
            .unwrap_or_else(|| "Install manually from the vendor site.".into());
        let url = plan.manual_url.unwrap_or_default();
        return Ok(CliInstallOutcome {
            target: plan.target,
            display_name: plan.display_name,
            ok: false,
            already_installed: false,
            message: format!("{hint} {url}"),
        });
    }

    on_line(&format!("Installing {} CLI…", plan.display_name));
    let entry = catalog_entry(target).ok_or_else(|| format!("No catalog entry for {target}"))?;
    let ok = match plan.method {
        CliInstallMethod::NativeScript => run_native_script(entry, on_line)?,
        CliInstallMethod::NpmGlobal => run_npm_global(entry, on_line)?,
        CliInstallMethod::Winget => run_winget(&plan, on_line)?,
        CliInstallMethod::Scoop => run_scoop(&plan, on_line)?,
        CliInstallMethod::Choco => run_choco(&plan, on_line)?,
        CliInstallMethod::Manual => false,
    };

    let available = detect_cli_available(target);
    let message = if available {
        format!("{} installed — `{}` is ready.", plan.display_name, plan.bin)
    } else if ok {
        format!(
            "{} installer finished — restart ax web, then verify with `{} --version`.",
            plan.display_name, plan.bin
        )
    } else {
        format!(
            "{} CLI install did not complete successfully. Check the log above.",
            plan.display_name
        )
    };

    Ok(CliInstallOutcome {
        target: plan.target,
        display_name: plan.display_name,
        ok: ok || available,
        already_installed: false,
        message,
    })
}

pub fn install_cli_targets(
    targets: &[String],
    on_line: &mut impl FnMut(&str),
) -> Vec<CliInstallOutcome> {
    let mut out = Vec::new();
    for target in targets {
        match install_cli(target, on_line) {
            Ok(result) => {
                on_line(&result.message);
                out.push(result);
            }
            Err(e) => {
                on_line(&e);
                out.push(CliInstallOutcome {
                    target: target.clone(),
                    display_name: display_name(target).to_string(),
                    ok: false,
                    already_installed: false,
                    message: e,
                });
            }
        }
    }
    out
}

/// Install CLI (if missing) and wire ax MCP before using an external agent.
pub fn ensure_agent_ready(
    target: &str,
    project_root: &Path,
    on_line: &mut impl FnMut(&str),
) -> Result<(), String> {
    if target == "builtin" {
        return Ok(());
    }
    let entry = catalog_entry(target).ok_or_else(|| format!("Unknown agent: {target}"))?;
    if !entry.runnable {
        return Err(format!(
            "{} is MCP-only — no headless CLI for the agent terminal",
            entry.display_name
        ));
    }
    if !detect_cli_available(target) {
        let outcome = install_cli(target, on_line)?;
        if !outcome.ok && !detect_cli_available(target) {
            return Err(outcome.message);
        }
    }
    let statuses = crate::targets::agent_status(project_root)?;
    if let Some(s) = statuses.iter().find(|s| s.id == target) {
        if !s.configured {
            on_line(&format!("Wiring ax MCP for {}…", s.display_name));
            crate::targets::install_targets(project_root, &[target.to_string()])?;
        }
    }
    if !detect_cli_available(target) {
        let hint = if target == "cursor" {
            " Manual: irm 'https://cursor.com/install?win32=true' | iex (installs the `agent` CLI, not the IDE)."
        } else {
            ""
        };
        return Err(format!(
            "{} CLI (`{}`) not found after install — restart ax web, then retry.{hint}",
            entry.display_name, entry.bin
        ));
    }
    Ok(())
}

fn run_native_script(
    entry: &CliCatalogEntry,
    on_line: &mut impl FnMut(&str),
) -> Result<bool, String> {
    let url = native_install_url(entry).ok_or_else(|| format!("No native installer for {}", entry.id))?;
    if cfg!(target_os = "windows") {
        on_line(&format!("Running native installer: {url}"));
        on_line("Downloading installer script…");
        let script_name = format!("ax-{}-install.ps1", entry.id);
        let ps_cmd = format!(
            "$ProgressPreference='SilentlyContinue'; \
             $script=Join-Path $env:TEMP '{script_name}'; \
             Invoke-WebRequest -Uri '{url}' -OutFile $script -UseBasicParsing; \
             Write-Output 'Running installer (may take 1–2 minutes)…'; \
             & $script",
            url = url.replace('\'', "''"),
        );
        run_command_with_logs(
            "powershell",
            &[
                "-NoProfile",
                "-NonInteractive",
                "-ExecutionPolicy",
                "Bypass",
                "-Command",
                &ps_cmd,
            ],
            on_line,
            Duration::from_secs(240),
        )
    } else {
        on_line(&format!("Running native installer: {url}"));
        run_command_with_logs(
            "bash",
            &["-c", &format!("curl -fsSL '{url}' | bash")],
            on_line,
            Duration::from_secs(240),
        )
    }
}

fn run_npm_global(
    entry: &CliCatalogEntry,
    on_line: &mut impl FnMut(&str),
) -> Result<bool, String> {
    let package = entry
        .install
        .npm_package
        .ok_or_else(|| format!("No npm package for {}", entry.id))?;
    let npm = find_on_path("npm").ok_or_else(|| {
        "npm not found — install Node.js 22+ from https://nodejs.org/ first.".to_string()
    })?;
    on_line(&format!("npm install -g {package}"));
    run_command_with_logs(
        &npm,
        &["install", "-g", package],
        on_line,
        Duration::from_secs(600),
    )
}

fn run_winget(plan: &CliInstallPlan, on_line: &mut impl FnMut(&str)) -> Result<bool, String> {
    let winget = find_on_path("winget").ok_or_else(|| "winget not found".to_string())?;
    let id = match plan.target.as_str() {
        "cursor" => "Anysphere.Cursor",
        _ => return Err(format!("No winget package for {}", plan.target)),
    };
    on_line(&format!("winget install -e --id {id}"));
    run_command_with_logs(
        &winget,
        &[
            "install",
            "-e",
            "--id",
            id,
            "--accept-package-agreements",
            "--accept-source-agreements",
        ],
        on_line,
        Duration::from_secs(600),
    )
}

fn run_scoop(plan: &CliInstallPlan, on_line: &mut impl FnMut(&str)) -> Result<bool, String> {
    let scoop = find_on_path("scoop").ok_or_else(|| {
        "scoop not found — try: irm get.scoop.sh | iex, or use WSL2 + curl https://opencode.ai/install | bash".to_string()
    })?;
    let pkg = match plan.target.as_str() {
        "opencode" => "opencode",
        _ => return Err(format!("No scoop package for {}", plan.target)),
    };
    on_line(&format!("scoop install {pkg}"));
    run_command_with_logs(&scoop, &["install", pkg], on_line, Duration::from_secs(600))
}

fn run_choco(plan: &CliInstallPlan, on_line: &mut impl FnMut(&str)) -> Result<bool, String> {
    let choco = find_on_path("choco").ok_or_else(|| "choco not found".to_string())?;
    on_line(&format!("choco install {}", plan.target));
    run_command_with_logs(&choco, &["install", &plan.target, "-y"], on_line, Duration::from_secs(600))
}

fn find_on_path(name: &str) -> Option<String> {
    let program = if cfg!(target_os = "windows") {
        "where"
    } else {
        "which"
    };
    let output = Command::new(program).arg(name).output().ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .next()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn run_command_with_logs(
    program: &str,
    args: &[&str],
    on_line: &mut impl FnMut(&str),
    timeout: Duration,
) -> Result<bool, String> {
    on_line(&format!("$ {program} {}", args.join(" ")));
    let mut cmd = Command::new(program);
    cmd.args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let mut child = cmd
        .spawn()
        .map_err(|e| format!("Failed to spawn {program}: {e}"))?;

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let (tx, rx) = mpsc::channel();

    if let Some(out) = stdout {
        let tx_out = tx.clone();
        thread::spawn(move || {
            let reader = BufReader::new(out);
            for line in reader.lines().map_while(Result::ok) {
                let _ = tx_out.send(line);
            }
        });
    }
    if let Some(err) = stderr {
        let tx_err = tx.clone();
        thread::spawn(move || {
            let reader = BufReader::new(err);
            for line in reader.lines().map_while(Result::ok) {
                let _ = tx_err.send(line);
            }
        });
    }
    drop(tx);

    let deadline = Instant::now() + timeout;
    let mut last_output = Instant::now();
    loop {
        while let Ok(line) = rx.try_recv() {
            last_output = Instant::now();
            on_line(&line);
        }
        match child.try_wait() {
            Ok(Some(status)) => {
                while let Ok(line) = rx.try_recv() {
                    on_line(&line);
                }
                return Ok(status.success());
            }
            Ok(None) => {
                if last_output.elapsed() >= Duration::from_secs(8) {
                    on_line("Still installing…");
                    last_output = Instant::now();
                }
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    on_line(&format!(
                        "Install timed out after {}s — try installing manually from Settings → AI Agents",
                        timeout.as_secs()
                    ));
                    return Ok(false);
                }
                thread::sleep(Duration::from_millis(200));
            }
            Err(e) => return Err(e.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn main_targets_have_install_plans() {
        for t in ["claude", "cursor", "codex", "gemini", "opencode"] {
            assert!(cli_install_plan(t).is_some(), "missing plan for {t}");
        }
    }

    #[test]
    fn cli_bin_names() {
        assert_eq!(cli_bin_name("claude"), Some("claude"));
        assert_eq!(cli_bin_name("cursor"), Some("agent"));
        assert_eq!(cli_bin_name("kiro"), Some("kiro-cli"));
    }
}
