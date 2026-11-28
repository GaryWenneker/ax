//! Replay reference, run, and status (Plan B interface surface).

use std::path::PathBuf;

pub async fn reference(body: String) -> Result<(), String> {
    let value: serde_json::Value =
        serde_json::from_str(&body).map_err(|error| error.to_string())?;
    let parsed = ax_usage::context_authority::parse_replay_reference(&value)?;
    let out = serde_json::json!({
        "ok": true,
        "sessionId": parsed.session_id,
        "turnId": parsed.turn_id,
        "agentId": parsed.agent_id,
        "providerChatId": parsed.provider_chat_id,
    });
    finish_json(out)
}

pub async fn run(manifest_id: String) -> Result<(), String> {
    let root = project_root()?;
    let database = ax_db::Database::open(&root.join(".ax").join("ax.db"))
        .await
        .map_err(|error| error.to_string())?;
    match ax_usage::context_authority::start_replay_for_manifest(database.pool(), &manifest_id)
        .await
    {
        Ok(receipt) => finish_json(serde_json::json!({
            "ok": true,
            "run": receipt,
        })),
        Err(error) => finish_json(serde_json::json!({ "ok": false, "error": error })),
    }
}

pub async fn status(run_id: String) -> Result<(), String> {
    let root = project_root()?;
    let database = ax_db::Database::open(&root.join(".ax").join("ax.db"))
        .await
        .map_err(|error| error.to_string())?;
    match ax_usage::context_authority::load_replay_run(database.pool(), &run_id).await {
        Ok(run) => finish_json(serde_json::json!({ "ok": true, "run": run })),
        Err(error) => finish_json(serde_json::json!({ "ok": false, "error": error })),
    }
}

/// Create Plan C pilot arm dirs (G/A/B/C) and write frozen-task.json. Does not claim 95%.
pub fn pilot_prepare(dir: String) -> Result<(), String> {
    let parent = PathBuf::from(dir);
    std::fs::create_dir_all(&parent).map_err(|error| error.to_string())?;
    let bundle = ax_usage::replay_validation::prepare_pilot_bundle(&parent)?;
    finish_json(serde_json::json!({
        "ok": true,
        "taskId": bundle.task.task_id,
        "benchmarkVersion": bundle.task.benchmark_version,
        "definitionSeal": bundle.task.definition_seal,
        "taskPath": bundle.task_path,
        "arms": bundle.arms.iter().map(|arm| serde_json::json!({
            "arm": arm.arm,
            "root": arm.root,
        })).collect::<Vec<_>>(),
        "adapters": bundle.adapters,
        "releaseGate": "not claimed",
    }))
}

/// Score a pilot bundle with the fixture adapter only. Never claims a release gate.
pub fn pilot_score(dir: String) -> Result<(), String> {
    let parent = PathBuf::from(dir);
    std::fs::create_dir_all(&parent).map_err(|error| error.to_string())?;
    let report = ax_usage::replay_validation::score_fixture_pilot(&parent)?;
    finish_json(serde_json::json!({
        "ok": true,
        "taskId": report.task_id,
        "adapter": report.adapter,
        "liveAgentRuns": report.live_agent_runs,
        "releaseGate": report.release_gate,
        "arms": report.arms.iter().map(|arm| serde_json::json!({
            "arm": arm.arm,
            "runId": arm.run_id,
            "evidencePath": arm.evidence_path,
            "evidenceSeal": arm.evidence_seal,
            "verdict": arm.verdict,
            "functionalPercent": arm.functional_percent,
        })).collect::<Vec<_>>(),
    }))
}

fn finish_json(result: serde_json::Value) -> Result<(), String> {
    println!(
        "{}",
        serde_json::to_string(&result).map_err(|error| error.to_string())?
    );
    if result.get("ok").and_then(|ok| ok.as_bool()) == Some(true) {
        Ok(())
    } else {
        Err(result
            .get("error")
            .and_then(|error| error.as_str())
            .unwrap_or("request failed")
            .to_string())
    }
}

fn project_root() -> Result<PathBuf, String> {
    let cwd = std::env::current_dir().map_err(|error| error.to_string())?;
    super::find_project_root(&cwd).ok_or_else(|| "no ax project database found".into())
}
