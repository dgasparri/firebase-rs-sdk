//! Passkey sign-in roundtrip demonstration using mocked WebAuthn challenge/response payloads.
use firebase_rs_sdk::auth::{WebAuthnAssertionResponse, WebAuthnMultiFactorGenerator, WebAuthnSignInChallenge};
use serde_json::json;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Simulated payload returned by Firebase after calling
    // `MultiFactorResolver::start_passkey_sign_in`. In a real flow this comes
    // from the REST API.
    let challenge_payload = json!({
        "challenge": "QUJD", // "ABC" base64url encoded.
        "rpId": "example.com",
        "allowCredentials": [
            {
                "type": "public-key",
                "id": "cred-123",
                "transports": ["internal"]
            }
        ]
    });

    let challenge = WebAuthnSignInChallenge::from_value(challenge_payload)?;
    println!(
        "Relying party: {:?}, challenge bytes: {:?}",
        challenge.rp_id(),
        challenge.challenge_bytes()? // URL-safe base64 decoded helper
    );

    // In the browser you would call `navigator.credentials.get` and feed the
    // result back into the resolver. Here we mock the shape of the response.
    let response_payload = json!({
        "credentialId": "cred-123",
        "clientDataJSON": "CLIENT_DATA",
        "authenticatorData": "AUTH_DATA"
    });

    let response = WebAuthnAssertionResponse::try_from(response_payload)?
        .with_signature("SIGNATURE")
        .with_user_handle(Some("user-handle".to_string()));

    // Build the final assertion that can be passed to `resolver.resolve_sign_in`.
    let assertion = WebAuthnMultiFactorGenerator::assertion_for_sign_in("enroll1", response);
    assert_eq!(assertion.factor_id(), firebase_rs_sdk::auth::WEBAUTHN_FACTOR_ID);

    println!("Prepared assertion for enrollment 'enroll1'");
    Ok(())
}
