// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

//! Compiles the backend-neutral [`Filter`] and sort keys to SQL. Every
//! comparison is guarded by the JSON type of the field, so its result matches
//! `tess_core::query::execute_in_memory` exactly (checked by the testkit).

use tess_core::query::{Direction, Filter, Op, SortKey};
use sea_query::{Expr, Order};
use serde_json::Value;

use crate::dialect::{Dialect, Frag};

const FALSE: &str = "(1=0)";
const TRUE: &str = "(1=1)";

pub(crate) fn where_expr(dialect: Dialect, col: &str, filter: &Filter) -> Expr {
    let mut f = Frag::new(dialect);
    push_filter(&mut f, col, filter);
    f.into_expr()
}

/// Pushes a boolean expression that is never NULL.
fn push_filter(f: &mut Frag, col: &str, filter: &Filter) {
    match filter {
        Filter::And(fs) | Filter::Or(fs) if fs.is_empty() => {
            f.push(if matches!(filter, Filter::And(_)) { TRUE } else { FALSE });
        }
        Filter::And(fs) | Filter::Or(fs) => {
            let sep = if matches!(filter, Filter::And(_)) { " AND " } else { " OR " };
            f.push("(");
            for (i, sub) in fs.iter().enumerate() {
                if i > 0 {
                    f.push(sep);
                }
                push_filter(f, col, sub);
            }
            f.push(")");
        }
        Filter::Not(sub) => {
            f.push("(NOT ");
            push_filter(f, col, sub);
            f.push(")");
        }
        Filter::Cmp { field, op, value } => {
            f.push("COALESCE(");
            push_cmp(f, col, field, *op, value);
            f.push(&format!(", {FALSE})"));
        }
    }
}

/// `CASE WHEN type = '<ty>' THEN <then> ELSE false END`
fn guarded(f: &mut Frag, col: &str, field: &str, ty: &str, then: impl FnOnce(&mut Frag)) {
    f.push("(CASE WHEN ");
    f.dialect.type_of(f, col, field);
    f.push(&format!(" = '{ty}' THEN "));
    then(f);
    f.push(&format!(" ELSE {FALSE} END)"));
}

fn push_eq(f: &mut Frag, col: &str, field: &str, value: &Value) {
    let d = f.dialect;
    match value {
        Value::Null => {
            f.push("(");
            d.type_of(f, col, field);
            f.push(" IS NULL OR ");
            d.type_of(f, col, field);
            f.push(" = 'null')");
        }
        Value::Number(n) => {
            let n = n.as_f64().unwrap_or(f64::NAN);
            guarded(f, col, field, "number", |f| {
                d.number(f, col, field);
                f.push(" = ").bind(n);
            });
        }
        Value::String(s) => guarded(f, col, field, "string", |f| {
            d.text(f, col, field);
            f.push(" = ");
            d.bind_text(f, s);
        }),
        Value::Bool(b) => guarded(f, col, field, "boolean", |f| {
            d.boolean(f, col, field);
            f.push(" = ").bind(i32::from(*b));
        }),
        // Arrays/objects are not filterable through the API.
        Value::Array(_) | Value::Object(_) => {
            f.push(FALSE);
        }
    }
}

fn push_in(f: &mut Frag, col: &str, field: &str, values: &[Value]) {
    if values.is_empty() {
        f.push(FALSE);
        return;
    }
    f.push("(");
    for (i, v) in values.iter().enumerate() {
        if i > 0 {
            f.push(" OR ");
        }
        push_eq(f, col, field, v);
    }
    f.push(")");
}

fn push_cmp(f: &mut Frag, col: &str, field: &str, op: Op, value: &Value) {
    let d = f.dialect;
    match op {
        Op::Eq => push_eq(f, col, field, value),
        Op::Ne => {
            f.push("(NOT COALESCE(");
            push_eq(f, col, field, value);
            f.push(&format!(", {FALSE}))"));
        }
        Op::In | Op::NotIn => {
            let Some(values) = value.as_array() else {
                f.push(if op == Op::In { FALSE } else { TRUE });
                return;
            };
            if op == Op::NotIn {
                f.push("(NOT COALESCE(");
                push_in(f, col, field, values);
                f.push(&format!(", {FALSE}))"));
            } else {
                push_in(f, col, field, values);
            }
        }
        Op::Lt | Op::Lte | Op::Gt | Op::Gte => {
            let sql_op = match op {
                Op::Lt => " < ",
                Op::Lte => " <= ",
                Op::Gt => " > ",
                _ => " >= ",
            };
            match value {
                Value::Number(n) => {
                    let n = n.as_f64().unwrap_or(f64::NAN);
                    guarded(f, col, field, "number", |f| {
                        d.number(f, col, field);
                        f.push(sql_op).bind(n);
                    });
                }
                Value::String(s) => guarded(f, col, field, "string", |f| {
                    d.text(f, col, field);
                    f.push(sql_op);
                    d.bind_text(f, s);
                }),
                Value::Bool(b) => guarded(f, col, field, "boolean", |f| {
                    d.boolean(f, col, field);
                    f.push(sql_op).bind(i32::from(*b));
                }),
                // Range comparisons never match null, arrays or objects.
                _ => {
                    f.push(FALSE);
                }
            }
        }
        Op::Contains | Op::StartsWith | Op::EndsWith => {
            let Value::String(needle) = value else {
                f.push(FALSE);
                return;
            };
            guarded(f, col, field, "string", |f| {
                if needle.is_empty() {
                    f.push(TRUE);
                    return;
                }
                match op {
                    Op::Contains => {
                        f.push(&format!("{}(", d.instr()));
                        d.text(f, col, field);
                        f.push(", ");
                        d.bind_text(f, needle);
                        f.push(") > 0");
                    }
                    Op::StartsWith => {
                        f.push("substr(");
                        d.text(f, col, field);
                        f.push(&format!(", 1, {}(", d.char_length()));
                        d.bind_text(f, needle);
                        f.push(")) = ");
                        d.bind_text(f, needle);
                    }
                    _ => d.ends_with(f, col, field, needle),
                }
            });
        }
    }
}

/// ORDER BY expressions reproducing the in-memory total order:
/// missing/null < numbers < strings (bytewise) < booleans < arrays < objects.
pub(crate) fn order_exprs(dialect: Dialect, col: &str, keys: &[SortKey]) -> Vec<(Expr, Order)> {
    let mut out = Vec::new();
    for key in keys {
        let order = if key.direction == Direction::Asc { Order::Asc } else { Order::Desc };
        let field = key.field.as_str();

        let mut rank = Frag::new(dialect);
        rank.push("(CASE ");
        dialect.type_of(&mut rank, col, field);
        rank.push(
            " WHEN 'number' THEN 1 WHEN 'string' THEN 2 WHEN 'boolean' THEN 3 \
             WHEN 'array' THEN 4 WHEN 'object' THEN 5 ELSE 0 END)",
        );
        out.push((rank.into_expr(), order.clone()));

        type Accessor = fn(Dialect, &mut Frag, &str, &str);
        let accessors: [(&str, Accessor); 3] =
            [("number", Dialect::number), ("string", Dialect::text), ("boolean", Dialect::boolean)];
        for (ty, accessor) in accessors {
            let mut e = Frag::new(dialect);
            e.push("(CASE WHEN ");
            dialect.type_of(&mut e, col, field);
            e.push(&format!(" = '{ty}' THEN "));
            accessor(dialect, &mut e, col, field);
            e.push(" END)");
            out.push((e.into_expr(), order.clone()));
        }
    }
    out
}
