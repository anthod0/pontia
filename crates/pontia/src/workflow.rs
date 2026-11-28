use std::{
    env, fs,
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
    path::{Path, PathBuf},
};

use clap::{Args, Subcommand};
use pontia_config::AppConfig;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

const WORKFLOW_HELP: &str = "\
Requires a running service.
Start a Workflow from a Pontia Session. Submit output or report a blocking problem from its Agent Node Session.

For Workflows, see https://pontia.dev/docs/workflows.md";

const RUN_HELP: &str = "\
Prints the Workflow ID. Paths are relative to the definition file.

Example definition:
  title = \"Implement search\"
  cwd = \".\"

  [[handoffs]]
  name = \"requirements.md\"
  source = \"./requirements.md\"

  [[nodes]]
  type = \"agent\"
  phase = \"Build\"
  title = \"Implement\"
  instructions = \"Implement and verify the requirements.\"
  execution_profile_id = \"coder\"
  inputs = [\"requirements.md\"]
  output = \"result.md\"

For the definition format, see https://pontia.dev/docs/workflows.md#create-a-definition";

#[derive(Debug, Args)]
#[command(after_help = WORKFLOW_HELP)]
pub(crate) struct WorkflowCommand {
    #[command(subcommand)]
    command: WorkflowCommandKind,
}

#[derive(Debug, Subcommand)]
enum WorkflowCommandKind {
    /// Start a Workflow from a TOML definition
    #[command(after_help = RUN_HELP)]
    Run(RunArgs),
    /// Show the Workflow context and current Node as Markdown
    Show(ShowArgs),
    /// Submit the current Agent Node's output
    Submit,
    /// Report a blocking problem to the initiating Session
    Terminate,
    /// Retry a failed Node in its original Session
    Retry {
        workflow_id: String,
        failure_event_id: String,
    },
}

#[derive(Debug, Args)]
struct RunArgs {
    /// Workflow definition TOML file
    #[arg(value_name = "WORKFLOW_FILE")]
    workflow_file: PathBuf,
}

#[derive(Debug, Args)]
struct ShowArgs {
    /// Workflow ID [default: $PONTIA_WORKFLOW_ID]
    #[arg(value_name = "WORKFLOW_ID")]
    workflow_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct WorkflowDefinition {
    title: String,
    cwd: PathBuf,
    #[serde(default)]
    handoffs: Vec<HandoffDefinition>,
    nodes: Vec<NodeDefinition>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct HandoffDefinition {
    name: String,
    source: PathBuf,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct NodeDefinition {
    #[serde(rename = "type")]
    node_type: String,
    phase: String,
    title: String,
    instructions: String,
    #[serde(default)]
    inputs: Vec<String>,
    output: String,
    execution_profile_id: String,
}

#[derive(Debug, Serialize)]
struct RunWorkflowRequest {
    session_id: String,
    runtime_id: String,
    workflow_id: String,
    title: String,
    cwd: String,
    handoffs: Vec<InitialHandoff>,
    nodes: Vec<NodeDefinition>,
}

#[derive(Debug, Serialize)]
struct InitialHandoff {
    name: String,
    content: String,
}

#[derive(Debug, Deserialize)]
struct RunWorkflowResponse {
    data: RunWorkflowResponseData,
}

#[derive(Debug, Deserialize)]
struct RunWorkflowResponseData {
    workflow_id: String,
}

#[derive(Debug, Deserialize)]
struct WorkflowContextResponse {
    data: WorkflowContextResponseData,
}

#[derive(Debug, Deserialize)]
struct WorkflowContextResponseData {
    context: WorkflowContext,
}

#[derive(Debug, Deserialize)]
struct WorkflowContext {
    workflow: WorkflowContextSummary,
    current_node: WorkflowNodeContext,
}

#[derive(Debug, Deserialize)]
struct WorkflowContextSummary {
    workflow_id: String,
    title: String,
    state: String,
    initiator_session_id: String,
    termination_reason: Option<String>,
    retry_failure_event_id: Option<String>,
    failure_message: Option<String>,
    agent_submitted_count: usize,
    agent_total_count: usize,
    current_node_id: Option<String>,
    nodes: Vec<WorkflowNodeSummary>,
}

#[derive(Debug, Deserialize)]
struct WorkflowNodeSummary {
    node_id: String,
    phase: String,
    title: String,
    status: String,
}

#[derive(Debug, Deserialize)]
struct WorkflowNodeContext {
    instructions: String,
    inputs: Vec<WorkflowInput>,
    output: String,
}

#[derive(Debug, Deserialize)]
struct WorkflowInput {
    name: String,
    content: Option<String>,
}

pub(crate) async fn run(workflow: WorkflowCommand, config: &AppConfig) -> Result<(), String> {
    match workflow.command {
        WorkflowCommandKind::Run(args) => run_workflow(args, config).await,
        WorkflowCommandKind::Show(args) => show_workflow(args, config).await,
        WorkflowCommandKind::Submit => node_command(config, "submissions").await,
        WorkflowCommandKind::Terminate => node_command(config, "terminate").await,
        WorkflowCommandKind::Retry {
            workflow_id,
            failure_event_id,
        } => {
            post_command(
                config,
                &format!("workflows/{workflow_id}/retry"),
                serde_json::json!({ "failure_event_id":failure_event_id }),
            )
            .await
        }
    }
}

async fn run_workflow(args: RunArgs, config: &AppConfig) -> Result<(), String> {
    let (session_id, runtime_id) = current_session_identity()?;
    let definition_path = args.workflow_file.canonicalize().map_err(|error| {
        format!(
            "failed to resolve Workflow file {}: {error}",
            args.workflow_file.display()
        )
    })?;
    let definition_dir = definition_path.parent().ok_or_else(|| {
        format!(
            "Workflow file {} has no parent directory",
            definition_path.display()
        )
    })?;
    let source = fs::read_to_string(&definition_path).map_err(|error| {
        format!(
            "failed to read UTF-8 Workflow file {}: {error}",
            definition_path.display()
        )
    })?;
    let definition: WorkflowDefinition = toml::from_str(&source).map_err(|error| {
        format!(
            "failed to parse Workflow file {}: {error}",
            definition_path.display()
        )
    })?;
    let cwd = resolve_existing_path(definition_dir, &definition.cwd, "Workflow cwd")?;
    let handoffs = definition
        .handoffs
        .into_iter()
        .map(|handoff| {
            let source = resolve_existing_path(definition_dir, &handoff.source, "Handoff source")?;
            let content = fs::read_to_string(&source).map_err(|error| {
                format!(
                    "failed to read UTF-8 Handoff source {}: {error}",
                    source.display()
                )
            })?;
            Ok(InitialHandoff {
                name: handoff.name,
                content,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let request = RunWorkflowRequest {
        session_id,
        runtime_id,
        workflow_id: format!("wf_{}", Uuid::now_v7()),
        title: definition.title,
        cwd: cwd.display().to_string(),
        handoffs,
        nodes: definition.nodes,
    };
    let token = config
        .external_api_token
        .as_deref()
        .ok_or_else(|| "Pontia local API token is not configured".to_string())?;
    let url = format!(
        "http://{}/api/v1/workflows",
        local_api_addr(config.bind_addr)
    );
    let response = reqwest::Client::new()
        .post(url)
        .bearer_auth(token)
        .json(&request)
        .send()
        .await
        .map_err(|error| format!("failed to run Workflow: {error}"))?;
    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(format!("Workflow run failed with HTTP {status}: {body}"));
    }
    let response = response
        .json::<RunWorkflowResponse>()
        .await
        .map_err(|error| format!("failed to decode Workflow run response: {error}"))?;
    println!("{}", response.data.workflow_id);
    Ok(())
}

async fn show_workflow(args: ShowArgs, config: &AppConfig) -> Result<(), String> {
    let workflow_id = args
        .workflow_id
        .or_else(|| {
            env::var("PONTIA_WORKFLOW_ID")
                .ok()
                .filter(|value| !value.trim().is_empty())
        })
        .ok_or_else(|| "WORKFLOW_ID is required when PONTIA_WORKFLOW_ID is not set".to_string())?;
    let token = config
        .external_api_token
        .as_deref()
        .ok_or_else(|| "Pontia local API token is not configured".to_string())?;
    let base = format!(
        "http://{}/api/v1/workflows",
        local_api_addr(config.bind_addr)
    );
    let mut url = reqwest::Url::parse(&base)
        .map_err(|error| format!("failed to build Workflow URL: {error}"))?;
    url.path_segments_mut()
        .map_err(|_| "failed to build Workflow URL".to_string())?
        .push(&workflow_id)
        .push("context");
    let response = reqwest::Client::new()
        .get(url)
        .bearer_auth(token)
        .send()
        .await
        .map_err(|error| format!("failed to show Workflow: {error}"))?;
    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(format!("Workflow show failed with HTTP {status}: {body}"));
    }
    let response = response
        .json::<WorkflowContextResponse>()
        .await
        .map_err(|error| format!("failed to decode Workflow context response: {error}"))?;
    println!("{}", render_workflow_context(&response.data.context));
    Ok(())
}

fn render_workflow_context(context: &WorkflowContext) -> String {
    let workflow = &context.workflow;
    let current = workflow
        .current_node_id
        .as_deref()
        .and_then(|id| workflow.nodes.iter().find(|node| node.node_id == id));
    let mut document = format!(
        "# {}\nWorkflow: `{}` | State: {} | Progress: {}/{} | Initiating Session: {}",
        workflow.title,
        workflow.workflow_id,
        workflow.state,
        workflow.agent_submitted_count,
        workflow.agent_total_count,
        workflow.initiator_session_id,
    );
    if let Some(reason) = &workflow.termination_reason {
        document.push_str(&format!("\nTermination reason: {reason}"));
    }
    if let Some(event_id) = &workflow.retry_failure_event_id {
        document.push_str(&format!(
            "\nRetry: pontia workflow retry {} {event_id}",
            workflow.workflow_id
        ));
    }
    if let Some(message) = workflow.failure_message.as_deref() {
        document.push_str(&format!("\nFailure: {message}"));
    }
    if let Some(node) = current {
        document.push_str(&format!(
            "\n\n## Current node: {} — {}\nStatus: {} | Output: `{}`",
            node.phase, node.title, node.status, context.current_node.output
        ));
    }
    document.push_str("\n\n### Instructions\n");
    document.push_str(context.current_node.instructions.trim());
    document.push_str("\n\n### Inputs");
    if context.current_node.inputs.is_empty() {
        document.push_str("\nNone.");
    } else {
        for input in &context.current_node.inputs {
            document.push_str(&format!("\n\n#### `{}`\n", input.name));
            document.push_str(input.content.as_deref().unwrap_or("Unavailable.").trim());
        }
    }
    document.push_str("\n\n## Progress");
    for node in &workflow.nodes {
        let marker = if workflow.current_node_id.as_deref() == Some(node.node_id.as_str()) {
            "→"
        } else {
            match node.status.as_str() {
                "submitted" => "✓",
                "failed" => "✗",
                _ => "·",
            }
        };
        document.push_str(&format!(
            "\n- {marker} {} — {} ({})",
            node.phase, node.title, node.status
        ));
    }
    document
}

fn resolve_existing_path(base: &Path, path: &Path, description: &str) -> Result<PathBuf, String> {
    let path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        base.join(path)
    };
    path.canonicalize().map_err(|error| {
        format!(
            "failed to resolve {description} {}: {error}",
            path.display()
        )
    })
}

async fn node_command(config: &AppConfig, command: &str) -> Result<(), String> {
    let (session_id, runtime_id) = current_session_identity()?;
    post_command(
        config,
        &format!("workflow/{command}"),
        serde_json::json!({"session_id": session_id,"runtime_id":runtime_id}),
    )
    .await
}

async fn post_command(
    config: &AppConfig,
    path: &str,
    body: serde_json::Value,
) -> Result<(), String> {
    let token = config
        .external_api_token
        .as_deref()
        .ok_or("Pontia local API token is not configured")?;
    let url = format!("http://{}/api/v1/{path}", local_api_addr(config.bind_addr));
    let response = reqwest::Client::new()
        .post(url)
        .bearer_auth(token)
        .json(&body)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let status = response.status();
    if !status.is_success() {
        return Err(format!(
            "Workflow command failed with HTTP {status}: {}",
            response.text().await.unwrap_or_default()
        ));
    }
    Ok(())
}

fn local_api_addr(bind_addr: SocketAddr) -> SocketAddr {
    let ip = if bind_addr.ip().is_unspecified() {
        match bind_addr.ip() {
            IpAddr::V4(_) => IpAddr::V4(Ipv4Addr::LOCALHOST),
            IpAddr::V6(_) => IpAddr::V6(Ipv6Addr::LOCALHOST),
        }
    } else {
        bind_addr.ip()
    };
    SocketAddr::new(ip, bind_addr.port())
}

fn current_session_identity() -> Result<(String, String), String> {
    let read = |name: &str| {
        env::var(name)
            .ok()
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| format!("Missing {name}; run this command inside a Pontia Session"))
    };
    Ok((
        read("PONTIA_SESSION_ID")?,
        read("PONTIA_RUNTIME_INSTANCE_ID")?,
    ))
}
