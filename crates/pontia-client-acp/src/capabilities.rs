use agent_client_protocol::v1::schema::InitializeResponse;
use pontia_core::{Error, Result, client_capabilities::AgentClientCapabilities};

use crate::{ACP_MAX_CAPABILITIES, AcpAgentDefinition};

pub(crate) struct AgentBounds(AgentClientCapabilities);

impl AgentBounds {
    pub fn of(definition: Option<&AcpAgentDefinition>) -> Self {
        Self(definition.map_or(ACP_MAX_CAPABILITIES, |agent| {
            agent.capabilities.intersection(&ACP_MAX_CAPABILITIES)
        }))
    }
}

#[derive(Default, Clone, Copy)]
pub(crate) struct Negotiated {
    pub steer: bool,
}

impl Negotiated {
    pub fn from_initialize(response: &InitializeResponse) -> Result<Self> {
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
        let mut caps = bounds.0.clone();
        caps.steer &= negotiated.steer;
        caps.interrupt &= state.active;
        caps.timeline &= state.history_ok && !state.replaying;
        caps.accept_task &= !state.replaying && !state.blocked;
        caps
    })
}

pub(crate) fn effective(
    snapshot: Option<&AgentClientCapabilities>,
    usage_observed: bool,
    implementation: &AgentClientCapabilities,
) -> AgentClientCapabilities {
    AgentClientCapabilities {
        context_usage: if usage_observed {
            implementation.context_usage
        } else {
            Default::default()
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
        let bounds = AgentBounds::of(agents.get("grok"));
        assert_eq!(bounds.0, AgentBounds::of(None).0);
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
            stream_output: true,
            context_usage: ContextUsageCapability::Exact,
            ..Default::default()
        };
        assert_eq!(
            live(&bounds, Negotiated::default(), idle()).unwrap(),
            expected
        );
        let negotiated = Negotiated { steer: true };
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
                    timeline,
                    accept_task,
                    ..expected.clone()
                }
            );
        }
        let mut agent = agents.get("grok").unwrap().clone();
        agent.capabilities.steer = false;
        assert!(
            !live(&AgentBounds::of(Some(&agent)), negotiated, idle())
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
                (false, ContextUsageCapability::Unsupported),
                (true, ContextUsageCapability::Exact),
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
    fn initialize_requires_load_and_close_and_reads_steering() {
        for (caps, missing) in [
            (json!({"sessionCapabilities":{"close":{}}}), "loadSession"),
            (json!({"loadSession":true}), "sessionCapabilities.close"),
            (json!({}), "loadSession, sessionCapabilities.close"),
        ] {
            let response =
                serde_json::from_value(json!({"protocolVersion":1, "agentCapabilities":caps}))
                    .unwrap();
            assert!(matches!(Negotiated::from_initialize(&response),
                Err(Error::CapabilityUnavailable(message)) if message == format!("ACP peer missing required capabilities: {missing}")));
        }
        for supported in [None, Some(false), Some(true)] {
            let mut value = json!({"protocolVersion":1,"agentCapabilities":{"loadSession":true,"sessionCapabilities":{"close":{}}}});
            if let Some(supported) = supported {
                value["_meta"] = json!({"steering":{"supported":supported}});
            }
            let response = serde_json::from_value(value).unwrap();
            assert_eq!(
                Negotiated::from_initialize(&response).unwrap().steer,
                supported == Some(true)
            );
        }
    }
}
