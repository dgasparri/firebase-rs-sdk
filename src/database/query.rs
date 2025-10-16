use serde_json::Value;

use crate::database::error::{internal_error, invalid_argument, DatabaseResult};

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub(crate) enum QueryIndex {
    #[default]
    Priority,
    Key,
    Value,
    Child(String),
}

#[derive(Clone, Debug)]
pub(crate) struct QueryParams {
    pub(crate) index: QueryIndex,
    pub(crate) start: Option<QueryBound>,
    pub(crate) end: Option<QueryBound>,
    pub(crate) limit: Option<QueryLimit>,
    pub(crate) order_by_called: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct QueryBound {
    pub(crate) value: Value,
    pub(crate) name: Option<String>,
    pub(crate) inclusive: bool,
}

#[derive(Clone, Debug)]
pub(crate) enum QueryLimit {
    First(u32),
    Last(u32),
}

impl Default for QueryParams {
    fn default() -> Self {
        Self {
            index: QueryIndex::Priority,
            start: None,
            end: None,
            limit: None,
            order_by_called: false,
        }
    }
}

impl QueryParams {
    pub(crate) fn set_index(&mut self, index: QueryIndex) -> DatabaseResult<()> {
        if self.order_by_called {
            return Err(invalid_argument("orderBy has already been specified"));
        }
        self.index = index;
        self.order_by_called = true;
        Ok(())
    }

    pub(crate) fn set_start(&mut self, bound: QueryBound) -> DatabaseResult<()> {
        if self.start.is_some() {
            return Err(invalid_argument(
                "startAt/startAfter has already been specified",
            ));
        }
        self.start = Some(bound);
        Ok(())
    }

    pub(crate) fn set_end(&mut self, bound: QueryBound) -> DatabaseResult<()> {
        if self.end.is_some() {
            return Err(invalid_argument(
                "endAt/endBefore has already been specified",
            ));
        }
        self.end = Some(bound);
        Ok(())
    }

    pub(crate) fn set_limit(&mut self, limit: QueryLimit) -> DatabaseResult<()> {
        if self.limit.is_some() {
            return Err(invalid_argument("limit has already been specified"));
        }
        self.limit = Some(limit);
        Ok(())
    }

    pub(crate) fn is_default(&self) -> bool {
        !self.order_by_called
            && matches!(self.index, QueryIndex::Priority)
            && self.start.is_none()
            && self.end.is_none()
            && self.limit.is_none()
    }

    pub(crate) fn to_rest_params(&self) -> DatabaseResult<Vec<(String, String)>> {
        let mut params = Vec::new();

        if self.is_default() {
            return Ok(params);
        }

        let order_by = match &self.index {
            QueryIndex::Priority => "$priority".to_string(),
            QueryIndex::Key => "$key".to_string(),
            QueryIndex::Value => "$value".to_string(),
            QueryIndex::Child(child) => child.clone(),
        };
        params.push((
            "orderBy".to_string(),
            serde_json::to_string(&order_by)
                .map_err(|err| internal_error(format!("Failed to encode orderBy: {err}")))?,
        ));

        if let Some(bound) = &self.start {
            let key = if bound.inclusive {
                "startAt"
            } else {
                "startAfter"
            };
            params.push((key.to_string(), encode_bound(bound)?));
        }

        if let Some(bound) = &self.end {
            let key = if bound.inclusive {
                "endAt"
            } else {
                "endBefore"
            };
            params.push((key.to_string(), encode_bound(bound)?));
        }

        if let Some(limit) = &self.limit {
            match limit {
                QueryLimit::First(count) => {
                    params.push(("limitToFirst".to_string(), count.to_string()));
                }
                QueryLimit::Last(count) => {
                    params.push(("limitToLast".to_string(), count.to_string()));
                }
            }
        }

        Ok(params)
    }
}

fn encode_bound(bound: &QueryBound) -> DatabaseResult<String> {
    let mut encoded = serde_json::to_string(&bound.value)
        .map_err(|err| internal_error(format!("Failed to encode query bound: {err}")))?;
    if let Some(name) = &bound.name {
        let encoded_name = serde_json::to_string(name)
            .map_err(|err| internal_error(format!("Failed to encode query name: {err}")))?;
        encoded.push(',');
        encoded.push_str(&encoded_name);
    }
    Ok(encoded)
}
