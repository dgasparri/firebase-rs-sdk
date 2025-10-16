use std::collections::{BTreeMap, HashMap};
use std::fmt;

pub type ErrorData = BTreeMap<String, String>;
pub type ErrorMap = &'static [(&'static str, &'static str)];

#[derive(Debug, Clone)]
pub struct FirebaseError {
    pub code: String,
    pub message: String,
    pub service: String,
    pub service_name: String,
    pub custom_data: ErrorData,
}

impl fmt::Display for FirebaseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for FirebaseError {}

pub struct ErrorFactory {
    service: String,
    service_name: String,
    errors: HashMap<&'static str, &'static str>,
}

impl ErrorFactory {
    pub fn new(service: &'static str, service_name: &'static str, errors: ErrorMap) -> Self {
        let errors_map = errors.iter().copied().collect();
        Self {
            service: service.to_string(),
            service_name: service_name.to_string(),
            errors: errors_map,
        }
    }

    pub fn create(&self, code: &str) -> FirebaseError {
        self.build_error(code, ErrorData::new())
    }

    pub fn create_with_data<I, K, V>(&self, code: &str, data: I) -> FirebaseError
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<String>,
        V: Into<String>,
    {
        let mut custom_data = ErrorData::new();
        for (key, value) in data {
            custom_data.insert(key.into(), value.into());
        }
        self.build_error(code, custom_data)
    }

    fn build_error(&self, code: &str, custom_data: ErrorData) -> FirebaseError {
        let template = self.errors.get(code).copied().unwrap_or("Error");
        let message = replace_template(template, &custom_data);
        let full_code = format!("{}/{}", self.service, code);
        let full_message = format!("{}: {} ({}).", self.service_name, message, full_code);

        FirebaseError {
            code: full_code,
            message: full_message,
            service: self.service.clone(),
            service_name: self.service_name.clone(),
            custom_data,
        }
    }
}

fn replace_template(template: &str, data: &ErrorData) -> String {
    let mut result = String::with_capacity(template.len());
    let mut remainder = template;

    while let Some(start) = remainder.find("{$") {
        let (head, tail) = remainder.split_at(start);
        result.push_str(head);
        if let Some(end) = tail.find('}') {
            let key = &tail[2..end];
            let value = data
                .get(key)
                .cloned()
                .unwrap_or_else(|| format!("<{key}?>"));
            result.push_str(&value);
            remainder = &tail[end + 1..];
        } else {
            // No closing brace, append the rest and break.
            result.push_str(tail);
            remainder = "";
            break;
        }
    }

    result.push_str(remainder);
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    const ERRORS: ErrorMap = &[
        ("missing", "Missing field: {$field}"),
        ("unknown", "Unknown error"),
    ];

    #[test]
    fn create_without_data_uses_template() {
        let factory = ErrorFactory::new("service", "Service", ERRORS);
        let error = factory.create("unknown");
        assert_eq!(error.code, "service/unknown");
        assert!(error.message.contains("Service"));
    }

    #[test]
    fn create_with_data_replaces_placeholders() {
        let factory = ErrorFactory::new("service", "Service", ERRORS);
        let error = factory.create_with_data("missing", [("field", "name")]);
        assert!(error.message.contains("name"));
        assert_eq!(error.custom_data.get("field"), Some(&"name".to_string()));
    }

    #[test]
    fn missing_placeholder_is_flagged() {
        let factory = ErrorFactory::new("service", "Service", ERRORS);
        let error = factory.create("missing");
        assert!(error.message.contains("<field?>"));
    }
}
