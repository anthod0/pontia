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
        let limit = if method == "submit" {
            MAX_CONTROL_FRAME_BYTES
        } else {
            MAX_FRAME_BYTES
        };
        let response = self
            .rpc
            .request(method, params, limit, method == "history.read")
            .await?;
        if let Some(error) = response.get("error") {
            if method == "history.read" && error["code"] == -32601 {
                return Err(Error::CapabilityUnavailable(
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
}

impl pontia_agent_control::ClientControlChannel for PiRpcPeer {
    fn set_reasoning_effort<'a>(
        &'a self,
        reasoning_effort: &'a str,
    ) -> pontia_agent_control::ClientControlOperation<'a> {
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
    fn steer<'a>(
        &'a self,
        input: &'a str,
        inbox_message_id: Option<&'a str>,
        turn_id: &'a str,
    ) -> pontia_agent_control::ClientControlOperation<'a> {
        Box::pin(async move {
            if self
                .call(
                    "steer",
                    json!({"input":input,"inbox_message_id":inbox_message_id,"turn_id":turn_id}),
                )
                .await?
                != json!({"accepted":true})
            {
                return Err(Error::ControlUnknown("Invalid Pi steer response".into()));
            }
            Ok(())
        })
    }
    fn compact<'a>(
        &'a self,
        operation_id: &'a str,
    ) -> pontia_agent_control::ClientControlOperation<'a> {
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
    fn native_history(
        &self,
        params: Value,
    ) -> pontia_agent_control::ClientControlOperation<'_, Value> {
        Box::pin(async move { self.call("history.read", params).await })
    }
    fn available(&self) -> bool {
        !self.is_closed()
    }
    fn invalidate(&self) {
        self.close();
    }
    fn replay<'a>(
        &'a self,
        replay_id: &'a str,
    ) -> pontia_agent_control::ClientControlOperation<'a> {
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
    fn interrupt(&self) -> pontia_agent_control::ClientControlOperation<'_> {
        Box::pin(async {
            if self.call("interrupt", json!({})).await? != json!({"accepted":true}) {
                return Err(Error::ControlUnknown(
                    "Invalid Pi interrupt acknowledgement".into(),
                ));
            }
            Ok(())
        })
    }
    fn shutdown_if_idle(&self) -> pontia_agent_control::ClientControlOperation<'_, bool> {
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
    fn shutdown(&self) -> pontia_agent_control::ClientControlOperation<'_> {
        Box::pin(async {
            if self.call("shutdown", json!({})).await? != json!({"accepted":true}) {
                return Err(Error::ControlUnknown(
                    "Invalid Pi shutdown acknowledgement".into(),
                ));
            }
            Ok(())
        })
    }
    fn ping(&self) -> pontia_agent_control::ClientControlOperation<'_> {
        Box::pin(async {
            if self.call("ping", json!({})).await? != json!({"pong":true}) {
                return Err(Error::ControlUnknown("Invalid Pi ping response".into()));
            }
            Ok(())
        })
    }
    fn submit<'a>(
        &'a self,
        input: &'a str,
        inbox_message_id: Option<&'a str>,
    ) -> pontia_agent_control::ClientControlOperation<'a> {
        Box::pin(async move {
            if self
                .call(
                    "submit",
                    json!({"input":input,"inbox_message_id":inbox_message_id}),
                )
                .await?
                != json!({"accepted":true})
            {
                return Err(Error::ControlUnknown("Invalid Pi submit response".into()));
            }
            Ok(())
        })
    }
}
