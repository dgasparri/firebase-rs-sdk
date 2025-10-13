use serde_json::{Map, Value};

pub fn is_empty(value: &Value) -> bool {
    match value {
        Value::Object(map) => map.is_empty(),
        Value::Array(array) => array.is_empty(),
        Value::Null => true,
        _ => false,
    }
}

pub fn deep_equal(a: &Value, b: &Value) -> bool {
    a == b
}

pub fn map_values<F>(input: &Map<String, Value>, mut f: F) -> Map<String, Value>
where
    F: FnMut(&Value, &str, &Map<String, Value>) -> Value,
{
    let mut result = Map::new();
    for (key, value) in input.iter() {
        result.insert(key.clone(), f(value, key, input));
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn empty_checks() {
        assert!(is_empty(&json!({})));
        assert!(is_empty(&json!([])));
        assert!(!is_empty(&json!({"a": 1})));
    }

    #[test]
    fn deep_equal_uses_value_eq() {
        assert!(deep_equal(&json!({"a": 1}), &json!({"a": 1})));
        assert!(!deep_equal(&json!({"a": 1}), &json!({"a": 2})));
    }

    #[test]
    fn map_values_transforms_entries() {
        let input = json!({"a": 1, "b": 2}).as_object().unwrap().clone();
        let mapped = map_values(&input, |value, _key, _| match value {
            Value::Number(num) => {
                Value::Number(serde_json::Number::from(num.as_i64().unwrap() * 2))
            }
            other => other.clone(),
        });
        assert_eq!(mapped.get("a").unwrap(), &json!(2));
        assert_eq!(mapped.get("b").unwrap(), &json!(4));
    }
}
