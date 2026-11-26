pub(crate) mod restricted;
mod steering;
pub(crate) use steering::{Notes, Steers, steer, steering_supported};

use std::{collections::BTreeMap, path::Path, time::Duration};

use pontia_core::{Error, Result};
use serde::Deserialize;
use serde_json::Value;
use tokio::process::Command;

use crate::{AcpLaunch, relay::Observation};

/// The ACP selector gives the model ID. The CLI catalog gives its window in tokens.
pub(crate) async fn model_windows(launch: &AcpLaunch, cwd: &Path) -> Result<BTreeMap<String, u64>> {
    #[derive(Deserialize)]
    struct Catalog {
        models: Vec<Model>,
    }
    #[derive(Deserialize)]
    struct Model {
        model_id: String,
        context_window_tokens: Option<u64>,
    }
    let mut command = Command::new(&launch.command);
    command
        .args(["chat", "--list-models", "--format", "json"])
        .envs(&launch.env)
        .current_dir(cwd)
        .kill_on_drop(true);
    let output = tokio::time::timeout(Duration::from_secs(10), command.output())
        .await
        .map_err(|_| Error::CapabilityUnavailable("Kiro model catalog timed out".into()))??;
    if !output.status.success() {
        return Err(Error::CapabilityUnavailable(format!(
            "Kiro model catalog exited with {}",
            output.status
        )));
    }
    let catalog: Catalog = serde_json::from_slice(&output.stdout)?;
    Ok(catalog
        .models
        .into_iter()
        .filter_map(|model| {
            Some((
                model.model_id,
                model.context_window_tokens.filter(|size| *size > 0)?,
            ))
        })
        .collect())
}

pub(crate) fn info(update: &Value) -> Option<&Value> {
    (update["sessionUpdate"] == "session_info_update")
        .then(|| update.pointer("/_meta/kiro"))
        .flatten()
}

pub(crate) fn usage(context: &Value, info: &Value) -> Option<Observation> {
    let percent = info["usagePercentage"].as_f64()?;
    if info["kind"] != "context_usage" || !(0.0..=100.0).contains(&percent) {
        return None;
    }
    let ratio = percent / 100.0;
    let size = context["current_model"]
        .as_str()
        .and_then(|model| context["model_windows"][model].as_u64());
    Some(Observation::UsageEstimate { ratio, size })
}

pub(crate) fn compaction(
    context: &mut Value,
    frame: &Value,
    automatic: &mut Option<String>,
) -> Option<Observation> {
    let pending = &context["compaction"];
    let manual = pending["operation"].as_str();
    if frame.get("method").is_none() {
        let operation = manual?;
        if frame["id"] != pending["request_id"] {
            return None;
        }
        let observation = if frame["result"]["success"] == true {
            Observation::Compacted {
                operation: operation.into(),
                manual: true,
                native_compaction_id: None,
                tokens_before: None,
                tokens_after: None,
            }
        } else {
            Observation::CompactionFailed {
                operation: operation.into(),
                manual: true,
                aborted: false,
                message: frame["error"]["message"]
                    .as_str()
                    .or_else(|| frame["result"]["error"].as_str())
                    .unwrap_or("Kiro did not confirm compaction")
                    .into(),
            }
        };
        context["compaction"] = Value::Null;
        return Some(observation);
    }

    // Kiro sends the summary notification before answering a manual request.
    // Only its matching response settles that operation.
    let params = &frame["params"];
    if manual.is_some()
        || frame["method"] != "session/update"
        || params["sessionId"] != context["native"]
    {
        return None;
    }
    let info = info(&params["update"])?;
    if info["replay"] == true {
        return None;
    }
    let failed = match info["kind"].as_str()? {
        "summarization_started" if automatic.is_none() => {
            let operation = new_operation();
            *automatic = Some(operation.clone());
            return Some(Observation::CompactionStarted {
                operation,
                manual: false,
            });
        }
        "summarization_completed" => None,
        "summarization_failed" => Some(
            info["summarization"]["error"]
                .as_str()
                .or_else(|| info["error"].as_str())
                .unwrap_or("Kiro reported a compaction error")
                .into(),
        ),
        _ => return None,
    };
    let operation = automatic.take().unwrap_or_else(new_operation);
    Some(match failed {
        Some(message) => Observation::CompactionFailed {
            operation,
            manual: false,
            aborted: false,
            message,
        },
        None => Observation::Compacted {
            operation,
            manual: false,
            native_compaction_id: None,
            tokens_before: None,
            tokens_after: None,
        },
    })
}

fn new_operation() -> String {
    format!("kiro:{}", pontia_core::ids::new_event_id())
}
