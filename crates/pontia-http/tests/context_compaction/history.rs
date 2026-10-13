use super::fixture::*;
use pontia_application::ClientControlChannel;
use pontia_application::{
    ClientControlService,
    client_contract::{
        BranchTargetRequest, ClientData, NativeEventEvidence, TimelineBoundaryBackend,
        TurnTimelineBackend, TurnTopologyBackend,
        history::{RecoveredTurnHistory, TurnHistoryCandidate},
        native_history::{HistoryRead, NativeHistory},
        raw_transcripts::*,
    },
};
use pontia_core::{
    Error, Result,
    domain::{DomainEvent, EventType},
};
use serde_json::Value;
use std::sync::Arc;

pub struct History {
    pub rpc: bool,
}
impl ClientData for History {
    fn normalize_payload(&self, _: EventType, data: Value) -> Result<Value> {
        Ok(data)
    }
    fn take_evidence(&self, _: &mut DomainEvent) -> NativeEventEvidence {
        NativeEventEvidence::default()
    }
    fn native_history(
        &self,
        _: AgentBindingResolveRequest,
        _: Option<ClientControlService>,
    ) -> Option<Arc<dyn NativeHistory>> {
        self.rpc.then(|| Arc::new(Reader) as Arc<dyn NativeHistory>)
    }
    fn timeline(&self) -> TurnTimelineBackend {
        TurnTimelineBackend {
            resolver: Box::new(Reader),
            reader: Box::new(Reader),
        }
    }
    fn boundaries(&self) -> TimelineBoundaryBackend {
        TimelineBoundaryBackend {
            resolver: Box::new(Reader),
            capturer: Box::new(Reader),
        }
    }
    fn topology(&self) -> Option<TurnTopologyBackend> {
        None
    }
    fn branch_target(&self, _: BranchTargetRequest) -> Result<String> {
        Err(Error::CapabilityUnavailable("unsupported".into()))
    }
}
struct Reader;
impl AgentBindingResolver for Reader {
    fn client_type(&self) -> &'static str {
        SPEC.client_type
    }
    fn resolve(&self, request: &AgentBindingResolveRequest) -> Result<ResolvedAgentBinding> {
        Ok(ResolvedAgentBinding {
            id: request.id.clone(),
            client_type: request.client_type.clone(),
            format: "test-native".into(),
            path: request
                .client_session_file
                .clone()
                .unwrap_or_else(|| "/unused-native-source".into()),
            fingerprint: None,
        })
    }
}
impl TimelineBoundaryCapturer for Reader {
    fn client_type(&self) -> &'static str {
        SPEC.client_type
    }
    fn capture_boundary(
        &self,
        _: TimelineBoundaryCaptureRequest,
    ) -> Result<CapturedTimelineBoundary> {
        Err(Error::CapabilityUnavailable("unsupported".into()))
    }
    fn capture_source_origin_head(
        &self,
        _: &str,
        _: Option<String>,
    ) -> Result<CapturedTimelineBoundary> {
        Err(Error::CapabilityUnavailable("unsupported".into()))
    }
}
fn item(id: &str, kind: &str, turn: Option<&str>) -> TurnTimelineItem {
    TurnTimelineItem {
        turn_id: turn.map(str::to_owned),
        item: TimelineItem {
            item_id: id.into(),
            kind: kind.into(),
            raw_kind: None,
            role: "system".into(),
            title: None,
            status: None,
            occurred_at: None,
            content_preview: String::new(),
            managed_tool_use: None,
        },
    }
}
fn selected(selection: SessionTimelineSelection) -> Vec<TurnTimelineItem> {
    if selection.turns.is_empty() {
        return vec![item("before-first-turn", "context_compaction", None)];
    }
    let mut items = vec![];
    if selection.selected_turn_ids.iter().any(|id| id == "turn_a") {
        items.push(item("a", "assistant_message", Some("turn_a")));
    }
    if selection.selected_turn_ids.iter().any(|id| id == "turn_b") {
        items.extend([
            item("gap", "context_compaction", None),
            item("b", "user_message", Some("turn_b")),
            item("inside-b", "context_compaction", Some("turn_b")),
            item("tail", "context_compaction", None),
        ]);
    }
    items
}
impl TurnTimelineReader for Reader {
    fn client_type(&self) -> &'static str {
        SPEC.client_type
    }
    fn read_session_timeline(
        &self,
        request: SessionTimelineReadRequest,
    ) -> std::result::Result<Vec<TurnTimelineItem>, TurnTimelineReadError> {
        Ok(selected(request.selection))
    }
    fn read_turn_ranges(
        &self,
        request: TurnTimelineReadRequest,
    ) -> std::result::Result<Vec<TurnTimelineItem>, TurnTimelineReadError> {
        Ok(request
            .ranges
            .into_iter()
            .map(|range| item(&range.turn_id, "assistant_message", Some(&range.turn_id)))
            .collect())
    }
}
impl NativeHistory for Reader {
    fn read_session_timeline(
        &self,
        selection: SessionTimelineSelection,
    ) -> HistoryRead<'_, std::result::Result<Vec<TurnTimelineItem>, TurnTimelineReadError>> {
        Box::pin(async move { Ok(selected(selection)) })
    }
    fn read_ranges(
        &self,
        ranges: Vec<TurnTimelineRange>,
    ) -> HistoryRead<'_, std::result::Result<Vec<TurnTimelineItem>, TurnTimelineReadError>> {
        Box::pin(async move {
            Ok(ranges
                .into_iter()
                .map(|range| item(&range.turn_id, "assistant_message", Some(&range.turn_id)))
                .collect())
        })
    }
    fn recover(
        &self,
        _: Vec<TurnHistoryCandidate>,
    ) -> HistoryRead<'_, Result<Vec<RecoveredTurnHistory>>> {
        Box::pin(async { Ok(vec![]) })
    }
    fn branch_target(&self, _: TurnTimelineRange) -> HistoryRead<'_, Result<String>> {
        Box::pin(async { Err(Error::CapabilityUnavailable("unsupported".into())) })
    }
}
async fn turns(fixture: &Fixture) {
    for turn in ["turn_a", "turn_b"] {
        sqlx::query("INSERT INTO turns(turn_id,session_id,state,head_cursor,tail_cursor,topology_status) VALUES (?,'session','completed',?,?,'root')").bind(turn).bind(format!("head:{turn}")).bind(format!("tail:{turn}")).execute(&fixture.state.db()).await.unwrap();
    }
}
#[tokio::test]
async fn session_timeline_preserves_native_order_and_nullable_turn_identity_from_both_sources() {
    for rpc in [false, true] {
        let fixture = Fixture::new(&SPEC, rpc).await;
        fixture.seed().await;
        turns(&fixture).await;
        // No compaction lifecycle facts exist; native history alone supplies checkpoints.
        for direction in ["forward", "backward"] {
            let (status, body) = fixture
                .request(
                    "GET",
                    &format!("/api/v1/sessions/session/turns/timeline?direction={direction}"),
                    None,
                    true,
                )
                .await;
            assert_eq!(status, axum::http::StatusCode::OK, "{body}");
            let items = body["data"]["items"].as_array().unwrap();
            let ids: Vec<_> = items
                .iter()
                .map(|item| item["item_id"].as_str().unwrap())
                .collect();
            assert_eq!(ids, ["a", "gap", "b", "inside-b", "tail"]);
            assert!(items[1]["turn_id"].is_null());
            assert_eq!(items[1]["kind"], "context_compaction");
            assert_eq!(items[3]["turn_id"], "turn_b");
            assert!(items[4]["turn_id"].is_null());
        }
        assert_eq!(
            fixture
                .state
                .queries()
                .list_session_events("session")
                .await
                .unwrap()
                .len(),
            2
        );
    }
}
#[tokio::test]
async fn paginated_history_keeps_gap_checkpoints_with_the_following_turn() {
    let fixture = Fixture::new(&SPEC, true).await;
    fixture.seed().await;
    turns(&fixture).await;
    let (_, first) = fixture
        .request(
            "GET",
            "/api/v1/sessions/session/turns/timeline?direction=forward&limit=1",
            None,
            true,
        )
        .await;
    assert_eq!(first["data"]["next_turn_id"], "turn_b");
    assert_eq!(first["data"]["items"].as_array().unwrap().len(), 1);
    let (_, second) = fixture
        .request(
            "GET",
            "/api/v1/sessions/session/turns/timeline?direction=forward&turn_id=turn_b&limit=1",
            None,
            true,
        )
        .await;
    assert!(second["data"]["next_turn_id"].is_null());
    let ids: Vec<_> = second["data"]["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["item_id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, ["gap", "b", "inside-b", "tail"]);
}
#[tokio::test]
async fn native_checkpoints_can_be_read_without_any_pontia_turns_or_writable_channel() {
    let fixture = Fixture::new(&SPEC, false).await;
    fixture.seed().await;
    fixture.channel.invalidate();
    let (status, body) = fixture
        .request(
            "GET",
            "/api/v1/sessions/session/turns/timeline?direction=forward",
            None,
            true,
        )
        .await;
    assert_eq!(status, axum::http::StatusCode::OK, "{body}");
    assert_eq!(body["data"]["items"][0]["item_id"], "before-first-turn");
    assert!(body["data"]["items"][0]["turn_id"].is_null());
}
#[tokio::test]
async fn compaction_history_does_not_follow_the_event_capability_and_unsupported_history_keeps_existing_behavior()
 {
    const NO_EVENTS: pontia_application::client_contract::AgentClientSpec =
        pontia_application::client_contract::AgentClientSpec {
            capabilities: pontia_application::client_contract::AgentClientCapabilities {
                compaction_events: false,
                ..SPEC.capabilities
            },
            ..SPEC
        };
    let fixture = Fixture::new(&NO_EVENTS, true).await;
    fixture.seed().await;
    let (status, body) = fixture
        .request(
            "GET",
            "/api/v1/sessions/session/turns/timeline?direction=forward",
            None,
            true,
        )
        .await;
    assert_eq!(status, axum::http::StatusCode::OK);
    assert_eq!(body["data"]["items"][0]["kind"], "context_compaction");
    let unsupported = Fixture::new(&UNSUPPORTED, false).await;
    unsupported.seed().await;
    let (status, body) = unsupported
        .request(
            "GET",
            "/api/v1/sessions/session/turns/timeline?direction=forward",
            None,
            true,
        )
        .await;
    assert_eq!(status, axum::http::StatusCode::OK);
    assert_eq!(body["data"]["items"], serde_json::json!([]));
    turns(&unsupported).await;
    let (status, body) = unsupported
        .request(
            "GET",
            "/api/v1/sessions/session/turns/timeline?direction=forward",
            None,
            true,
        )
        .await;
    assert_eq!(status, axum::http::StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body["error"]["code"], "timeline_capability_unavailable");
}
