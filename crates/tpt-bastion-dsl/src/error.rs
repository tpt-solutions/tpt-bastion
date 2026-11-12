// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Compile-time error types for the Bastion DSL toolchain.

use thiserror::Error;

/// Errors produced while compiling a Bastion document.
///
/// Errors are split by phase so the CLI can present the most relevant
/// diagnostic (a lexer error points at the offending character; a parser
/// error points at the unexpected token; a compile error describes a
/// structural/semantic problem such as a duplicate resource id).
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum DslError {
    /// Raised while turning source text into tokens.
    #[error("lex error at {line}:{col}: {message}")]
    Lex {
        /// 1-based line number.
        line: usize,
        /// 1-based column number.
        col: usize,
        /// Human-readable cause.
        message: String,
    },

    /// Raised while turning tokens into an AST.
    #[error("parse error at {line}:{col}: {message}")]
    Parse {
        /// 1-based line number.
        line: usize,
        /// 1-based column number.
        col: usize,
        /// Human-readable cause.
        message: String,
    },

    /// Raised while lowering the AST to IR.
    #[error("compile error: {message}")]
    Compile {
        /// Human-readable cause.
        message: String,
    },
}
