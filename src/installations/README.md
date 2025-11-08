# Firebase Installations

The Installations module issues Firebase Installation IDs (FIDs) and scoped auth tokens used by other Firebase services. This Rust port mirrors the public JS API while speaking directly to the official Firebase Installations REST endpoints.

Porting status: 45% `[#####     ]` ([details](https://github.com/dgasparri/firebase-rs-sdk/blob/main/src/installations/PORTING_STATUS.md))


## Quick Start Example
```rust,no_run
use firebase_rs_sdk::app::{initialize_app, FirebaseAppSettings, FirebaseOptions};
use firebase_rs_sdk::installations::{get_installations, InstallationToken};

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
   let app = initialize_app(
       FirebaseOptions {
           api_key: Some("AIza...".into()),
           project_id: Some("my-project".into()),
           app_id: Some("1:123:web:abc".into()),
           ..Default::default()
       },
       Some(FirebaseAppSettings::default()),
   ).await?;

   let installations = get_installations(Some(app.clone()))?;
   let fid = installations.get_id().await?;
   let InstallationToken { token, expires_at } = installations.get_token(false).await?;
   println!("FID={fid}, token={token}, expires={expires_at:?}");
   Ok(())
}
```

## References to the Firebase JS SDK

- API: <https://firebase.google.com/docs/reference/js/installations.md#installations_package>
- Github Repo - Module: <https://github.com/firebase/firebase-js-sdk/tree/main/packages/installations>
- Github Repo - API: <https://github.com/firebase/firebase-js-sdk/tree/main/packages/firebase/installations>

### WASM Notes

- Enable the `wasm-web` feature to pull in the fetch-based REST client and browser-specific glue.
- Add `experimental-indexed-db` alongside `wasm-web` to persist installations to IndexedDB; without it, wasm builds fall back to in-memory persistence while keeping the same API surface.
- IndexedDB persistence now has wasm-bindgen tests that validate round-trip storage and BroadcastChannel propagation (`src/installations/persistence.rs`).

