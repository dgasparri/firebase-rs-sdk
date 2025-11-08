# Firebase Firestore

This module ports core pieces of the Firestore SDK to Rust so applications can
discover collections and create, update, retrieve, and delete documents. It
provides functionality to interact with Firestore, including retrieving and
querying documents, working with collections, and managing real-time updates.

It includes error handling, configuration options, and integration with
Firebase apps.

Porting status: 85% `[######### ]` ([details](https://github.com/dgasparri/firebase-rs-sdk/blob/main/src/firestore/PORTING_STATUS.md))

## Quick Start Example

```rust,ignore
use firebase_rs_sdk::app::*;
use firebase_rs_sdk::app_check::*;
use firebase_rs_sdk::auth::*;
use firebase_rs_sdk::firestore::*;

use std::collections::BTreeMap;
use std::time::Duration;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // TODO: replace with your project configuration
    let options = FirebaseOptions {
        project_id: Some("your-project".into()),
        // add other Firebase options as needed
        ..Default::default()
    };
    let app = initialize_app(options, Some(FirebaseAppSettings::default())).await?;
    let auth = auth_for_app(app.clone())?;
    // Optional: wire App Check tokens into Firestore.
    let app_check_provider = custom_provider(|| token_with_ttl("fake-token", Duration::from_secs(60)));
    let app_check =
        initialize_app_check(Some(app.clone()), AppCheckOptions::new(app_check_provider)).await?;
    let app_check_internal = FirebaseAppCheckInternal::new(app_check);
    let firestore = get_firestore(Some(app.clone())).await?;
    let client = FirestoreClient::with_http_datastore_authenticated(
        firebase_rs_sdk::firestore::api::Firestore::from_arc(firestore.clone()),
        auth.token_provider(),
        Some(app_check_internal.token_provider()),
    )?;
    let mut ada = BTreeMap::new();
    ada.insert("first".into(), FirestoreValue::from_string("Ada"));
    ada.insert("last".into(), FirestoreValue::from_string("Lovelace"));
    ada.insert("born".into(), FirestoreValue::from_integer(1815));
    let ada_snapshot = client.add_doc("users", ada).await?;
    println!("Document written with ID: {}", ada_snapshot.id());
    let mut alan = BTreeMap::new();
    alan.insert("first".into(), FirestoreValue::from_string("Alan"));
    alan.insert("middle".into(), FirestoreValue::from_string("Mathison"));
    alan.insert("last".into(), FirestoreValue::from_string("Turing"));
    alan.insert("born".into(), FirestoreValue::from_integer(1912));
let alan_snapshot = client.add_doc("users", alan).await?;
println!("Document written with ID: {}", alan_snapshot.id());
if let Some(born) = alan_snapshot.get("born")? {
    if let ValueKind::Integer(year) = born.kind() {
        println!("Alan was born in {year}");
    }
}
let query = firestore.collection("users").unwrap().query();
let aggregates = client.get_count(&query).await?;
println!(
    "Total users: {}",
    aggregates.count("count")?.unwrap_or_default()
);
    Ok(())
}
```

If App Check is not enabled for your app, pass `None` as the third argument to
`with_http_datastore_authenticated`.

### Quick Start Example Using Converters

```rust,ignore
use firebase_rs_sdk::app::*;
use firebase_rs_sdk::firestore::*;
use std::collections::BTreeMap;

#[derive(Clone)]
struct MyUser {
   name: String,
}

#[derive(Clone)]
struct UserConverter;

impl FirestoreDataConverter for UserConverter {
    type Model = MyUser;

    fn to_map(
        &self,
        value: &Self::Model,
    ) -> FirestoreResult<BTreeMap<String, FirestoreValue>> {
        // Encode your model into Firestore fields.
        todo!()
    }

    fn from_map(&self, value: &MapValue) -> FirestoreResult<Self::Model> {
        // Decode Firestore fields into your model.
        todo!()
    }
}

async fn example_with_converter(
    firestore: &Firestore,
    client: &FirestoreClient,
) -> FirestoreResult<Option<MyUser>> {
    let users = firestore.collection("typed-users")?.with_converter(UserConverter);
    let doc = users.doc(Some("ada"))?;
    client
        .set_doc_with_converter(&doc, MyUser { name: "Ada".to_string() }, None)
        .await?;
    let typed_snapshot = client.get_doc_with_converter(&doc).await?;
    let user: Option<MyUser> = typed_snapshot.data()?;
    Ok(user)
}
```

## References to the Firebase JS SDK

- QuickStart: <https://firebase.google.com/docs/firestore/quickstart>
- API: <https://firebase.google.com/docs/reference/js/firestore.md#firestore_package>
- Github Repo - Module: <https://github.com/firebase/firebase-js-sdk/tree/main/packages/firestore>
- Github Repo - API: <https://github.com/firebase/firebase-js-sdk/tree/main/packages/firebase/firestore>
