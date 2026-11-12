// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Lexer: source text -> token stream.

use crate::error::DslError;
use std::fmt;

/// A single lexical token together with its source location.
#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    /// Token category and any carried payload.
    pub kind: TokenKind,
    /// 1-based line where the token starts.
    pub line: usize,
    /// 1-based column where the token starts.
    pub col: usize,
}

/// Categories of Bastion DSL tokens.
#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    /// An identifier or keyword.
    Ident(String),
    /// A double-quoted string literal.
    String(String),
    /// A numeric literal (integers and floats).
    Number(f64),
    /// `{`
    LeftBrace,
    /// `}`
    RightBrace,
    /// `(`
    LeftParen,
    /// `)`
    RightParen,
    /// `[`
    LeftBracket,
    /// `]`
    RightBracket,
    /// `,`
    Comma,
    /// `;`
    Semicolon,
    /// `=`
    Equals,
    /// `.`
    Dot,
    /// End of input.
    Eof,
}

impl fmt::Display for TokenKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TokenKind::Ident(s) => write!(f, "identifier `{s}`"),
            TokenKind::String(s) => write!(f, "string `{s}`"),
            TokenKind::Number(n) => write!(f, "number `{n}`"),
            TokenKind::LeftBrace => write!(f, "`{{`"),
            TokenKind::RightBrace => write!(f, "`}}`"),
            TokenKind::LeftParen => write!(f, "`(`"),
            TokenKind::RightParen => write!(f, "`)`"),
            TokenKind::LeftBracket => write!(f, "`[`"),
            TokenKind::RightBracket => write!(f, "`]`"),
            TokenKind::Comma => write!(f, "`,`"),
            TokenKind::Semicolon => write!(f, "`;`"),
            TokenKind::Equals => write!(f, "`=`"),
            TokenKind::Dot => write!(f, "`.`"),
            TokenKind::Eof => write!(f, "end of input"),
        }
    }
}

/// Tokenize `source` into a vector of tokens (including a trailing `Eof`).
pub fn tokenize(source: &str) -> Result<Vec<Token>, DslError> {
    let lexer = Lexer::new(source);
    lexer.run()
}

struct Lexer<'a> {
    chars: std::iter::Peekable<std::str::CharIndices<'a>>,
    line: usize,
    col: usize,
}

impl<'a> Lexer<'a> {
    fn new(src: &'a str) -> Self {
        Self {
            chars: src.char_indices().peekable(),
            line: 1,
            col: 1,
        }
    }

    fn peek(&mut self) -> Option<char> {
        self.chars.peek().map(|(_, c)| *c)
    }

    fn advance(&mut self) -> Option<char> {
        let (_, c) = self.chars.next()?;
        if c == '\n' {
            self.line += 1;
            self.col = 1;
        } else {
            self.col += 1;
        }
        Some(c)
    }

    fn error(&self, at_line: usize, at_col: usize, message: impl Into<String>) -> DslError {
        DslError::Lex {
            line: at_line,
            col: at_col,
            message: message.into(),
        }
    }

    fn run(mut self) -> Result<Vec<Token>, DslError> {
        let mut tokens = Vec::new();
        loop {
            self.skip_trivia()?;
            let line = self.line;
            let col = self.col;
            match self.peek() {
                None => {
                    tokens.push(Token {
                        kind: TokenKind::Eof,
                        line,
                        col,
                    });
                    break;
                }
                Some(c) if c.is_ascii_alphabetic() || c == '_' => {
                    let ident = self.read_ident();
                    tokens.push(Token {
                        kind: TokenKind::Ident(ident),
                        line,
                        col,
                    });
                }
                Some(c) if c.is_ascii_digit() => {
                    let value = self.read_number();
                    tokens.push(Token {
                        kind: TokenKind::Number(value),
                        line,
                        col,
                    });
                }
                Some('"') => {
                    let s = self.read_string()?;
                    tokens.push(Token {
                        kind: TokenKind::String(s),
                        line,
                        col,
                    });
                }
                Some('{') => {
                    self.advance();
                    tokens.push(Token {
                        kind: TokenKind::LeftBrace,
                        line,
                        col,
                    });
                }
                Some('}') => {
                    self.advance();
                    tokens.push(Token {
                        kind: TokenKind::RightBrace,
                        line,
                        col,
                    });
                }
                Some('(') => {
                    self.advance();
                    tokens.push(Token {
                        kind: TokenKind::LeftParen,
                        line,
                        col,
                    });
                }
                Some(')') => {
                    self.advance();
                    tokens.push(Token {
                        kind: TokenKind::RightParen,
                        line,
                        col,
                    });
                }
                Some('[') => {
                    self.advance();
                    tokens.push(Token {
                        kind: TokenKind::LeftBracket,
                        line,
                        col,
                    });
                }
                Some(']') => {
                    self.advance();
                    tokens.push(Token {
                        kind: TokenKind::RightBracket,
                        line,
                        col,
                    });
                }
                Some(',') => {
                    self.advance();
                    tokens.push(Token {
                        kind: TokenKind::Comma,
                        line,
                        col,
                    });
                }
                Some(';') => {
                    self.advance();
                    tokens.push(Token {
                        kind: TokenKind::Semicolon,
                        line,
                        col,
                    });
                }
                Some('=') => {
                    self.advance();
                    tokens.push(Token {
                        kind: TokenKind::Equals,
                        line,
                        col,
                    });
                }
                Some('.') => {
                    self.advance();
                    tokens.push(Token {
                        kind: TokenKind::Dot,
                        line,
                        col,
                    });
                }
                Some(other) => {
                    return Err(self.error(line, col, format!("unexpected character `{other}`")));
                }
            }
        }
        Ok(tokens)
    }

    fn skip_trivia(&mut self) -> Result<(), DslError> {
        loop {
            match self.peek() {
                Some(c) if c.is_whitespace() => {
                    self.advance();
                }
                Some('/') => {
                    // Look ahead for `//` or `/*`.
                    let start_line = self.line;
                    let start_col = self.col;
                    self.advance();
                    match self.peek() {
                        Some('/') => {
                            self.advance();
                            while let Some(c) = self.peek() {
                                if c == '\n' {
                                    break;
                                }
                                self.advance();
                            }
                        }
                        Some('*') => {
                            self.advance();
                            let mut closed = false;
                            while let Some(c) = self.advance() {
                                if c == '*' && self.peek() == Some('/') {
                                    self.advance();
                                    closed = true;
                                    break;
                                }
                            }
                            if !closed {
                                return Err(self.error(
                                    start_line,
                                    start_col,
                                    "unterminated block comment",
                                ));
                            }
                        }
                        _ => {
                            return Err(self.error(start_line, start_col, "unexpected `/`"));
                        }
                    }
                }
                _ => break,
            }
        }
        Ok(())
    }

    fn read_ident(&mut self) -> String {
        let mut s = String::new();
        while let Some(c) = self.peek() {
            if c.is_ascii_alphanumeric() || c == '_' {
                s.push(c);
                self.advance();
            } else {
                break;
            }
        }
        s
    }

    fn read_number(&mut self) -> f64 {
        let mut s = String::new();
        while let Some(c) = self.peek() {
            if c.is_ascii_digit() {
                s.push(c);
                self.advance();
            } else {
                break;
            }
        }
        if self.peek() == Some('.') {
            s.push('.');
            self.advance();
            while let Some(c) = self.peek() {
                if c.is_ascii_digit() {
                    s.push(c);
                    self.advance();
                } else {
                    break;
                }
            }
        }
        s.parse::<f64>().unwrap_or(0.0)
    }

    fn read_string(&mut self) -> Result<String, DslError> {
        let start_line = self.line;
        let start_col = self.col;
        self.advance(); // opening quote
        let mut s = String::new();
        loop {
            match self.advance() {
                None => {
                    return Err(self.error(start_line, start_col, "unterminated string literal"));
                }
                Some('"') => break,
                Some('\\') => match self.advance() {
                    Some('n') => s.push('\n'),
                    Some('t') => s.push('\t'),
                    Some('r') => s.push('\r'),
                    Some('"') => s.push('"'),
                    Some('\\') => s.push('\\'),
                    Some(other) => {
                        return Err(self.error(
                            start_line,
                            start_col,
                            format!("invalid escape sequence `\\{other}`"),
                        ));
                    }
                    None => {
                        return Err(self.error(
                            start_line,
                            start_col,
                            "unterminated string literal",
                        ));
                    }
                },
                Some(c) => s.push(c),
            }
        }
        Ok(s)
    }
}
