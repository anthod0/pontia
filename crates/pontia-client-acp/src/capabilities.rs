use agent_client_protocol::v1::schema::InitializeResponse;
use pontia_core::{Error, Result, client_capabilities::AgentClientCapabilities};

use crate::{ACP_MAX_CAPABILITIES, AcpAgentDefinition};

pub(crate) struct AgentBounds {
    capabilities: AgentClientCapabilities,
    pub compaction: Option<crate::CompactionProtocol>,
    pub prompt_steering: bool,
    pub kiro: bool,
    pub interjection: Option<&'static crate::InterjectionProtocol>,
}

impl AgentBounds {
    pub fn compaction_events(&self) -> bool {
        self.capabilities.compaction_events
    }

    pub fn of(agent_id: &str, definition: Option<&AcpAgentDefinition>) -> Self {
        let mut caps = definition.map_or(ACP_MAX_CAPABILITIES, |agent| {
            agent.capabilities.intersection(&ACP_MAX_CAPABILITIES)
        });
        let compaction = definition.and_then(|agent| agent.compaction);
        if compaction.is_none() {
            caps.manual_compaction = false;
            caps.compaction_events = false;
            caps.compaction_history = false;
        }
        Self {
            compaction,
            capabilities: caps,
            kiro: agent_id == "kiro",
            prompt_steering: agent_id == "antigravity",
            interjection: definition.and_then(|agent| agent.interjection),
        }
    }
}

#[derive(Default, Clone, Copy)]
pub(crate) struct Negotiated {
    pub steer: bool,
    pub kiro_steer: bool,
    pub interject: bool,
    pub compact_command: bool,
    pub deferred_models: bool,
}

impl Negotiated {
    pub fn from_initialize(response: &InitializeResponse, bounds: &AgentBounds) -> Result<Self> {
        let caps = &response.agent_capabilities;
        let mut missing = Vec::new();
        if !caps.load_session {
            missing.push("loadSession");
        }
        if caps.session_capabilities.close.is_none() {
            missing.push("sessionCapabilities.close");
        }
        if !missing.is_empty() {
            return Err(Error::CapabilityUnavailable(format!(
                "ACP peer missing required capabilities: {}",
                missing.join(", ")
            )));
        }
        Ok(Self {
            kiro_steer: false,
            deferred_models: crate::models::deferred_options(caps),
            compact_command: match bounds.compaction {
                Some(crate::CompactionProtocol::Kiro) => caps
                    .meta
                    .as_ref()
                    .and_then(|meta| meta.get("kiro")?["extensionMethods"].as_array())
                    .is_some_and(|methods| {
                        methods
                            .iter()
                            .any(|method| method == "_kiro/session/compact")
                    }),
                Some(crate::CompactionProtocol::Grok) => response
                    .meta
                    .as_ref()
                    .and_then(|meta| meta.get("availableCommands")?.as_array())
                    .is_some_and(|commands| commands.iter().any(|cmd| cmd["name"] == "compact")),
                None => false,
            },
            interject: bounds.interjection.is_some()
                && response
                    .meta
                    .as_ref()
                    .and_then(|meta| meta.get("steering"))
                    .is_none(),
            steer: response
                .meta
                .as_ref()
                .and_then(|meta| meta.get("steering"))
                .is_some_and(|steering| steering["supported"] == true),
        })
    }
}

pub(crate) struct Live {
    pub current: bool,
    pub ready: bool,
    pub has_native: bool,
    pub closing: bool,
    pub active: bool,
    pub history_ok: bool,
    pub replaying: bool,
    pub blocked: bool,
}

pub(crate) fn live(
    bounds: &AgentBounds,
    negotiated: Negotiated,
    state: Live,
) -> Option<AgentClientCapabilities> {
    (state.current && state.ready && state.has_native && !state.closing).then(|| {
        let mut caps = bounds.capabilities.clone();
        caps.steer &= negotiated.steer
            || negotiated.kiro_steer
            || negotiated.interject
            || bounds.prompt_steering;
        caps.interrupt &= state.active;
        caps.manual_compaction &=
            negotiated.compact_command && !state.active && !state.blocked && !state.replaying;
        caps.compaction_history &= state.history_ok && !state.replaying;
        caps.timeline &= state.history_ok && !state.replaying;
        caps.accept_task &= !state.replaying && !state.blocked;
        caps
    })
}

pub(crate) fn effective(
    snapshot: Option<&AgentClientCapabilities>,
    usage_observed: Option<&str>,
    implementation: &AgentClientCapabilities,
) -> AgentClientCapabilities {
    AgentClientCapabilities {
        context_usage: match usage_observed {
            Some("exact") => implementation.context_usage,
            Some("estimated") if implementation.context_usage != Default::default() => {
                pontia_core::client_capabilities::ContextUsageCapability::Estimated
            }
            _ => Default::default(),
        },
        ..snapshot
            .map(|caps| caps.intersection(implementation))
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::AcpAgents;
    use pontia_core::client_capabilities::ContextUsageCapability;
    use serde_json::json;

    fn idle() -> Live {
        Live {
            current: true,
            ready: true,
            has_native: true,
            closing: false,
            active: false,
            history_ok: true,
            replaying: false,
            blocked: false,
        }
    }

    #[test]
    fn grok_live_capabilities_follow_the_protocol_and_session_state() {
        let agents = AcpAgents::builtin();
        let bounds = AgentBounds::of("grok", agents.get("grok"));
        for state in [
            Live {
                current: false,
                ..idle()
            },
            Live {
                ready: false,
                ..idle()
            },
            Live {
                has_native: false,
                ..idle()
            },
            Live {
                closing: true,
                ..idle()
            },
        ] {
            assert!(live(&bounds, Negotiated::default(), state).is_none());
        }
        let expected = AgentClientCapabilities {
            accept_task: true,
            report_turn_started: true,
            report_turn_finished: true,
            timeline: true,
            compaction_events: true,
            compaction_history: true,
            stream_output: true,
            context_usage: ContextUsageCapability::Exact,
            ..Default::default()
        };
        assert_eq!(
            live(&bounds, Negotiated::default(), idle()).unwrap(),
            expected
        );
        let negotiated = Negotiated {
            steer: true,
            kiro_steer: false,
            compact_command: true,
            interject: false,
            deferred_models: false,
        };
        assert!(live(&bounds, negotiated, idle()).unwrap().manual_compaction);
        let other = live(&AgentBounds::of("another", None), negotiated, idle()).unwrap();
        assert!(!other.manual_compaction && !other.compaction_events && !other.compaction_history);
        for (state, timeline, accept_task) in [
            (
                Live {
                    blocked: true,
                    ..idle()
                },
                true,
                false,
            ),
            (
                Live {
                    history_ok: false,
                    ..idle()
                },
                false,
                true,
            ),
            (
                Live {
                    replaying: true,
                    ..idle()
                },
                false,
                false,
            ),
        ] {
            let active = live(
                &bounds,
                negotiated,
                Live {
                    active: true,
                    ..state
                },
            )
            .unwrap();
            assert_eq!(
                active,
                AgentClientCapabilities {
                    steer: true,
                    interrupt: true,
                    compaction_history: timeline,
                    timeline,
                    accept_task,
                    ..expected.clone()
                }
            );
        }
        let mut agent = agents.get("grok").unwrap().clone();
        agent.capabilities.steer = false;
        assert!(
            !live(&AgentBounds::of("grok", Some(&agent)), negotiated, idle())
                .unwrap()
                .steer
        );
    }

    #[test]
    fn usage_requires_an_observation_even_when_the_peer_is_live() {
        let snapshot = AgentClientCapabilities {
            accept_task: true,
            ..Default::default()
        };
        for (live, accept_task) in [(None, false), (Some(&snapshot), true)] {
            for (observed, context_usage) in [
                (None, ContextUsageCapability::Unsupported),
                (Some("estimated"), ContextUsageCapability::Estimated),
                (Some("exact"), ContextUsageCapability::Exact),
            ] {
                assert_eq!(
                    effective(live, observed, &ACP_MAX_CAPABILITIES),
                    AgentClientCapabilities {
                        accept_task,
                        context_usage,
                        ..Default::default()
                    }
                );
            }
        }
    }

    #[test]
    fn kiro_uses_the_advertised_extension_to_enable_manual_compaction() {
        let agents = AcpAgents::builtin();
        let bounds = AgentBounds::of("kiro", agents.get("kiro"));
        for (methods, available) in [(json!([]), false), (json!(["_kiro/session/compact"]), true)] {
            let response = serde_json::from_value(json!({"protocolVersion":1,
                "_meta":{"availableCommands":[{"name":"compact"}]},
                "agentCapabilities":{"loadSession":true,"sessionCapabilities":{"close":{}},
                    "_meta":{"kiro":{"extensionMethods":methods}}}
            }))
            .unwrap();
            let negotiated = Negotiated::from_initialize(&response, &bounds).unwrap();
            assert_eq!(
                live(&bounds, negotiated, idle()).unwrap().manual_compaction,
                available
            );
        }
    }

    #[test]
    fn initialize_requires_load_and_close_and_reads_steering() {
        let agents = AcpAgents::builtin();
        let plain = AgentBounds::of("other", None);
        let grok = AgentBounds::of("grok", agents.get("grok"));
        for (caps, missing) in [
            (json!({"sessionCapabilities":{"close":{}}}), "loadSession"),
            (json!({"loadSession":true}), "sessionCapabilities.close"),
            (json!({}), "loadSession, sessionCapabilities.close"),
        ] {
            let response =
                serde_json::from_value(json!({"protocolVersion":1, "agentCapabilities":caps}))
                    .unwrap();
            assert!(matches!(Negotiated::from_initialize(&response, &plain),
                Err(Error::CapabilityUnavailable(message)) if message == format!("ACP peer missing required capabilities: {missing}")));
        }
        for supported in [None, Some(false), Some(true)] {
            let mut value = json!({"protocolVersion":1,"agentCapabilities":{"loadSession":true,"sessionCapabilities":{"close":{}}}});
            if let Some(supported) = supported {
                value["_meta"] = json!({"steering":{"supported":supported}});
            }
            let response = serde_json::from_value(value).unwrap();
            assert_eq!(
                Negotiated::from_initialize(&response, &plain)
                    .unwrap()
                    .steer,
                supported == Some(true)
            );
            let builtin = Negotiated::from_initialize(&response, &grok).unwrap();
            assert_eq!(builtin.steer, supported == Some(true));
            assert_eq!(builtin.interject, supported.is_none());
        }
    }
}
