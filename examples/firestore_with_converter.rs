use firebase_rs_sdk::firestore::*;
use std::collections::BTreeMap;

#[derive(Clone)]
struct MyUser {
    _name: String,
}

#[derive(Clone)]
struct UserConverter;
impl FirestoreDataConverter for UserConverter {
    type Model = MyUser;
    fn to_map(&self, _value: &Self::Model) -> FirestoreResult<BTreeMap<String, FirestoreValue>> {
        // Encode your model into Firestore fields.
        todo!()
    }
    fn from_map(&self, _value: &MapValue) -> FirestoreResult<Self::Model> {
        // Decode Firestore fields into your model.
        todo!()
    }
}

#[allow(dead_code)]
async fn example_with_converter(firestore: &Firestore, client: &FirestoreClient) -> FirestoreResult<Option<MyUser>> {
    let users = firestore.collection("typed-users")?.with_converter(UserConverter);
    let doc = users.doc(Some("ada"))?;
    client
        .set_doc_with_converter(
            &doc,
            MyUser {
                _name: "Ada".to_string(),
            },
            None,
        )
        .await?;
    let typed_snapshot = client.get_doc_with_converter(&doc).await?;
    let user: Option<MyUser> = typed_snapshot.data()?;
    Ok(user)
}

fn main() {
    println!("Example usage of a converter for the document. See the source code for details.");
}
