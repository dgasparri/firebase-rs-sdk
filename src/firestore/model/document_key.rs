use crate::firestore::error::{invalid_argument, FirestoreResult};
use crate::firestore::model::ResourcePath;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct DocumentKey {
    path: ResourcePath,
}

impl DocumentKey {
    pub fn from_path(path: ResourcePath) -> FirestoreResult<Self> {
        if path.len() < 2 || path.len() % 2 != 0 {
            return Err(invalid_argument(
                "Document keys must point to a document (even number of segments)",
            ));
        }
        Ok(Self { path })
    }

    pub fn from_string(path: &str) -> FirestoreResult<Self> {
        let resource = ResourcePath::from_string(path)?;
        Self::from_path(resource)
    }

    pub fn collection_path(&self) -> ResourcePath {
        self.path.without_last()
    }

    pub fn path(&self) -> &ResourcePath {
        &self.path
    }

    pub fn id(&self) -> &str {
        self.path
            .last_segment()
            .expect("DocumentKey path always has id")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_even_segments() {
        let err = DocumentKey::from_string("cities").unwrap_err();
        assert_eq!(err.code_str(), "firestore/invalid-argument");
    }

    #[test]
    fn parses_valid_path() {
        let key = DocumentKey::from_string("cities/sf").unwrap();
        assert_eq!(key.id(), "sf");
        assert_eq!(key.collection_path().canonical_string(), "cities");
    }
}
