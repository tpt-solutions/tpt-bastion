// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Abstract syntax tree for the Bastion DSL.

/// An expression value in the DSL.
///
/// References are represented structurally so the compiler can translate them
/// into IR reference values without loss of information.
#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    /// A UTF-8 string literal.
    String(String),
    /// A numeric literal.
    Number(f64),
    /// A boolean literal.
    Bool(bool),
    /// The `null` literal.
    Null,
    /// A reference to another object's attribute, e.g. `my_bucket.bucket`.
    Reference {
        /// Base identifier.
        base: String,
        /// Attribute chain after the base, e.g. `["bucket"]`.
        path: Vec<String>,
        /// 1-based line of the base identifier.
        line: usize,
        /// 1-based column of the base identifier.
        col: usize,
    },
    /// An ordered list literal.
    List(Vec<Expr>),
    /// An object/map literal (key order is preserved from source).
    Map(Vec<(String, Expr)>),
}

/// A top-level block (`resource`, `module`, `variable`, `output`, `config`).
#[derive(Debug, Clone, PartialEq)]
pub struct Block {
    /// Block kind, e.g. `"resource"`.
    pub kind: String,
    /// 1-based line of the block keyword.
    pub line: usize,
    /// 1-based column of the block keyword.
    pub col: usize,
    /// Optional type label (used by `resource`, e.g. `aws_s3_bucket`).
    pub type_name: Option<String>,
    /// Optional name label (the local name of the object).
    pub name: Option<String>,
    /// Ordered list of `key = value` and nested `key { ... }` attributes.
    pub attributes: Vec<(String, Expr)>,
}

/// A parsed Bastion document.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Ast {
    /// All top-level blocks in source order.
    pub blocks: Vec<Block>,
}
