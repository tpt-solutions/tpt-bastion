// SPDX-License-Identifier: Apache-2.0 OR MIT

//! # bastion-dsl
//!
//! The Bastion configuration language: a type-safe, declarative
//! Infrastructure-as-Code DSL that compiles to the stable
//! [`bastion_ir`] intermediate representation.
//!
//! The pipeline is three stages:
//!
//! 1. [`lex`](lex::tokenize) — source text into tokens
//! 2. [`parse`](parse::parse) — tokens into an [`ast::Ast`]
//! 3. [`compile`](compile::compile) — AST into a [`bastion_ir::Document`]
//!
//! Most callers only need [`compile_source`], which runs all three stages.
//!
//! ## Example
//!
//! ```rust
//! use bastion_dsl::compile_source;
//! use bastion_ir::is_compatible;
//!
//! let src = r#"
//!     config { name = "demo" region = "us-east-1" }
//!     resource aws_s3_bucket demo {
//!         bucket = "demo-bucket"
//!     }
//! "#;
//! let doc = compile_source(src).unwrap();
//! assert!(is_compatible(doc.ir_version));
//! assert_eq!(doc.resources.len(), 1);
//! ```

pub mod ast;
pub mod compiler;
pub mod error;
pub mod lexer;
pub mod parser;

pub use ast::{Ast, Block, Expr};
pub use error::DslError;
pub use lexer::{tokenize, Token, TokenKind};
pub use parser::parse;

use bastion_ir::Document;

/// Run the full pipeline: source text -> IR document.
///
/// This is the convenience entry point used by the CLI and language bindings.
pub fn compile_source(source: &str) -> Result<Document, DslError> {
    let tokens = tokenize(source)?;
    let ast = parse(tokens)?;
    compiler::compile(&ast)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compiles_resource_with_reference() {
        let src = r#"
            config { name = "demo" region = "us-east-1" }

            variable region {
                type = "string"
                default = "us-east-1"
            }

            resource aws_s3_bucket my_bucket {
                bucket = "my-unique-bucket"
                acl = "private"
                tags {
                    environment = "dev"
                    team = "platform"
                }
                versioning = true
                count = 3
            }

            output bucket_name {
                value = my_bucket.bucket
            }
        "#;
        let doc = compile_source(src).unwrap();
        assert_eq!(doc.config.as_ref().unwrap().name.as_deref(), Some("demo"));
        assert_eq!(doc.variables.len(), 1);
        assert_eq!(doc.resources.len(), 1);

        let res = &doc.resources[0];
        assert_eq!(res.id, "aws_s3_bucket.my_bucket");
        assert_eq!(
            res.attributes.get("bucket"),
            Some(&bastion_ir::Value::String("my-unique-bucket".into()))
        );
        assert_eq!(
            res.attributes.get("versioning"),
            Some(&bastion_ir::Value::Bool(true))
        );

        let tags = res.attributes.get("tags").unwrap();
        match tags {
            bastion_ir::Value::Map(m) => {
                assert_eq!(
                    m.get("environment"),
                    Some(&bastion_ir::Value::String("dev".into()))
                );
            }
            _ => panic!("tags should be a map"),
        }

        let out = &doc.outputs[0];
        assert_eq!(
            out.value,
            bastion_ir::Value::Reference {
                base: "my_bucket".into(),
                path: vec!["bucket".into()],
            }
        );
    }

    #[test]
    fn rejects_duplicate_resource() {
        let src = r#"
            resource aws_s3_bucket a { bucket = "x" }
            resource aws_s3_bucket a { bucket = "y" }
        "#;
        assert!(compile_source(src).is_err());
    }

    #[test]
    fn rejects_unknown_block() {
        let src = "weirdthing { }";
        assert!(compile_source(src).is_err());
    }

    #[test]
    fn reports_lex_error_with_location() {
        let src = "resource aws_s3_bucket x { bucket = \"unterminated }";
        let err = compile_source(src).unwrap_err();
        assert!(matches!(err, DslError::Lex { .. }));
    }
}
