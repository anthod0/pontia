use super::dag_scheduler::{SchedulerTaskContext, SchedulerWorkItem};

pub(crate) fn render_work_item_prompt(
    template: Option<&str>,
    task: &SchedulerTaskContext,
    work_item: &SchedulerWorkItem,
    run_id: &str,
) -> String {
    let fallback = "Execute WorkItem {{work_item_id}}: {{title}}\n\n{{description}}";
    let mut rendered = template.unwrap_or(fallback).to_string();
    for (key, value) in [
        ("task_id", task.task_id.as_str()),
        ("input", task.input.as_str()),
        ("work_item_id", work_item.work_item_id.as_str()),
        ("run_id", run_id),
        ("title", work_item.title.as_str()),
        ("description", work_item.description.as_str()),
        ("kind", work_item.kind.as_str()),
        ("action", work_item.action.as_str()),
    ] {
        rendered = rendered.replace(&format!("{{{{{key}}}}}"), value);
    }
    rendered
}
