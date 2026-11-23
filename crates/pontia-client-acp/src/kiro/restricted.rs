use pontia_core::{Error, Result, execution_policy::ExecutionPolicy};

use crate::restrictions::SessionPolicy;

const TOOLS: &[&str] = &["read", "shell", "web"];

pub(crate) fn policy() -> ExecutionPolicy {
    ExecutionPolicy {
        tools: Some(TOOLS.iter().map(|tool| (*tool).into()).collect()),
        disable_mcp: true,
        disable_skills: true,
        yolo: true,
        ..Default::default()
    }
}

pub(crate) fn resolve(requested: &ExecutionPolicy) -> Result<SessionPolicy> {
    if requested == &ExecutionPolicy::default() {
        return Ok(SessionPolicy::default());
    }
    if requested != &policy() {
        return Err(Error::CapabilityUnavailable(
            "Kiro requires its fixed restricted agent policy".into(),
        ));
    }
    Ok(SessionPolicy {
        meta: serde_json::Map::from_iter([(
            "kiro".into(),
            serde_json::json!({"modeId":"pontia-restricted"}),
        )]),
        profile: None,
    })
}

pub(crate) fn prepare(home: &std::path::Path, cwd: &std::path::Path) -> Result<()> {
    crate::restrictions::managed_agent_directory(home, cwd)?;
    crate::restrictions::write_config(
        &cwd.join(".kiro/settings"),
        "cli.json",
        r#"{"chat.disableInheritingDefaultResources":true}"#,
    )?;
    crate::restrictions::write_config(
        &cwd.join(".kiro/agents"),
        "pontia-restricted.json",
        include_str!("restricted.json"),
    )
}
