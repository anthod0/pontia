use super::*;

impl DagService {
    pub async fn apply_initial_dag(
        &self,
        task_id: &str,
        payload: &SubmitPlanPayload,
    ) -> Result<()> {
        ensure_task_exists(&self.pool, task_id).await?;
        dag_validator::validate_plan_shape(payload)?;
        self.ensure_profiles_exist(&payload.work_items).await?;

        if GraphProjectionService::new(self.pool.clone(), self.graph.clone())
            .task_graph(task_id)
            .await?
            .work_items
            .iter()
            .any(|work_item| work_item.active)
        {
            return Err(Error::StateConflict(format!(
                "task {task_id} already has an active DAG"
            )));
        }

        let mut tx = self.pool.begin().await?;
        append_task_event(
            &mut tx,
            task_id,
            "dag.applied",
            json!({
                "task_id": task_id,
                "summary": payload.summary,
                "assumptions": payload.assumptions,
                "risks": payload.risks,
            }),
        )
        .await?;

        let mut id_map = HashMap::new();
        for draft in &payload.work_items {
            let work_item_id = new_prefixed_id("wi");
            id_map.insert(
                draft.temp_id.clone().unwrap_or_default(),
                work_item_id.clone(),
            );
            append_task_event(
                &mut tx,
                task_id,
                "work_item.created",
                json!({ "work_item": work_item_event_payload(task_id, &work_item_id, draft) }),
            )
            .await?;
        }
        for edge in &payload.edges {
            let from = id_map.get(&edge.from_work_item_id).ok_or_else(|| {
                Error::Domain(format!(
                    "edge references unknown from work item {}",
                    edge.from_work_item_id
                ))
            })?;
            let to = id_map.get(&edge.to_work_item_id).ok_or_else(|| {
                Error::Domain(format!(
                    "edge references unknown to work item {}",
                    edge.to_work_item_id
                ))
            })?;
            append_task_event(
                &mut tx,
                task_id,
                "work_item.edge_added",
                json!({
                    "task_id": task_id,
                    "from_work_item_id": from,
                    "to_work_item_id": to,
                    "edge_type": edge.edge_type,
                }),
            )
            .await?;
        }
        tx.commit().await?;

        GraphProjectionService::new(self.pool.clone(), self.graph.clone())
            .project_task(task_id)
            .await?;
        initialize_projection(&self.pool, &self.graph, task_id).await?;
        Ok(())
    }
}
