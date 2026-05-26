use std::collections::{HashMap, HashSet};

use sqlx::{Row, Transaction};
use uuid::Uuid;

use super::*;

mod helpers;
mod initial_apply;
mod patch_apply;
mod patch_validation;
mod projection;
mod proposals;

use helpers::{
    append_task_event, ensure_task_exists, ensure_work_item_exists, ensure_work_item_not_running,
    expand_patch_operations, new_prefixed_id, parse_json_string, resolve_patch_ref,
    resolve_runtime_ref, validate_supersede_policy, work_item_event_payload,
};
use projection::initialize_projection;

#[derive(Clone)]
pub struct DagService {
    pool: SqlitePool,
    graph: GraphRuntimeConfig,
}

impl DagService {
    pub fn new(pool: SqlitePool) -> Self {
        Self::with_graph(pool, GraphRuntimeConfig::default())
    }

    pub fn with_graph(pool: SqlitePool, graph: GraphRuntimeConfig) -> Self {
        Self { pool, graph }
    }
}
