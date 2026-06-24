use serde_json::Value;

pub(crate) fn nested_string(value: &Value, path: &[&str]) -> Option<String> {
    let mut current = value;
    for key in path {
        current = current.get(*key)?;
    }
    current.as_str().map(ToString::to_string)
}

pub(crate) fn nested_array_strings(value: &Value, path: &[&str]) -> Option<Vec<String>> {
    let mut current = value;
    for key in path {
        current = current.get(*key)?;
    }
    Some(
        current
            .as_array()?
            .iter()
            .filter_map(Value::as_str)
            .map(ToString::to_string)
            .collect(),
    )
}

pub(crate) fn remove_internal_metadata_fields(value: &mut Value) {
    if let Some(object) = value.as_object_mut() {
        object.remove("source_ref");
    }
}
