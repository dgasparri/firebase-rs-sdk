use serde_json::{Map, Value};

pub fn deep_copy(value: &Value) -> Value {
    deep_extend(&Value::Null, value)
}

pub fn deep_extend(target: &Value, source: &Value) -> Value {
    match source {
        Value::Object(source_map) => {
            let mut result = match target {
                Value::Object(target_map) => target_map.clone(),
                _ => Map::new(),
            };
            for (key, value) in source_map {
                if key == "__proto__" {
                    continue;
                }
                let existing = result.get(key).cloned().unwrap_or(Value::Null);
                result.insert(key.clone(), deep_extend(&existing, value));
            }
            Value::Object(result)
        }
        Value::Array(source_array) => Value::Array(source_array.clone()),
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => source.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn deep_copy_preserves_nested_objects() {
        let original = json!({"a": {"b": 2}});
        let copy = deep_copy(&original);
        assert_eq!(original, copy);
    }

    #[test]
    fn deep_extend_merges_objects() {
        let target = json!({"a": {"b": 1}, "c": 2});
        let source = json!({"a": {"d": 3}});
        let merged = deep_extend(&target, &source);
        assert_eq!(merged["a"]["b"], json!(1));
        assert_eq!(merged["a"]["d"], json!(3));
        assert_eq!(merged["c"], json!(2));
    }

    #[test]
    fn deep_extend_overwrites_arrays() {
        let target = json!([1, 2, 3]);
        let source = json!([4, 5]);
        let merged = deep_extend(&target, &source);
        assert_eq!(merged, source);
    }
}
