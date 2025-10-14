use serde_json::Value;

#[derive(Clone, Debug)]
pub struct ObjectMetadata {
    raw: Value,
}

impl ObjectMetadata {
    pub fn from_value(raw: Value) -> Self {
        Self { raw }
    }

    pub fn raw(&self) -> &Value {
        &self.raw
    }

    pub fn name(&self) -> Option<&str> {
        self.raw.get("name").and_then(Value::as_str)
    }

    pub fn bucket(&self) -> Option<&str> {
        self.raw.get("bucket").and_then(Value::as_str)
    }

    pub fn content_type(&self) -> Option<&str> {
        self.raw.get("contentType").and_then(Value::as_str)
    }
}
