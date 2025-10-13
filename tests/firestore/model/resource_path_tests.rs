use firebase_rs_sdk_unofficial::firestore::model::ResourcePath;

fn resource_path(path: &str) -> ResourcePath {
    ResourcePath::from_segments(path.split('/').filter(|segment| !segment.is_empty()))
}

#[test]
fn can_be_constructed() {
    ResourcePath::from_segments(["rooms", "Eros", "messages"]);
}

#[test]
fn indexes_into_segments() {
    let path = ResourcePath::from_segments(["rooms", "Eros", "messages"]);
    assert_eq!(path.get(0), Some("rooms"));
    assert_eq!(path.get(1), Some("Eros"));
    assert_eq!(path.get(2), Some("messages"));
}

#[test]
fn can_be_constructed_with_offsets() {
    let base: Vec<String> = ["rooms", "Eros", "messages"].into_iter().map(String::from).collect();
    let path = ResourcePath::with_offset(base.clone(), 2);
    assert_eq!(path, ResourcePath::from_segments(["messages"]));
    assert_eq!(path.len(), 1);

    let path = ResourcePath::with_offset(base.clone(), 3);
    assert!(path.is_empty());
}

#[test]
fn pop_first_repeatedly() {
    let path = ResourcePath::from_segments(["rooms", "Eros", "messages"]);

    assert_eq!(path.pop_first(), ResourcePath::from_segments(["Eros", "messages"]));
    assert_eq!(
        path.pop_first().pop_first(),
        ResourcePath::from_segments(["messages"])
    );
    assert!(path.pop_first().pop_first().pop_first().is_empty());
    assert_eq!(path.pop_first_n(0), path);
    assert_eq!(path.pop_first_n(1), ResourcePath::from_segments(["Eros", "messages"]));
    assert_eq!(path.pop_first_n(2), ResourcePath::from_segments(["messages"]));
    assert!(path.pop_first_n(3).is_empty());
    assert_eq!(path, ResourcePath::from_segments(["rooms", "Eros", "messages"]));
}

#[test]
fn yields_last_segment() {
    let path = ResourcePath::from_segments(["rooms", "Eros", "messages"]);
    assert_eq!(path.last_segment(), Some("messages"));
    assert_eq!(path.without_last().last_segment(), Some("Eros"));
    assert_eq!(path.without_last().without_last().last_segment(), Some("rooms"));
}

#[test]
fn creates_child_path() {
    let base = resource_path("rooms");
    assert_eq!(base.child(["eros"]), resource_path("rooms/eros"));
    assert_eq!(
        base.child(["eros"]).child(["1"]),
        resource_path("rooms/eros/1")
    );
    assert_eq!(base, resource_path("rooms"));
}

#[test]
fn pop_last_repeatedly() {
    let path = ResourcePath::from_segments(["rooms", "Eros", "messages"]);
    assert_eq!(path.without_last(), ResourcePath::from_segments(["rooms", "Eros"]));
    assert_eq!(
        path.without_last().without_last(),
        ResourcePath::from_segments(["rooms"])
    );
    assert!(path.without_last().without_last().without_last().is_empty());
    assert_eq!(path, ResourcePath::from_segments(["rooms", "Eros", "messages"]));
}

#[test]
fn compares_correctly() {
    fn expect_equal(a: &[&str], b: &[&str]) {
        assert_eq!(
            ResourcePath::comparator(&ResourcePath::from_segments(a.iter().cloned()),
                                      &ResourcePath::from_segments(b.iter().cloned())),
            std::cmp::Ordering::Equal
        );
    }

    fn expect_less(a: &[&str], b: &[&str]) {
        assert_eq!(
            ResourcePath::comparator(&ResourcePath::from_segments(a.iter().cloned()),
                                      &ResourcePath::from_segments(b.iter().cloned())),
            std::cmp::Ordering::Less
        );
        assert_eq!(
            ResourcePath::comparator(&ResourcePath::from_segments(b.iter().cloned()),
                                      &ResourcePath::from_segments(a.iter().cloned())),
            std::cmp::Ordering::Greater
        );
    }

    expect_equal(&[], &[]);
    expect_equal(&["a"], &["a"]);
    expect_equal(&["a", "b", "c"], &["a", "b", "c"]);

    expect_less(&[], &["a"]);
    expect_less(&["a"], &["b"]);
    expect_less(&["a"], &["a", "b"]);
}

#[test]
fn determines_prefix() {
    let empty = ResourcePath::root();
    let a = ResourcePath::from_segments(["a"]);
    let ab = ResourcePath::from_segments(["a", "b"]);
    let abc = ResourcePath::from_segments(["a", "b", "c"]);
    let b = ResourcePath::from_segments(["b"]);
    let ba = ResourcePath::from_segments(["b", "a"]);

    assert!(empty.is_prefix_of(&a));
    assert!(empty.is_prefix_of(&ab));
    assert!(empty.is_prefix_of(&abc));
    assert!(empty.is_prefix_of(&empty));
    assert!(empty.is_prefix_of(&b));
    assert!(empty.is_prefix_of(&ba));

    assert!(a.is_prefix_of(&a));
    assert!(a.is_prefix_of(&ab));
    assert!(a.is_prefix_of(&abc));
    assert!(!a.is_prefix_of(&empty));
    assert!(!a.is_prefix_of(&b));
    assert!(!a.is_prefix_of(&ba));

    assert!(!ab.is_prefix_of(&a));
    assert!(ab.is_prefix_of(&ab));
    assert!(ab.is_prefix_of(&abc));
    assert!(!ab.is_prefix_of(&empty));
    assert!(!ab.is_prefix_of(&b));
    assert!(!ab.is_prefix_of(&ba));

    assert!(!abc.is_prefix_of(&a));
    assert!(!abc.is_prefix_of(&ab));
    assert!(abc.is_prefix_of(&abc));
    assert!(!abc.is_prefix_of(&empty));
    assert!(!abc.is_prefix_of(&b));
    assert!(!abc.is_prefix_of(&ba));
}
