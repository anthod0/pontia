use std::{collections::BTreeMap, sync::Arc};

use crate::restrictions::{AgentProfile, RestrictedExecution};
use pontia_core::client_capabilities::AgentClientCapabilities;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AcpLaunch {
    pub command: String,
    pub args: Vec<String>,
    pub env: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct InterjectionProtocol {
    pub method: &'static str,
    pub echo: &'static str,
    pub queue: &'static str,
    pub terminal: &'static str,
    pub promoted_prefix: &'static str,
    pub interjection_marker: &'static str,
    pub display_text: &'static str,
    pub prompt_index: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CompactionProtocol {
    Grok,
    Kiro,
}

impl CompactionProtocol {
    pub(crate) fn method(self) -> &'static str {
        match self {
            Self::Grok => "_x.ai/compact_conversation",
            Self::Kiro => "_kiro/session/compact",
        }
    }
}

#[derive(Debug, Clone)]
pub struct AcpAgentDefinition {
    pub name: String,
    pub launch: AcpLaunch,
    pub new_session_instructions_meta_key: Option<String>,
    pub capabilities: AgentClientCapabilities,
    pub compaction: Option<CompactionProtocol>,
    pub interjection: Option<&'static InterjectionProtocol>,
    pub restricted_execution: Option<&'static RestrictedExecution>,
}

#[derive(Clone)]
pub struct AcpAgents(Arc<BTreeMap<String, AcpAgentDefinition>>);

impl AcpAgents {
    pub fn builtin() -> Self {
        Self::new([
            (
                "antigravity".into(),
                AcpAgentDefinition {
                    name: "Google Antigravity".into(),
                    launch: AcpLaunch {
                        command: "agy".into(),
                        args: vec!["acp".into()],
                        env: BTreeMap::new(),
                    },
                    new_session_instructions_meta_key: None,
                    capabilities: AgentClientCapabilities {
                        manual_compaction: false,
                        compaction_events: false,
                        compaction_history: false,
                        ..crate::ACP_MAX_CAPABILITIES
                    },
                    compaction: None,
                    interjection: None,
                    restricted_execution: Some(&RestrictedExecution::Antigravity),
                },
            ),
            (
                "grok".into(),
                AcpAgentDefinition {
                    name: "Grok Build".into(),
                    launch: AcpLaunch {
                        command: "grok".into(),
                        args: vec!["agent".into(), "stdio".into()],
                        env: BTreeMap::new(),
                    },
                    new_session_instructions_meta_key: Some("rules".into()),
                    capabilities: crate::ACP_MAX_CAPABILITIES,
                    compaction: Some(CompactionProtocol::Grok),
                    interjection: Some(&InterjectionProtocol {
                        method: "_x.ai/interject",
                        echo: "_x.ai/session/interjection",
                        queue: "_x.ai/queue/changed",
                        terminal: "_x.ai/session_notification",
                        promoted_prefix: "interject-fallback-",
                        interjection_marker: "/_meta/interjection",
                        display_text: "/content/_meta/displayText",
                        prompt_index: "/_meta/promptIndex",
                    }),
                    restricted_execution: Some(&RestrictedExecution::AgentProfile(
                        &GROK_RESTRICTED,
                    )),
                },
            ),
            (
                "kiro".into(),
                AcpAgentDefinition {
                    name: "Kiro".into(),
                    launch: AcpLaunch {
                        command: "kiro-cli".into(),
                        args: vec!["acp".into(), "--agent-engine=v3".into()],
                        env: BTreeMap::new(),
                    },
                    new_session_instructions_meta_key: None,
                    capabilities: AgentClientCapabilities {
                        context_usage:
                            pontia_core::client_capabilities::ContextUsageCapability::Estimated,
                        ..crate::ACP_MAX_CAPABILITIES
                    },
                    compaction: Some(CompactionProtocol::Kiro),
                    interjection: None,
                    restricted_execution: Some(&RestrictedExecution::Kiro),
                },
            ),
        ])
    }

    pub fn new(agents: impl IntoIterator<Item = (String, AcpAgentDefinition)>) -> Self {
        Self(Arc::new(agents.into_iter().collect()))
    }

    pub fn get(&self, key: &str) -> Option<&AcpAgentDefinition> {
        self.0.get(key)
    }

    pub fn ids(&self) -> impl Iterator<Item = &str> {
        self.0.keys().map(String::as_str)
    }
}

const GROK_RESTRICTED: AgentProfile = AgentProfile {
    profile_key: "agentProfile",
    profile_name: ("name", "pontia-restricted"),
    profile_description: ("description", "Pontia restricted Session"),
    disabled_env_flags: &[
        "GROK_CLAUDE_MCPS_ENABLED",
        "GROK_CURSOR_MCPS_ENABLED",
        "GROK_MANAGED_MCPS_ENABLED",
        "GROK_MANAGED_MCP_GATEWAY_TOOLS_ENABLED",
    ],
    allow_key: "tools",
    allowed_tools: &["read_file", "run_terminal_cmd"],
    // Grok keeps the two MCP dispatch tools even with an explicit allowlist.
    deny_key: "disallowedTools",
    disabled_tools: &["search_tool", "use_tool"],
    discover_skills_key: "discoverSkills",
    yolo_key: "yoloMode",
    startup_model_key: "modelId",
    list_method: "_x.ai/models/list",
    catalog_path: "/result",
    agent_type_path: "/_meta/agentType",
    allowed_types: &["grok-build-plan"],
    info_method: "_x.ai/session/info",
    profile_path: "/result/agentName",
    model_path: "/result/model",
};
