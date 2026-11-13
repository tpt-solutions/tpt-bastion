// SPDX-License-Identifier: Apache-2.0 OR MIT

//! # tpt-bastion-dsl
//!
//! The Bastion configuration language: a type-safe, declarative
//! Infrastructure-as-Code DSL that compiles to the stable
//! [`tpt_bastion_ir`] intermediate representation.
//!
//! The pipeline is three stages:
//!
//! 1. [`lex`](lex::tokenize) — source text into tokens
//! 2. [`parse`](parse::parse) — tokens into an [`ast::Ast`]
//! 3. [`compile`](compile::compile) — AST into a [`tpt_bastion_ir::Document`]
//!
//! Most callers only need [`compile_source`], which runs all three stages.
//!
//! ## Example
//!
//! ```rust
//! use tpt_bastion_dsl::compile_source;
//! use tpt_bastion_ir::is_compatible;
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
pub mod format;
pub mod lexer;
pub mod parser;

pub use ast::{Ast, Block, Expr};
pub use error::DslError;
pub use lexer::{tokenize, Token, TokenKind};
pub use parser::parse;

use tpt_bastion_ir::Document;

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
            Some(&tpt_bastion_ir::Value::String("my-unique-bucket".into()))
        );
        assert_eq!(
            res.attributes.get("versioning"),
            Some(&tpt_bastion_ir::Value::Bool(true))
        );

        let tags = res.attributes.get("tags").unwrap();
        match tags {
            tpt_bastion_ir::Value::Map(m) => {
                assert_eq!(
                    m.get("environment"),
                    Some(&tpt_bastion_ir::Value::String("dev".into()))
                );
            }
            _ => panic!("tags should be a map"),
        }

        let out = &doc.outputs[0];
        assert_eq!(
            out.value,
            tpt_bastion_ir::Value::Reference {
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
    fn rejects_reference_to_unknown_resource_at_compile_time() {
        let src = r#"
            resource aws_s3_bucket a { bucket = nope.bucket }
        "#;
        let err = compile_source(src).unwrap_err();
        assert!(err.to_string().contains("unknown `nope`"), "{err}");
    }

    #[test]
    fn rejects_reference_to_variable_with_a_clear_message() {
        let src = r#"
            variable region { default = "us-east-1" }
            resource aws_s3_bucket a { bucket = region.value }
        "#;
        let err = compile_source(src).unwrap_err();
        assert!(
            err.to_string().contains("variables cannot be referenced"),
            "{err}"
        );
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
