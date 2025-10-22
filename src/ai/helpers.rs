use crate::ai::backend::Backend;
use crate::ai::constants::AI_TYPE;
use crate::ai::error::{AiError, AiErrorCode, AiResult};

/// Encodes a backend configuration into an identifier string.
///
/// Ported from `packages/ai/src/helpers.ts` (`encodeInstanceIdentifier`).
pub fn encode_instance_identifier(backend: &Backend) -> String {
    match backend {
        Backend::GoogleAi(_) => format!("{}/googleai", AI_TYPE),
        Backend::VertexAi(config) => format!("{}/vertexai/{}", AI_TYPE, config.location()),
    }
}

/// Decodes an identifier back into a backend configuration.
///
/// Ported from `packages/ai/src/helpers.ts` (`decodeInstanceIdentifier`).
pub fn decode_instance_identifier(identifier: &str) -> AiResult<Backend> {
    let mut parts = identifier.split('/');
    let prefix = parts.next().unwrap_or_default();
    if prefix != AI_TYPE {
        return Err(AiError::new(
            AiErrorCode::Error,
            format!("Invalid instance identifier, unknown prefix '{prefix}'"),
            None,
        ));
    }

    match parts.next() {
        Some("googleai") => Ok(Backend::google_ai()),
        Some("vertexai") => {
            let location = parts.next().ok_or_else(|| {
                AiError::new(
                    AiErrorCode::Error,
                    format!("Invalid instance identifier, missing location '{identifier}'"),
                    None,
                )
            })?;
            Ok(Backend::vertex_ai(location))
        }
        other => Err(AiError::new(
            AiErrorCode::Error,
            format!("Invalid instance identifier string: '{identifier}' (segment={other:?})"),
            None,
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_decode_round_trip_vertex() {
        let backend = Backend::vertex_ai("europe-west4");
        let encoded = encode_instance_identifier(&backend);
        assert_eq!(encoded, "AI/vertexai/europe-west4");
        let decoded = decode_instance_identifier(&encoded).unwrap();
        assert_eq!(decoded, backend);
    }

    #[test]
    fn decode_invalid_prefix_returns_error_code() {
        let err = decode_instance_identifier("WRONG/id").unwrap_err();
        assert_eq!(err.code(), AiErrorCode::Error);
        assert!(err.code_str().contains("AI/error"));
    }
}
