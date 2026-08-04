use std::{
    env, fs,
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
    path::{Path, PathBuf},
    process::Command,
};

use clap::{Args, Parser, Subcommand};
use pontia_config::AppConfig;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Parser)]
#[command(
    name = "pontiactl",
    version,
    about = "Control Pontia from the command line"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<CommandKind>,
}

#[derive(Debug, Subcommand)]
enum CommandKind {
    Workflow(WorkflowCommand),
}

#[derive(Debug, Args)]
struct WorkflowCommand {
    #[command(subcommand)]
    command: WorkflowCommandKind,
}

#[derive(Debug, Subcommand)]
enum WorkflowCommandKind {
    Run(RunArgs),
    Submit(SubmitArgs),
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

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    let result = match cli.command {
        None => Ok(()),
        Some(CommandKind::Workflow(workflow)) => match workflow.command {
            WorkflowCommandKind::Run(args) => run_workflow(args).await,
            WorkflowCommandKind::Submit(args) => submit_workflow(args).await,
        },
    };
    if let Err(error) = result {
        eprintln!("pontiactl: {error}");
        std::process::exit(1);
    }
}

async fn run_workflow(args: RunArgs) -> Result<(), String> {
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
    let config = AppConfig::from_env().map_err(|error| error.to_string())?;
    let token = config
        .external_api_token
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

async fn submit_workflow(args: SubmitArgs) -> Result<(), String> {
    let content = fs::read_to_string(&args.input)
        .map_err(|error| format!("failed to read UTF-8 input file {}: {error}", args.input))?;
    let (session_id, runtime_instance_id) = current_managed_pane_identity()?;
    let config = AppConfig::from_env().map_err(|error| error.to_string())?;
    let token = config
        .external_api_token
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
