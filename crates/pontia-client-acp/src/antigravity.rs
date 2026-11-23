use std::{collections::BTreeSet, fs, io::Write, path::Path};

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
    if cwd.canonicalize()?.parent() != Some(home.join("bot").canonicalize()?.as_path()) {
        return Err(Error::Domain(
            "Restricted Antigravity Sessions require a Pontia-managed Bot directory".into(),
        ));
    }
    let directory = cwd.join(".agents/agents");
    fs::create_dir_all(&directory)?;
    let path = directory.join("pontia-restricted.md");
    let current = match fs::read(&path) {
        Ok(contents) => contents == AGENT.as_bytes(),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
        Err(error) => return Err(error.into()),
    };
    if !current {
        let mut file = tempfile::NamedTempFile::new_in(&directory)?;
        file.write_all(AGENT.as_bytes())?;
        file.persist(&path).map_err(|error| error.error)?;
    }
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
