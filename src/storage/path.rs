pub fn parent(path: &str) -> Option<String> {
    if path.is_empty() {
        return None;
    }
    if let Some(index) = path.rfind('/') {
        if index == 0 {
            return Some(String::new());
        }
        return Some(path[..index].to_string());
    }
    Some(String::new())
}

pub fn child(path: &str, child_path: &str) -> String {
    let canonical_child = child_path
        .split('/')
        .filter(|component| !component.is_empty())
        .collect::<Vec<_>>()
        .join("/");

    if path.is_empty() {
        canonical_child
    } else if canonical_child.is_empty() {
        path.to_string()
    } else {
        format!("{}/{}", path, canonical_child)
    }
}

pub fn last_component(path: &str) -> String {
    if path.is_empty() {
        return String::new();
    }
    if let Some(index) = path[..path.len()].rfind('/') {
        return path[index + 1..].to_string();
    }
    path.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parent_handles_root() {
        assert_eq!(parent(""), None);
        assert_eq!(parent("foo"), Some(String::new()));
        assert_eq!(parent("foo/bar"), Some("foo".to_string()));
    }

    #[test]
    fn child_normalizes_slashes() {
        assert_eq!(child("", "a/b"), "a/b");
        assert_eq!(child("foo", "bar"), "foo/bar");
        assert_eq!(child("foo", "/bar//baz"), "foo/bar/baz");
    }

    #[test]
    fn last_component_extracts_tail() {
        assert_eq!(last_component("foo/bar"), "bar");
        assert_eq!(last_component("foo/bar/baz/"), "");
        assert_eq!(last_component("single"), "single");
    }
}
