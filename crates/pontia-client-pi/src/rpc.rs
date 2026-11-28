use pontia_agent_control::ipc::rpc::{MAX_FRAME_BYTES, Requests, RpcPeer};
use pontia_core::{Error, Result};
use serde_json::{Value, json};
use std::sync::Arc;
use tokio::net::UnixStream;

pub const MAX_CONTROL_FRAME_BYTES: usize = 64 * 1024;

pub struct PiRpcPeer {
    rpc: Arc<RpcPeer>,
}
impl PiRpcPeer {
    pub async fn set_reasoning_effort(&self, reasoning_effort: &str) -> Result<()> {
        crate::connections::ControlChannel::set_reasoning_effort(self, reasoning_effort).await
    }

    pub fn start_turn<'a>(
        &'a self,
        payload: &'a pontia_agent_control::InputPayload,
    ) -> pontia_agent_control::DeliveryOperation<'a> {
        crate::connections::ControlChannel::start_turn(self, payload)
    }

    pub fn steer<'a>(
        &'a self,
        payload: &'a pontia_agent_control::InputPayload,
    ) -> pontia_agent_control::DeliveryOperation<'a> {
        crate::connections::ControlChannel::steer(self, payload)
    }

    pub fn interrupt(&self) -> pontia_agent_control::DeliveryOperation<'_> {
        crate::connections::ControlChannel::interrupt(self)
    }

    pub fn new(stream: UnixStream) -> (Arc<Self>, Requests) {
        let (rpc, requests) = RpcPeer::new(stream);
        (Self::from_rpc(rpc), requests)
    }
    pub(crate) fn from_rpc(rpc: Arc<RpcPeer>) -> Arc<Self> {
        Arc::new(Self { rpc })
    }
    pub fn is_closed(&self) -> bool {
        self.rpc.is_closed()
    }
    pub fn close(&self) {
        self.rpc.close();
    }
    pub async fn closed(&self) {
        self.rpc.closed().await;
    }
    pub async fn reply(&self, id: Option<Value>, result: Value) -> Result<()> {
        self.rpc.reply(id, result).await
    }
    pub async fn reply_error(&self, id: Option<Value>, code: i64, message: &str) -> Result<()> {
        self.rpc.reply_error(id, code, message).await
    }
    pub async fn call(&self, method: &str, params: Value) -> Result<Value> {
        let response = self.request(method, params).await?;
        if let Some(error) = response.get("error") {
            if method == "history.read" && error["code"] == -32601 {
                return Err(Error::Unsupported(
                    "Pi peer does not support native history".into(),
                ));
            }
            if error["code"] == -32010 {
                return Err(Error::Conflict {
                    code: "input_busy",
                    message: error["message"].as_str().unwrap().into(),
                });
            }
            if error["code"] == -32011 {
                return Err(Error::Conflict {
                    code: "steer_target_changed",
                    message: error["message"].as_str().unwrap().into(),
                });
            }
            if error["code"] == -32007 {
                return Err(Error::ControlUnknown(
                    error["message"].as_str().unwrap().into(),
                ));
            }
            return Err(Error::Domain(format!(
                "Pi RPC {}: {}",
                error["code"], error["message"]
            )));
        }
        Ok(response["result"].clone())
    }
    async fn request(&self, method: &str, params: Value) -> Result<Value> {
        let limit = if method == "submit" {
            MAX_CONTROL_FRAME_BYTES
        } else {
            MAX_FRAME_BYTES
        };
        self.rpc
            .request(method, params, limit, method == "history.read")
            .await
    }
    async fn deliver(
        &self,
        method: &str,
        params: Value,
    ) -> std::result::Result<Value, pontia_agent_control::DeliveryError> {
        let response = self
            .request(method, params)
            .await
            .map_err(pontia_agent_control::DeliveryError::from)?;
        if let Some(error) = response.get("error") {
            let message = error["message"].as_str().unwrap().to_owned();
            return Err(match error["code"].as_i64() {
                Some(-32601) => pontia_agent_control::DeliveryError::Unsupported(message),
                Some(-32010 | -32011) => pontia_agent_control::DeliveryError::StateChanged(message),
                Some(-32007) => pontia_agent_control::DeliveryError::Uncertain(message),
                _ => pontia_agent_control::DeliveryError::Offline(message),
            });
        }
        Ok(response["result"].clone())
    }
}

impl crate::connections::ControlChannel for PiRpcPeer {
    fn set_reasoning_effort<'a>(
        &'a self,
        reasoning_effort: &'a str,
    ) -> crate::connections::ControlOperation<'a> {
        Box::pin(async move {
            if self
                .call(
                    "set_reasoning_effort",
                    json!({"reasoning_effort": reasoning_effort}),
                )
                .await?
                != json!({"accepted": true})
            {
                return Err(Error::ControlUnknown(
                    "Invalid Pi reasoning effort acknowledgement".into(),
                ));
            }
            Ok(())
        })
    }
    fn start_turn<'a>(
        &'a self,
        payload: &'a pontia_agent_control::InputPayload,
    ) -> pontia_agent_control::DeliveryOperation<'a> {
        Box::pin(async move {
            let result = self
                .deliver("submit", json!({"input":payload.text}))
                .await?;
            if result != json!({"accepted":true}) {
                return Err(pontia_agent_control::DeliveryError::Uncertain(
                    "Invalid Pi submit response".into(),
                ));
            }
            Ok(())
        })
    }
    fn steer<'a>(
        &'a self,
        payload: &'a pontia_agent_control::InputPayload,
    ) -> pontia_agent_control::DeliveryOperation<'a> {
        Box::pin(async move {
            if self.deliver("steer", json!({"input":payload.text})).await?
                != json!({"accepted":true})
            {
                return Err(pontia_agent_control::DeliveryError::Uncertain(
                    "Invalid Pi steer response".into(),
                ));
            }
            Ok(())
        })
    }
    fn compact<'a>(&'a self, operation_id: &'a str) -> crate::connections::ControlOperation<'a> {
        Box::pin(async move {
            if self
                .call("compact", json!({"operation_id": operation_id}))
                .await?
                != json!({"accepted": true})
            {
                return Err(Error::ControlUnknown(
                    "Invalid Pi compaction acknowledgement".into(),
                ));
            }
            Ok(())
        })
    }
    fn native_history(&self, params: Value) -> crate::connections::ControlOperation<'_, Value> {
        Box::pin(async move { self.call("history.read", params).await })
    }
    fn available(&self) -> bool {
        !self.is_closed()
    }
    fn invalidate(&self) {
        self.close();
    }
    fn replay<'a>(&'a self, replay_id: &'a str) -> crate::connections::ControlOperation<'a> {
        Box::pin(async move {
            if self
                .call("branch.replay", json!({"replay_id": replay_id}))
                .await?
                != json!({"accepted": true})
            {
                return Err(Error::ControlUnknown(
                    "Invalid Pi branch replay acknowledgement".into(),
                ));
            }
            Ok(())
        })
    }
    fn interrupt(&self) -> pontia_agent_control::DeliveryOperation<'_> {
        Box::pin(async {
            if self.deliver("interrupt", json!({})).await? != json!({"accepted":true}) {
                return Err(pontia_agent_control::DeliveryError::Uncertain(
                    "Invalid Pi interrupt acknowledgement".into(),
                ));
            }
            Ok(())
        })
    }
    fn shutdown_if_idle(&self) -> crate::connections::ControlOperation<'_, bool> {
        Box::pin(async {
            let response = self.call("shutdown_if_idle", json!({})).await?;
            if response == json!({"accepted":true}) {
                Ok(true)
            } else if response == json!({"accepted":false}) {
                Ok(false)
            } else {
                Err(Error::ControlUnknown(
                    "Invalid Pi conditional shutdown acknowledgement".into(),
                ))
            }
        })
    }
    fn shutdown(&self) -> crate::connections::ControlOperation<'_> {
        Box::pin(async {
            if self.call("shutdown", json!({})).await? != json!({"accepted":true}) {
                return Err(Error::ControlUnknown(
                    "Invalid Pi shutdown acknowledgement".into(),
                ));
            }
            Ok(())
        })
    }
}
