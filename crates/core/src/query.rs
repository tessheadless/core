// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

//! Backend-neutral query language plus a reference in-memory evaluator.
//!
//! Backends with a rich query engine (Postgres, MongoDB) translate [`Filter`]
//! natively. Key-value backends (RocksDB, DynamoDB, memory) load the entries
//! of a content type and call [`execute_in_memory`]. The testkit checks that
//! both paths return the same results.

use std::cmp::Ordering;

use serde_json::{Map, Value};

use crate::model::{Entry, Stage};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    Eq,
    Ne,
    Lt,
    Lte,
    Gt,
    Gte,
    /// Value is an array, matches if the field equals one of its elements.
    In,
    NotIn,
    Contains,
    StartsWith,
    EndsWith,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Filter {
    And(Vec<Filter>),
    Or(Vec<Filter>),
    Not(Box<Filter>),
    Cmp { field: String, op: Op, value: Value },
}

impl Filter {
    pub fn cmp(field: impl Into<String>, op: Op, value: impl Into<Value>) -> Self {
        Filter::Cmp { field: field.into(), op, value: value.into() }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Asc,
    Desc,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SortKey {
    pub field: String,
    pub direction: Direction,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EntryQuery {
    pub stage: Stage,
    pub filter: Option<Filter>,
    /// Sort keys in priority order. Backends always append `id ASC` as a
    /// final tie-breaker so pagination is stable.
    pub sort: Vec<SortKey>,
    pub limit: usize,
    pub offset: usize,
}

impl EntryQuery {
    pub fn new(stage: Stage) -> Self {
        EntryQuery { stage, filter: None, sort: Vec::new(), limit: 100, offset: 0 }
    }

    /// Sort keys with the default (`createdAt`) and the `id` tie-breaker applied.
    pub fn effective_sort(&self) -> Vec<SortKey> {
        let mut keys = self.sort.clone();
        if keys.is_empty() {
            keys.push(SortKey { field: "createdAt".into(), direction: Direction::Asc });
        }
        if !keys.iter().any(|k| k.field == "id") {
            keys.push(SortKey { field: "id".into(), direction: Direction::Asc });
        }
        keys
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Page {
    pub items: Vec<Entry>,
    /// Number of entries matching the filter, ignoring limit/offset.
    pub total: u64,
}

/// Filters, sorts and paginates entries of a single content type in memory.
pub fn execute_in_memory(entries: impl IntoIterator<Item = Entry>, query: &EntryQuery) -> Page {
    let mut matched: Vec<(Map<String, Value>, Entry)> = entries
        .into_iter()
        .filter_map(|e| e.view(query.stage).map(|v| (v, e)))
        .filter(|(view, _)| query.filter.as_ref().is_none_or(|f| matches(f, view)))
        .collect();
    let sort = query.effective_sort();
    matched.sort_by(|(a, _), (b, _)| compare_docs(a, b, &sort));
    let total = matched.len() as u64;
    let items = matched.into_iter().skip(query.offset).take(query.limit).map(|(_, e)| e).collect();
    Page { items, total }
}

/// Evaluates a filter against an entry view. Missing fields behave as `null`.
pub fn matches(filter: &Filter, doc: &Map<String, Value>) -> bool {
    match filter {
        Filter::And(fs) => fs.iter().all(|f| matches(f, doc)),
        Filter::Or(fs) => fs.iter().any(|f| matches(f, doc)),
        Filter::Not(f) => !matches(f, doc),
        Filter::Cmp { field, op, value } => {
            let actual = doc.get(field).unwrap_or(&Value::Null);
            compare(actual, *op, value)
        }
    }
}

fn compare(actual: &Value, op: Op, expected: &Value) -> bool {
    match op {
        Op::Eq => json_eq(actual, expected),
        Op::Ne => !json_eq(actual, expected),
        Op::In => expected.as_array().is_some_and(|xs| xs.iter().any(|x| json_eq(actual, x))),
        Op::NotIn => !expected.as_array().is_some_and(|xs| xs.iter().any(|x| json_eq(actual, x))),
        Op::Lt => same_type_cmp(actual, expected) == Some(Ordering::Less),
        Op::Lte => matches!(same_type_cmp(actual, expected), Some(Ordering::Less | Ordering::Equal)),
        Op::Gt => same_type_cmp(actual, expected) == Some(Ordering::Greater),
        Op::Gte => {
            matches!(same_type_cmp(actual, expected), Some(Ordering::Greater | Ordering::Equal))
        }
        Op::Contains | Op::StartsWith | Op::EndsWith => match (actual, expected) {
            (Value::String(a), Value::String(e)) => match op {
                Op::Contains => a.contains(e.as_str()),
                Op::StartsWith => a.starts_with(e.as_str()),
                _ => a.ends_with(e.as_str()),
            },
            _ => false,
        },
    }
}

/// Equality where `1` and `1.0` are equal, as in Postgres and MongoDB.
pub fn json_eq(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => x.as_f64() == y.as_f64(),
        _ => a == b,
    }
}

/// Ordering only between values of the same JSON type; `None` otherwise, so
/// range operators never match across types (e.g. `null < 5` is false).
fn same_type_cmp(a: &Value, b: &Value) -> Option<Ordering> {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => x.as_f64()?.partial_cmp(&y.as_f64()?),
        (Value::String(x), Value::String(y)) => Some(x.cmp(y)),
        (Value::Bool(x), Value::Bool(y)) => Some(x.cmp(y)),
        _ => None,
    }
}

/// Total order used for sorting: null < numbers < strings < booleans < others.
fn sort_cmp(a: &Value, b: &Value) -> Ordering {
    fn rank(v: &Value) -> u8 {
        match v {
            Value::Null => 0,
            Value::Number(_) => 1,
            Value::String(_) => 2,
            Value::Bool(_) => 3,
            Value::Array(_) => 4,
            Value::Object(_) => 5,
        }
    }
    rank(a).cmp(&rank(b)).then_with(|| same_type_cmp(a, b).unwrap_or(Ordering::Equal))
}

fn compare_docs(a: &Map<String, Value>, b: &Map<String, Value>, sort: &[SortKey]) -> Ordering {
    for key in sort {
        let va = a.get(&key.field).unwrap_or(&Value::Null);
        let vb = b.get(&key.field).unwrap_or(&Value::Null);
        let ord = sort_cmp(va, vb);
        let ord = if key.direction == Direction::Desc { ord.reverse() } else { ord };
        if ord != Ordering::Equal {
            return ord;
        }
    }
    Ordering::Equal
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn doc(v: Value) -> Map<String, Value> {
        v.as_object().unwrap().clone()
    }

    #[test]
    fn missing_fields_are_null() {
        let d = doc(json!({"a": 1}));
        assert!(matches(&Filter::cmp("b", Op::Eq, Value::Null), &d));
        assert!(matches(&Filter::cmp("b", Op::Ne, 3), &d));
        assert!(!matches(&Filter::cmp("b", Op::Lt, 3), &d));
    }

    #[test]
    fn numbers_compare_across_int_and_float() {
        let d = doc(json!({"a": 1}));
        assert!(matches(&Filter::cmp("a", Op::Eq, 1.0), &d));
        assert!(matches(&Filter::cmp("a", Op::Lt, 1.5), &d));
        assert!(matches(&Filter::cmp("a", Op::In, json!([0, 1])), &d));
    }

    #[test]
    fn boolean_combinators() {
        let d = doc(json!({"a": "hello", "n": 5}));
        let f = Filter::And(vec![
            Filter::cmp("a", Op::StartsWith, "he"),
            Filter::Not(Box::new(Filter::cmp("n", Op::Gt, 10))),
            Filter::Or(vec![Filter::cmp("n", Op::Eq, 5), Filter::cmp("n", Op::Eq, 6)]),
        ]);
        assert!(matches(&f, &d));
    }
}
