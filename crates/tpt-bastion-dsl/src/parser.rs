// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Recursive-descent parser: token stream -> AST.

use crate::ast::{Ast, Block, Expr};
use crate::error::DslError;
use crate::lexer::{Token, TokenKind};

/// Parse a token stream into an [`Ast`].
pub fn parse(tokens: Vec<Token>) -> Result<Ast, DslError> {
    let mut parser = Parser { tokens, pos: 0 };
    parser.parse_document()
}

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    fn peek(&self) -> &Token {
        &self.tokens[self.pos]
    }

    fn peek_kind(&self) -> &TokenKind {
        &self.tokens[self.pos].kind
    }

    fn advance(&mut self) -> Token {
        let tok = self.tokens[self.pos].clone();
        if self.pos < self.tokens.len() - 1 {
            self.pos += 1;
        }
        tok
    }

    fn at_eof(&self) -> bool {
        matches!(self.peek_kind(), TokenKind::Eof)
    }

    fn error(&self, tok: &Token, message: impl Into<String>) -> DslError {
        DslError::Parse {
            line: tok.line,
            col: tok.col,
            message: message.into(),
        }
    }

    fn expect(&mut self, kind: &TokenKind) -> Result<Token, DslError> {
        let tok = self.peek().clone();
        if std::mem::discriminant(&tok.kind) == std::mem::discriminant(kind) {
            self.advance();
            Ok(tok)
        } else {
            Err(self.error(&tok, format!("expected {}, found {}", kind, tok.kind)))
        }
    }

    fn expect_ident(&mut self) -> Result<String, DslError> {
        let tok = self.peek().clone();
        match &tok.kind {
            TokenKind::Ident(s) => {
                self.advance();
                Ok(s.clone())
            }
            _ => Err(self.error(&tok, format!("expected identifier, found {}", tok.kind))),
        }
    }

    fn parse_document(&mut self) -> Result<Ast, DslError> {
        let mut blocks = Vec::new();
        while !self.at_eof() {
            blocks.push(self.parse_block()?);
        }
        Ok(Ast { blocks })
    }

    fn parse_block(&mut self) -> Result<Block, DslError> {
        let kind = self.expect_ident()?;
        let known = matches!(
            kind.as_str(),
            "resource" | "module" | "variable" | "output" | "config"
        );
        if !known {
            let tok = self.tokens[self.pos - 1].clone();
            return Err(self.error(
                &tok,
                format!(
                    "unknown top-level block `{kind}` (expected one of: resource, module, variable, output, config)"
                ),
            ));
        }

        let mut type_name = None;
        let mut name = None;

        if kind == "resource" {
            type_name = Some(self.expect_ident()?);
            name = Some(self.expect_ident()?);
        } else if kind == "config" {
            // no labels
        } else {
            name = Some(self.expect_ident()?);
        }

        self.expect(&TokenKind::LeftBrace)?;
        let attributes = self.parse_body()?;
        self.expect(&TokenKind::RightBrace)?;

        Ok(Block {
            kind,
            type_name,
            name,
            attributes,
        })
    }

    /// Parse the attribute list inside `{ ... }`.
    ///
    /// Each entry is `key = value` or `key { ... }` (nested object). Entries
    /// are separated by optional `,` / `;` or simply whitespace.
    fn parse_body(&mut self) -> Result<Vec<(String, Expr)>, DslError> {
        let mut attrs = Vec::new();
        while !matches!(self.peek_kind(), TokenKind::RightBrace | TokenKind::Eof) {
            let key = self.expect_ident()?;
            let sep = self.peek().clone();
            match &sep.kind {
                TokenKind::Equals => {
                    self.advance();
                    let value = self.parse_expr()?;
                    attrs.push((key, value));
                }
                TokenKind::LeftBrace => {
                    self.advance();
                    let nested = self.parse_body()?;
                    self.expect(&TokenKind::RightBrace)?;
                    attrs.push((key, Expr::Map(nested)));
                }
                other => {
                    return Err(self.error(
                        &sep,
                        format!("expected `=` or `{{` after key `{key}`, found {other}"),
                    ));
                }
            }
            // Optional separator; whitespace already acts as a separator.
            if matches!(self.peek_kind(), TokenKind::Comma | TokenKind::Semicolon) {
                self.advance();
            }
        }
        Ok(attrs)
    }

    /// Parse a value expression.
    fn parse_expr(&mut self) -> Result<Expr, DslError> {
        let tok = self.peek().clone();
        match &tok.kind {
            TokenKind::String(s) => {
                self.advance();
                Ok(Expr::String(s.clone()))
            }
            TokenKind::Number(n) => {
                self.advance();
                Ok(Expr::Number(*n))
            }
            TokenKind::Ident(s) => {
                self.advance();
                match s.as_str() {
                    "true" => return Ok(Expr::Bool(true)),
                    "false" => return Ok(Expr::Bool(false)),
                    "null" => return Ok(Expr::Null),
                    _ => {}
                }
                // Could be a reference with attribute access.
                let base = s.clone();
                let mut path = Vec::new();
                while matches!(self.peek_kind(), TokenKind::Dot) {
                    self.advance();
                    path.push(self.expect_ident()?);
                }
                Ok(Expr::Reference { base, path })
            }
            TokenKind::LeftBracket => {
                self.advance();
                let mut items = Vec::new();
                while !matches!(self.peek_kind(), TokenKind::RightBracket | TokenKind::Eof) {
                    items.push(self.parse_expr()?);
                    if matches!(self.peek_kind(), TokenKind::Comma) {
                        self.advance();
                    }
                }
                self.expect(&TokenKind::RightBracket)?;
                Ok(Expr::List(items))
            }
            TokenKind::LeftBrace => {
                self.advance();
                let body = self.parse_body()?;
                self.expect(&TokenKind::RightBrace)?;
                Ok(Expr::Map(body))
            }
            other => Err(self.error(&tok, format!("expected expression, found {other}"))),
        }
    }
}
