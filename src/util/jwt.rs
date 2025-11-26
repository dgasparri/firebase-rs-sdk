use crate::util::base64::base64_decode;
use crate::util::json::json_eval;
use serde_json::{Map, Value};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Default)]
pub struct DecodedToken {
    pub header: Value,
    pub claims: Value,
    pub data: Value,
    pub signature: String,
}

pub fn decode_jwt(token: &str) -> DecodedToken {
    let mut parts = token.split('.');
    let header_part = parts.next().unwrap_or_default();
    let claims_part = parts.next().unwrap_or_default();
    let signature = parts.next().unwrap_or_default().to_string();

    let header = decode_part(header_part);
    let (claims, data) = decode_claims(claims_part);

    DecodedToken {
        header,
        claims,
        data,
        signature,
    }
}

pub fn is_valid_timestamp(token: &str) -> bool {
    let decoded = decode_jwt(token);
    let claims = match decoded.claims {
        Value::Object(ref map) => map,
        _ => return false,
    };

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|dur| dur.as_secs() as i64)
        .unwrap_or_default();

    let valid_since = claims
        .get("nbf")
        .or_else(|| claims.get("iat"))
        .and_then(value_as_i64)
        .unwrap_or_default();

    let valid_until = claims.get("exp").and_then(value_as_i64).unwrap_or(valid_since + 86_400);

    now >= valid_since && now <= valid_until
}

pub fn issued_at_time(token: &str) -> Option<i64> {
    let decoded = decode_jwt(token);
    match decoded.claims {
        Value::Object(map) => map.get("iat").and_then(value_as_i64),
        _ => None,
    }
}

pub fn is_valid_format(token: &str) -> bool {
    let decoded = decode_jwt(token);
    match decoded.claims {
        Value::Object(map) => map.contains_key("iat"),
        _ => false,
    }
}

pub fn is_admin_token(token: &str) -> bool {
    let decoded = decode_jwt(token);
    match decoded.claims {
        Value::Object(map) => matches!(map.get("admin"), Some(Value::Bool(true))),
        _ => false,
    }
}

fn decode_part(part: &str) -> Value {
    if part.is_empty() {
        return Value::Object(Map::new());
    }

    match base64_decode(part)
        .ok()
        .and_then(|decoded| json_eval::<Value>(&decoded).ok())
    {
        Some(value) => value,
        None => Value::Object(Map::new()),
    }
}

fn decode_claims(part: &str) -> (Value, Value) {
    match decode_part(part) {
        Value::Object(mut map) => {
            let data = map.remove("d").unwrap_or_else(|| Value::Object(Map::new()));
            (Value::Object(map), data)
        }
        other => (other, Value::Object(Map::new())),
    }
}

fn value_as_i64(value: &Value) -> Option<i64> {
    value.as_i64().or_else(|| value.as_u64().map(|v| v as i64))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::util::base64::base64_url_encode_trimmed;
    use serde_json::json;

    fn build_token(claims: &Value) -> String {
        let header = base64_url_encode_trimmed(&json!({"alg": "none"}).to_string());
        let claims_str = base64_url_encode_trimmed(&claims.to_string());
        format!("{}.{}.sig", header, claims_str)
    }

    #[test]
    fn decode_extracts_data() {
        let claims = json!({"iat": 1, "d": {"foo": "bar"}});
        let token = build_token(&claims);
        let decoded = decode_jwt(&token);
        assert_eq!(decoded.data["foo"], json!("bar"));
        assert!(!decoded.claims.get("d").is_some());
    }

    #[test]
    fn format_validation_requires_iat() {
        let token = build_token(&json!({"exp": 10}));
        assert!(!is_valid_format(&token));
    }

    #[test]
    fn admin_detection() {
        let token = build_token(&json!({"iat": 1, "admin": true}));
        assert!(is_admin_token(&token));
    }
}
