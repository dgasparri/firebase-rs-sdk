use crate::firestore::value::FirestoreValue;

#[derive(Clone, Debug, PartialEq)]
pub struct ArrayValue {
    values: Vec<FirestoreValue>,
}

impl ArrayValue {
    pub fn new(values: Vec<FirestoreValue>) -> Self {
        Self { values }
    }

    pub fn values(&self) -> &[FirestoreValue] {
        &self.values
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stores_values() {
        let array = ArrayValue::new(vec![FirestoreValue::from_integer(1)]);
        assert_eq!(array.values().len(), 1);
    }
}
