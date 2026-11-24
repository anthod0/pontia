use crate::AcpLaunch;
use pontia_agent_control::client_contract::LaunchFile;
use std::path::Path;

pub(crate) const FILES: &[LaunchFile] = &[LaunchFile {
    path: ".agents/agents/pontia-restricted.md",
    contents: "---
name: pontia-restricted
description: Pontia restricted Session
tools: [read_file, run_command]
inheritMcp: false
inheritCustomizations: false
mainAgent: true
subagent: false
---
",
}];

pub(crate) fn configure_launch(home: &Path, launch: &mut AcpLaunch) {
    let data = home.join("clients/antigravity");
    launch.args.extend([
        "--gemini-dir".into(),
        data.to_string_lossy().into_owned(),
        "--agent".into(),
        "pontia-restricted".into(),
        "--include-default-customizations=false".into(),
        "--dangerously-skip-permissions".into(),
    ]);
}
