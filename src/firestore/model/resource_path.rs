use std::fmt::{Display, Formatter};
use std::ops::Deref;

use crate::firestore::error::{invalid_argument, FirestoreResult};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ResourcePath {
    segments: Vec<String>,
}

impl ResourcePath {
    pub fn new(segments: Vec<String>) -> Self {
        Self { segments }
    }

    pub fn root() -> Self {
        Self {
            segments: Vec::new(),
        }
    }

    pub fn from_segments<I, S>(segments: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let segments = segments.into_iter().map(Into::into).collect();
        Self::new(segments)
    }

    pub fn from_string(path: &str) -> FirestoreResult<Self> {
        if path.trim().is_empty() {
            return Ok(Self::root());
        }

        if path.contains("//") {
            return Err(invalid_argument("Found empty segment in resource path"));
        }

        Ok(Self::from_segments(
            path.split('/')
                .filter(|segment| !segment.is_empty())
                .map(|segment| segment.to_string()),
        ))
    }

    pub fn len(&self) -> usize {
        self.segments.len()
    }

    pub fn is_empty(&self) -> bool {
        self.segments.is_empty()
    }

    pub fn segment(&self, index: usize) -> Option<&str> {
        self.segments.get(index).map(|s| s.as_str())
    }

    pub fn child<I, S>(&self, segments: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let mut new_segments = self.segments.clone();
        new_segments.extend(segments.into_iter().map(Into::into));
        Self::new(new_segments)
    }

    pub fn pop_last(&self) -> Option<Self> {
        if self.segments.is_empty() {
            return None;
        }
        let mut segments = self.segments.clone();
        segments.pop();
        Some(Self::new(segments))
    }

    pub fn last_segment(&self) -> Option<&str> {
        self.segments.last().map(|s| s.as_str())
    }

    pub fn as_vec(&self) -> &Vec<String> {
        &self.segments
    }

    pub fn canonical_string(&self) -> String {
        self.segments.join("/")
    }
}

impl Display for ResourcePath {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.canonical_string())
    }
}

impl Deref for ResourcePath {
    type Target = [String];

    fn deref(&self) -> &Self::Target {
        &self.segments
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_and_render_path() {
        let path = ResourcePath::from_string("cities/sf/neighborhoods/downtown").unwrap();
        assert_eq!(path.len(), 4);
        assert_eq!(path.last_segment(), Some("downtown"));
        assert_eq!(path.canonical_string(), "cities/sf/neighborhoods/downtown");
    }

    #[test]
    fn handles_root_path() {
        let path = ResourcePath::from_string("").unwrap();
        assert!(path.is_empty());
    }

    #[test]
    fn rejects_empty_segments() {
        let err = ResourcePath::from_string("cities//sf").unwrap_err();
        assert_eq!(err.code_str(), "firestore/invalid-argument");
    }
}
