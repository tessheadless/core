// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

//! The only engine-specific SQL: reading typed values out of the JSON
//! documents. Everything else is built with SeaQuery.
//!
//! All accessors are expressed in a canonical form so the filter compiler is
//! engine-agnostic:
//! - [`Dialect::type_of`] yields `'null' | 'number' | 'string' | 'boolean' |
//!   'array' | 'object'`, or SQL `NULL` when the field is missing;
//! - numbers compare as double precision (like the reference evaluator);
//! - strings compare bytewise, whatever the database's default collation;
//! - booleans are `0` / `1`.

use sea_query::{Expr, Value};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dialect {
    Postgres,
    MySql,
    Sqlite,
}

/// A raw SQL fragment with its bound values, using the placeholder syntax of
/// the dialect (`$N` for Postgres, `?` otherwise).
pub(crate) struct Frag {
    pub dialect: Dialect,
    sql: String,
    values: Vec<Value>,
}

impl Frag {
    pub fn new(dialect: Dialect) -> Self {
        Frag {
            dialect,
            sql: String::new(),
            values: Vec::new(),
        }
    }

    pub fn push(&mut self, sql: &str) -> &mut Self {
        self.sql.push_str(sql);
        self
    }

    pub fn bind(&mut self, value: impl Into<Value>) -> &mut Self {
        self.values.push(value.into());
        match self.dialect {
            Dialect::Postgres => {
                let n = self.values.len();
                self.sql.push_str(&format!("${n}"));
            }
            _ => self.sql.push('?'),
        }
        self
    }

    pub fn into_expr(self) -> Expr {
        Expr::cust_with_values(self.sql, self.values)
    }
}

impl Dialect {
    pub fn from_url(url: &str) -> Option<Self> {
        let scheme = url.split(':').next()?.to_ascii_lowercase();
        match scheme.as_str() {
            "postgres" | "postgresql" => Some(Dialect::Postgres),
            "mysql" | "mariadb" => Some(Dialect::MySql),
            "sqlite" => Some(Dialect::Sqlite),
            _ => None,
        }
    }

    /// Binds the location of `field` inside a JSON document.
    fn bind_path(self, f: &mut Frag, field: &str) {
        match self {
            Dialect::Postgres => {
                f.push("CAST(").bind(field.to_string()).push(" AS text)");
            }
            // Field names are validated alphanumeric; quoting the key keeps
            // the path valid for any of them anyway.
            _ => {
                f.bind(format!("$.\"{field}\""));
            }
        }
    }

    pub(crate) fn type_of(self, f: &mut Frag, col: &str, field: &str) {
        match self {
            Dialect::Postgres => {
                f.push(&format!("jsonb_typeof({col} -> "));
                self.bind_path(f, field);
                f.push(")");
            }
            Dialect::MySql => {
                f.push(&format!("(CASE JSON_TYPE(JSON_EXTRACT({col}, "));
                self.bind_path(f, field);
                f.push(
                    ")) WHEN 'INTEGER' THEN 'number' WHEN 'UNSIGNED INTEGER' THEN 'number' \
                     WHEN 'DOUBLE' THEN 'number' WHEN 'DECIMAL' THEN 'number' WHEN 'STRING' THEN 'string' \
                     WHEN 'BOOLEAN' THEN 'boolean' WHEN 'NULL' THEN 'null' WHEN 'ARRAY' THEN 'array' \
                     WHEN 'OBJECT' THEN 'object' END)",
                );
            }
            Dialect::Sqlite => {
                f.push(&format!("(CASE json_type({col}, "));
                self.bind_path(f, field);
                f.push(
                    ") WHEN 'integer' THEN 'number' WHEN 'real' THEN 'number' WHEN 'text' THEN 'string' \
                     WHEN 'true' THEN 'boolean' WHEN 'false' THEN 'boolean' WHEN 'null' THEN 'null' \
                     WHEN 'array' THEN 'array' WHEN 'object' THEN 'object' END)",
                );
            }
        }
    }

    /// Numeric value; only meaningful when the type is `number`.
    pub(crate) fn number(self, f: &mut Frag, col: &str, field: &str) {
        match self {
            Dialect::Postgres => {
                f.push(&format!("CAST(({col} ->> "));
                self.bind_path(f, field);
                f.push(") AS double precision)");
            }
            Dialect::MySql => {
                f.push(&format!("CAST(JSON_EXTRACT({col}, "));
                self.bind_path(f, field);
                f.push(") AS DOUBLE)");
            }
            Dialect::Sqlite => {
                f.push(&format!("CAST(json_extract({col}, "));
                self.bind_path(f, field);
                f.push(") AS REAL)");
            }
        }
    }

    /// String value with bytewise collation; only meaningful for `string`.
    pub(crate) fn text(self, f: &mut Frag, col: &str, field: &str) {
        match self {
            Dialect::Postgres => {
                f.push(&format!("(({col} ->> "));
                self.bind_path(f, field);
                f.push(") COLLATE \"C\")");
            }
            Dialect::MySql => {
                f.push(&format!("(JSON_UNQUOTE(JSON_EXTRACT({col}, "));
                self.bind_path(f, field);
                f.push(")) COLLATE utf8mb4_bin)");
            }
            Dialect::Sqlite => {
                f.push(&format!("json_extract({col}, "));
                self.bind_path(f, field);
                f.push(")");
            }
        }
    }

    /// `1` for JSON `true`, `0` otherwise.
    pub(crate) fn boolean(self, f: &mut Frag, col: &str, field: &str) {
        match self {
            Dialect::Postgres => {
                f.push(&format!("(CASE WHEN ({col} -> "));
                self.bind_path(f, field);
                f.push(") = 'true'::jsonb THEN 1 ELSE 0 END)");
            }
            Dialect::MySql => {
                f.push(&format!("(CASE WHEN JSON_EXTRACT({col}, "));
                self.bind_path(f, field);
                f.push(") = CAST('true' AS JSON) THEN 1 ELSE 0 END)");
            }
            Dialect::Sqlite => {
                f.push(&format!("(CASE WHEN json_type({col}, "));
                self.bind_path(f, field);
                f.push(") = 'true' THEN 1 ELSE 0 END)");
            }
        }
    }

    /// A bound string, collated like [`Self::text`].
    pub(crate) fn bind_text(self, f: &mut Frag, s: &str) {
        match self {
            Dialect::Postgres => {
                f.push("(CAST(")
                    .bind(s.to_string())
                    .push(" AS text) COLLATE \"C\")");
            }
            Dialect::MySql => {
                f.push("(CONVERT(")
                    .bind(s.to_string())
                    .push(" USING utf8mb4) COLLATE utf8mb4_bin)");
            }
            Dialect::Sqlite => {
                f.bind(s.to_string());
            }
        }
    }

    pub(crate) fn char_length(self) -> &'static str {
        match self {
            Dialect::Sqlite => "length",
            _ => "char_length",
        }
    }

    pub(crate) fn instr(self) -> &'static str {
        match self {
            Dialect::Postgres => "strpos",
            _ => "instr",
        }
    }

    /// `text(field)` ends with `needle` (non-empty).
    pub(crate) fn ends_with(self, f: &mut Frag, col: &str, field: &str, needle: &str) {
        match self {
            Dialect::Sqlite => {
                f.push("substr(");
                self.text(f, col, field);
                f.push(", -length(");
                f.bind(needle.to_string());
                f.push(")) = ");
                self.bind_text(f, needle);
            }
            _ => {
                f.push("right(");
                self.text(f, col, field);
                f.push(&format!(", {}(", self.char_length()));
                self.bind_text(f, needle);
                f.push(")) = ");
                self.bind_text(f, needle);
            }
        }
    }
}
