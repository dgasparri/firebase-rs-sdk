use std::collections::BTreeMap;

use crate::firestore::error::{invalid_argument, FirestoreResult};
use crate::firestore::model::{FieldPath, IntoFieldPath};
use crate::firestore::value::{FirestoreValue, ValueKind};

use super::query::Query;

#[derive(Clone, Debug)]
pub(crate) enum AggregateOperation {
    Count,
    Sum(FieldPath),
    Average(FieldPath),
}

#[derive(Clone, Debug)]
/// Describes a single aggregation to perform when executing [`crate::firestore::api::document::FirestoreClient::get_aggregate`].
///
/// Mirrors the modular JS `AggregateField` from
/// `packages/firestore/src/lite-api/aggregate_types.ts`.
pub struct AggregateField {
    operation: AggregateOperation,
}

impl AggregateField {
    /// Counts the number of documents returned by a query.
    ///
    /// TypeScript reference: `count()` in `packages/firestore/src/lite-api/aggregate.ts`.
    pub fn count() -> Self {
        Self {
            operation: AggregateOperation::Count,
        }
    }

    /// Sums the numeric values stored at the provided field path.
    ///
    /// TypeScript reference: `sum(fieldPath)` in
    /// `packages/firestore/src/lite-api/aggregate.ts`.
    pub fn sum<P>(field: P) -> FirestoreResult<Self>
    where
        P: IntoFieldPath,
    {
        let field_path = field.into_field_path()?;
        Ok(Self {
            operation: AggregateOperation::Sum(field_path),
        })
    }

    /// Computes the average of the numeric values stored at the provided field path.
    ///
    /// TypeScript reference: `average(fieldPath)` in
    /// `packages/firestore/src/lite-api/aggregate.ts`.
    pub fn average<P>(field: P) -> FirestoreResult<Self>
    where
        P: IntoFieldPath,
    {
        let field_path = field.into_field_path()?;
        Ok(Self {
            operation: AggregateOperation::Average(field_path),
        })
    }

    pub(crate) fn operation(&self) -> &AggregateOperation {
        &self.operation
    }
}

#[derive(Clone, Debug, Default)]
/// Collection of aggregate fields keyed by the alias used in the result set.
///
/// Mirrors the modular JS `AggregateSpec` type from
/// `packages/firestore/src/lite-api/aggregate_types.ts`.
pub struct AggregateSpec {
    fields: BTreeMap<String, AggregateField>,
}

impl AggregateSpec {
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers a new aggregate keyed by `alias`.
    pub fn insert(
        &mut self,
        alias: impl Into<String>,
        field: AggregateField,
    ) -> FirestoreResult<()> {
        let alias = alias.into();
        if alias.trim().is_empty() {
            return Err(invalid_argument(
                "Aggregate aliases must contain at least one non-whitespace character",
            ));
        }
        self.fields.insert(alias, field);
        Ok(())
    }

    /// Registers a new aggregate keyed by `alias`, returning the updated spec for chaining.
    pub fn with_field(
        mut self,
        alias: impl Into<String>,
        field: AggregateField,
    ) -> FirestoreResult<Self> {
        self.insert(alias, field)?;
        Ok(self)
    }

    pub fn is_empty(&self) -> bool {
        self.fields.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = (&String, &AggregateField)> {
        self.fields.iter()
    }

    pub(crate) fn definitions(&self) -> Vec<AggregateDefinition> {
        self.fields
            .iter()
            .map(|(alias, field)| AggregateDefinition {
                alias: alias.clone(),
                operation: field.operation().clone(),
            })
            .collect()
    }
}

#[derive(Clone, Debug)]
/// Snapshot of aggregate values returned by [`crate::firestore::api::document::FirestoreClient::get_aggregate`].
///
/// Mirrors the modular JS `AggregateQuerySnapshot` from
/// `packages/firestore/src/lite-api/aggregate_types.ts`.
pub struct AggregateQuerySnapshot {
    query: Query,
    spec: AggregateSpec,
    data: BTreeMap<String, FirestoreValue>,
}

impl AggregateQuerySnapshot {
    pub(crate) fn new(
        query: Query,
        spec: AggregateSpec,
        data: BTreeMap<String, FirestoreValue>,
    ) -> Self {
        Self { query, spec, data }
    }

    pub fn query(&self) -> &Query {
        &self.query
    }

    pub fn spec(&self) -> &AggregateSpec {
        &self.spec
    }

    pub fn data(&self) -> &BTreeMap<String, FirestoreValue> {
        &self.data
    }

    pub fn into_data(self) -> BTreeMap<String, FirestoreValue> {
        self.data
    }

    pub fn get(&self, alias: &str) -> Option<&FirestoreValue> {
        self.data.get(alias)
    }

    /// Returns the integer count recorded under `alias`, if present.
    pub fn count(&self, alias: &str) -> FirestoreResult<Option<i64>> {
        match self.data.get(alias) {
            Some(value) => match value.kind() {
                ValueKind::Integer(i) => Ok(Some(*i)),
                other => Err(invalid_argument(format!(
                    "Aggregate alias '{alias}' does not resolve to an integer count (found {other:?})"
                ))),
            },
            None => Ok(None),
        }
    }
}

#[derive(Clone, Debug)]
pub struct AggregateDefinition {
    alias: String,
    operation: AggregateOperation,
}

impl AggregateDefinition {
    pub(crate) fn alias(&self) -> &str {
        &self.alias
    }

    pub(crate) fn operation(&self) -> &AggregateOperation {
        &self.operation
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aggregate_spec_rejects_empty_alias() {
        let mut spec = AggregateSpec::new();
        let err = spec.insert(" ", AggregateField::count()).unwrap_err();
        assert_eq!(err.code_str(), "firestore/invalid-argument");
    }

    #[test]
    fn aggregate_definitions_clone_fields() {
        let mut spec = AggregateSpec::new();
        spec.insert("total", AggregateField::sum("population").unwrap())
            .unwrap();
        let defs = spec.definitions();
        assert_eq!(defs.len(), 1);
        assert_eq!(defs[0].alias(), "total");
    }
}
