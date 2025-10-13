use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::Value;

pub fn json_eval<T: DeserializeOwned>(input: &str) -> serde_json::Result<T> {
    serde_json::from_str(input)
}

pub fn stringify<T: ?Sized + Serialize>(value: &T) -> serde_json::Result<String> {
    serde_json::to_string(value)
}

pub fn json_eval_value(input: &str) -> serde_json::Result<Value> {
    serde_json::from_str(input)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn roundtrip() {
        let value = json!({"a": 1, "b": "two"});
        let encoded = stringify(&value).unwrap();
        let decoded: Value = json_eval(&encoded).unwrap();
        assert_eq!(decoded, value);
    }
}
