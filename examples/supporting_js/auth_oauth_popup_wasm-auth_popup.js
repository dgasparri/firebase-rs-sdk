export function open_popup_via_js(url) {
    console.info("[auth_popup] opening OAuth popup for", url);

    const preset = globalThis.__AUTH_POPUP_RESULT__;
    if (preset) {
        console.warn("[auth_popup] returning preset credential from __AUTH_POPUP_RESULT__");
        return preset;
    }

    throw new Error(
        "open_popup_via_js is a stub. Set window.__AUTH_POPUP_RESULT__ to a credential payload " +
            "or replace auth_oauth_popup_wasm-auth_popup.js with a platform-specific implementation."
    );
}

export function start_passkey_conditional_ui(request) {
    console.info("[auth_popup] starting passkey conditional UI with request", request);
    return Promise.reject(
        new Error(
            "start_passkey_conditional_ui is a stub. Replace it with a bridge to the WebAuthn conditional UI API."
        )
    );
}
