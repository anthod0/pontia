use pontia_agent_control::{agent_clients::AgentClientModels, sessions::ReasoningEffortOption};
use serde_json::Value;

pub(super) fn read_efforts(models: &mut AgentClientModels, response: &Value) {
    let Some(available) = response
        .pointer("/models/availableModels")
        .and_then(Value::as_array)
    else {
        return;
    };
    for entry in available {
        let Some(model) = models
            .models
            .iter_mut()
            .find(|model| entry["modelId"].as_str() == Some(&model.id))
        else {
            continue;
        };
        let meta = &entry["_meta"];
        if meta["supportsReasoningEffort"] == false {
            model.reasoning_efforts.clear();
            model.default_reasoning_effort = None;
            continue;
        }
        let Some(levels) = meta["reasoningEfforts"].as_array() else {
            continue;
        };
        let mut efforts = Vec::new();
        let mut default = None;
        for level in levels {
            let Some(id) = level["value"]
                .as_str()
                .filter(|id| !id.is_empty())
                .or_else(|| level["id"].as_str().filter(|id| !id.is_empty()))
            else {
                continue;
            };
            if level["default"] == true || level["isDefault"] == true {
                default.get_or_insert_with(|| id.to_owned());
            }
            efforts.push(ReasoningEffortOption {
                id: id.to_owned(),
                name: level["label"].as_str().unwrap_or(id).to_owned(),
                description: level["description"].as_str().unwrap_or_default().to_owned(),
            });
        }
        model.default_reasoning_effort = meta["reasoningEffort"]
            .as_str()
            .filter(|id| efforts.iter().any(|e| e.id == *id))
            .map(str::to_owned)
            .or(default);
        model.reasoning_efforts = efforts;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn unlisted_current_effort_uses_the_advertised_default() {
        let mut models: AgentClientModels = serde_json::from_value(json!({
            "models": [{"id":"deep","name":"Deep","description":""}], "default_model":"deep"
        }))
        .unwrap();
        read_efforts(
            &mut models,
            &json!({"models":{"availableModels":[{
                "modelId":"deep", "_meta":{"reasoningEffort":"removed","reasoningEfforts":[
                    {"label":"Missing ID"}, {"value":"low"}, {"value":"","id":"thorough","isDefault":true}
                ]}
            }]}}),
        );
        let model = &models.models[0];
        assert_eq!(model.default_reasoning_effort.as_deref(), Some("thorough"));
        assert_eq!(model.reasoning_efforts.len(), 2);
        assert_eq!(model.reasoning_efforts[1].name, "thorough");
    }
}
