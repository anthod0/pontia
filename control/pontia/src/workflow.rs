use std::{
    env, fs,
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
    path::{Path, PathBuf},
    process::Command,
};

use clap::{Args, Subcommand};
use pontia_config::AppConfig;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Args)]
pub(crate) struct WorkflowCommand {
    #[command(subcommand)]
    command: WorkflowCommandKind,
}

#[derive(Debug, Subcommand)]
enum WorkflowCommandKind {
    Run(RunArgs),
    /// Show a compact, agent-readable Workflow context
    Show(ShowArgs),
    Submit(SubmitArgs),
    Patch(PatchArgs),
}

#[derive(Debug, Args)]
struct PatchArgs {
    #[command(subcommand)]
    command: PatchCommandKind,
}

#[derive(Debug, Subcommand)]
enum PatchCommandKind {
    Request(PatchRequestArgs),
}

#[derive(Debug, Args)]
struct PatchRequestArgs {
    #[arg(long, value_name = "REQUEST_FILE")]
    input: PathBuf,
}

#[derive(Debug, Args)]
struct RunArgs {
    #[arg(value_name = "WORKFLOW_FILE")]
    workflow_file: PathBuf,
}

#[derive(Debug, Args)]
struct SubmitArgs {
    #[arg(long, value_name = "PATH")]
    input: String,
    #[arg(long, value_name = "HANDOFF_FILE")]
    output: String,
}

#[derive(Debug, Args)]
struct ShowArgs {
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
    execution_profile_id: Option<String>,
    execution_profile_version: Option<String>,
}

#[derive(Debug, Serialize)]
struct RunWorkflowRequest {
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

#[derive(Debug, Serialize)]
struct WorkflowSubmissionRequest {
    session_id: String,
    runtime_instance_id: String,
    output: String,
    content: String,
}

#[derive(Debug, Serialize)]
struct WorkflowPatchRequest {
    session_id: String,
    runtime_instance_id: String,
    document: String,
}

#[derive(Debug, Deserialize)]
struct WorkflowPatchResponse {
    data: WorkflowPatchResponseData,
}

#[derive(Debug, Deserialize)]
struct WorkflowPatchResponseData {
    patch_id: String,
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
        WorkflowCommandKind::Submit(args) => submit_workflow(args, config).await,
        WorkflowCommandKind::Patch(args) => match args.command {
            PatchCommandKind::Request(args) => request_workflow_patch(args, config).await,
        },
    }
}

async fn run_workflow(args: RunArgs, config: &AppConfig) -> Result<(), String> {
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
        "http://{}/internal/v1/workflows",
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
        "http://{}/external/v1/workflows",
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
        "# {}\nWorkflow: `{}` | State: {} | Progress: {}/{}",
        workflow.title,
        workflow.workflow_id,
        workflow.state,
        workflow.agent_submitted_count,
        workflow.agent_total_count
    );
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

async fn submit_workflow(args: SubmitArgs, config: &AppConfig) -> Result<(), String> {
    let content = fs::read_to_string(&args.input)
        .map_err(|error| format!("failed to read UTF-8 input file {}: {error}", args.input))?;
    let (session_id, runtime_instance_id) = current_managed_pane_identity()?;
    let token = config
        .external_api_token
        .as_deref()
        .ok_or_else(|| "Pontia local API token is not configured".to_string())?;
    let url = format!(
        "http://{}/internal/v1/workflow/submissions",
        local_api_addr(config.bind_addr)
    );
    let response = reqwest::Client::new()
        .post(url)
        .bearer_auth(token)
        .json(&WorkflowSubmissionRequest {
            session_id,
            runtime_instance_id,
            output: args.output,
            content,
        })
        .send()
        .await
        .map_err(|error| format!("failed to submit Workflow output: {error}"))?;
    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(format!(
            "Workflow submission failed with HTTP {status}: {body}"
        ));
    }
    Ok(())
}

async fn request_workflow_patch(args: PatchRequestArgs, config: &AppConfig) -> Result<(), String> {
    let document = fs::read_to_string(&args.input).map_err(|error| {
        format!(
            "failed to read UTF-8 Workflow Patch request file {}: {error}",
            args.input.display()
        )
    })?;
    let (session_id, runtime_instance_id) = current_managed_pane_identity()?;
    let token = config
        .external_api_token
        .as_deref()
        .ok_or_else(|| "Pontia local API token is not configured".to_string())?;
    let url = format!(
        "http://{}/internal/v1/workflow/patches/request",
        local_api_addr(config.bind_addr)
    );
    let response = reqwest::Client::new()
        .post(url)
        .bearer_auth(token)
        .json(&WorkflowPatchRequest {
            session_id,
            runtime_instance_id,
            document,
        })
        .send()
        .await
        .map_err(|error| format!("failed to request Workflow Patch: {error}"))?;
    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(format!(
            "Workflow Patch request failed with HTTP {status}: {body}"
        ));
    }
    let response = response
        .json::<WorkflowPatchResponse>()
        .await
        .map_err(|error| format!("failed to decode Workflow Patch response: {error}"))?;
    println!("{}", response.data.patch_id);
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

fn current_managed_pane_identity() -> Result<(String, String), String> {
    if env::var_os("TMUX").is_none() || env::var_os("TMUX_PANE").is_none() {
        return Err("not running in a Pontia-managed tmux pane".to_string());
    }
    let session_id = pane_option("@pontia_session_id")?;
    let runtime_instance_id = pane_option("@pontia_runtime_instance_id")?;
    Ok((session_id, runtime_instance_id))
}

fn pane_option(option: &str) -> Result<String, String> {
    let pane_id = env::var("TMUX_PANE")
        .map_err(|_| "not running in a Pontia-managed tmux pane".to_string())?;
    let output = Command::new("tmux")
        .args(["show-options", "-p", "-v", "-t", &pane_id, option])
        .output()
        .map_err(|error| format!("failed to inspect current tmux pane: {error}"))?;
    if !output.status.success() {
        return Err("not running in a Pontia-managed tmux pane".to_string());
    }
    let value = String::from_utf8(output.stdout)
        .map_err(|_| "Pontia tmux pane identity is not UTF-8".to_string())?;
    let value = value.trim();
    if value.is_empty() {
        return Err("not running in a Pontia-managed tmux pane".to_string());
    }
    Ok(value.to_string())
}
