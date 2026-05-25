use super::*;

pub(super) struct RunForTurn {
    pub(super) run_id: String,
    pub(super) work_item_id: String,
    pub(super) task_id: String,
    pub(super) session_id: Option<String>,
    pub(super) state: String,
}

#[derive(Debug, Clone)]
pub(super) struct ParsedRunResult {
    pub(super) state: String,
    pub(super) summary: String,
    pub(super) outputs: Vec<Value>,
    pub(super) failure: Option<Value>,
    pub(super) signals: Vec<RaiseSignalPayload>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SubmitResultToolOutcome {
    pub task_id: String,
    pub work_item_id: String,
    pub run_id: String,
    pub state: String,
    pub scheduler: DagSchedulerOutcome,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RaiseSignalToolOutcome {
    pub signal_id: String,
    pub task_id: String,
    pub work_item_id: Option<String>,
    pub run_id: Option<String>,
    pub kind: String,
    pub state: String,
    pub replanner_started: bool,
}

pub(super) struct TerminalEventRefs {
    pub(super) turn_id: Option<String>,
    pub(super) domain_event_id: Option<String>,
}

pub(super) struct SignalEvent<'a> {
    pub(super) task_id: &'a str,
    pub(super) signal_id: &'a str,
    pub(super) work_item_id: Option<&'a str>,
    pub(super) run_id: Option<&'a str>,
    pub(super) source_session_id: Option<&'a str>,
    pub(super) source: &'a str,
    pub(super) payload: &'a RaiseSignalPayload,
}
