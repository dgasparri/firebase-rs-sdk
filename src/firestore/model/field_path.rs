use crate::firestore::error::{invalid_argument, FirestoreResult};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct FieldPath {
    segments: Vec<String>,
}

impl FieldPath {
    pub fn new<S, I>(segments: I) -> FirestoreResult<Self>
    where
        S: Into<String>,
        I: IntoIterator<Item = S>,
    {
        let segments: Vec<String> = segments.into_iter().map(Into::into).collect();
        if segments.is_empty() {
            return Err(invalid_argument(
                "FieldPath must contain at least one segment",
            ));
        }
        Ok(Self { segments })
    }

    pub fn from_dot_separated(path: &str) -> FirestoreResult<Self> {
        if path.trim().is_empty() {
            return Err(invalid_argument("FieldPath string cannot be empty"));
        }
        FieldPath::new(path.split('.'))
    }

    pub fn last_segment(&self) -> &str {
        self.segments
            .last()
            .expect("FieldPath always has at least one segment")
            .as_str()
    }

    pub fn segments(&self) -> &[String] {
        &self.segments
    }

    pub fn canonical_string(&self) -> String {
        self.segments.join(".")
    }

    pub fn to_vec(&self) -> Vec<String> {
        self.segments.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_dot_path() {
        let field = FieldPath::from_dot_separated("foo.bar").unwrap();
        assert_eq!(field.segments(), &["foo", "bar"]);
    }

    #[test]
    fn rejects_empty() {
        let err = FieldPath::from_dot_separated("").unwrap_err();
        assert_eq!(err.code_str(), "firestore/invalid-argument");
    }
}
