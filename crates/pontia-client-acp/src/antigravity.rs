use std::{collections::BTreeSet, fs, path::Path};

use pontia_core::{Error, Result, execution_policy::ExecutionPolicy};

use crate::AcpLaunch;

pub(crate) const DISABLED_TOOLS: &[&str] = &[
    "write_file",
    "edit_file",
    "invoke_subagent",
    "define_subagent",
    "manage_subagents",
    "send_message",
    "schedule",
    "run_workflow",
    "swarm",
    "ask_question",
];

const AGENT: &str = "---
name: pontia-restricted
description: Pontia restricted Session
tools: [read_file, run_command, search_web]
mainAgent: true
subagent: false
---
";

pub(crate) fn validate(policy: &ExecutionPolicy) -> Result<()> {
    if policy == &ExecutionPolicy::default() {
        return Ok(());
    }
    if policy.tools.is_some()
        || policy.disable_mcp
        || policy
            .disabled_tools
            .iter()
            .map(String::as_str)
            .collect::<BTreeSet<_>>()
            != DISABLED_TOOLS.iter().copied().collect()
    {
        return Err(Error::CapabilityUnavailable(
            "Antigravity requires its fixed restricted tool policy".into(),
        ));
    }
    Ok(())
}

pub(crate) fn prepare_launch(
    home: &Path,
    cwd: &Path,
    policy: &ExecutionPolicy,
    launch: &mut AcpLaunch,
) -> Result<()> {
    validate(policy)?;
    crate::restrictions::managed_agent_directory(home, cwd)?;
    crate::restrictions::write_config(&cwd.join(".agents/agents"), "pontia-restricted.md", AGENT)?;
    let data = home.join("clients/antigravity");
    fs::create_dir_all(&data)?;
    launch.args.extend([
        "--gemini-dir".into(),
        data.to_string_lossy().into_owned(),
        "--agent".into(),
        "pontia-restricted".into(),
        "--include-default-customizations=false".into(),
    ]);
    if policy.yolo {
        launch.args.push("--dangerously-skip-permissions".into());
    }
    Ok(())
}
