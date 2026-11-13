// SPDX-License-Identifier: Apache-2.0 OR MIT

//! A parser for the common subset of HCL2, the language of Terraform `.tf` files.
//!
//! Supported: blocks with labels, `name = value` attributes, strings (with
//! escapes and heredocs), numbers, booleans, `null`, lists, objects, and
//! traversals such as `aws_s3_bucket.b.bucket`.
//!
//! Recognised but not supported: template interpolation (`${...}`) and function
//! calls. These parse into [`Value::Template`] and [`Value::Call`] so the converter
//! can report them, instead of failing the whole file.

use std::fmt;

/// A parse failure with its position in the source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HclError {
    /// 1-based line.
    pub line: usize,
    /// 1-based column.
    pub col: usize,
    /// What went wrong.
    pub message: String,
}

impl fmt::Display for HclError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}: {}", self.line, self.col, self.message)
    }
}

impl std::error::Error for HclError {}

/// A body: attributes and nested blocks, in source order.
#[derive(Debug, Clone, PartialEq)]
pub struct Body {
    /// The items in source order.
    pub items: Vec<Item>,
}

/// One entry in a body.
#[derive(Debug, Clone, PartialEq)]
pub enum Item {
    /// `name = value`.
    Attr(Attr),
    /// `kind "label" ... { body }`.
    Block(Block),
}

/// An attribute assignment.
#[derive(Debug, Clone, PartialEq)]
pub struct Attr {
    /// Attribute name.
    pub name: String,
    /// The value.
    pub value: Value,
    /// 1-based line of the name.
    pub line: usize,
}

/// A block with zero or more labels.
#[derive(Debug, Clone, PartialEq)]
pub struct Block {
    /// Block type, such as `resource`.
    pub kind: String,
    /// Labels, such as `["aws_s3_bucket", "b"]`.
    pub labels: Vec<String>,
    /// The block body.
    pub body: Body,
    /// 1-based line of the block type.
    pub line: usize,
}

/// An HCL expression.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// A string literal with no interpolation.
    Str(String),
    /// A string containing `${` or `%{`. Not supported; carries the raw text.
    Template(String),
    /// A number.
    Num(f64),
    /// A boolean.
    Bool(bool),
    /// `null`.
    Null,
    /// A list.
    List(Vec<Value>),
    /// An object, with keys in source order.
    Object(Vec<(String, Value)>),
    /// A traversal such as `aws_s3_bucket.b.bucket`.
    Ref(Vec<String>),
    /// A function call. Not supported; carries the function name.
    Call(String),
    /// Anything else (for example an operator). Carries a short description.
    Other(String),
}

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Ident(String),
    Str { text: String, template: bool },
    Num(f64),
    Newline,
    LBrace,
    RBrace,
    LBrack,
    RBrack,
    LParen,
    RParen,
    Equals,
    Comma,
    Dot,
    Other(char),
    Eof,
}

#[derive(Debug, Clone)]
struct Spanned {
    tok: Tok,
    line: usize,
    col: usize,
}

struct Lexer<'a> {
    chars: Vec<char>,
    pos: usize,
    line: usize,
    col: usize,
    _src: &'a str,
}

impl<'a> Lexer<'a> {
    fn new(src: &'a str) -> Self {
        Lexer {
            chars: src.chars().collect(),
            pos: 0,
            line: 1,
            col: 1,
            _src: src,
        }
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn peek2(&self) -> Option<char> {
        self.chars.get(self.pos + 1).copied()
    }

    fn bump(&mut self) -> Option<char> {
        let c = self.chars.get(self.pos).copied()?;
        self.pos += 1;
        if c == '\n' {
            self.line += 1;
            self.col = 1;
        } else {
            self.col += 1;
        }
        Some(c)
    }

    fn error(&self, line: usize, col: usize, message: impl Into<String>) -> HclError {
        HclError {
            line,
            col,
            message: message.into(),
        }
    }

    fn tokenize(mut self) -> Result<Vec<Spanned>, HclError> {
        let mut out = Vec::new();
        loop {
            // Skip spaces and comments, but not newlines (they separate attributes).
            match self.peek() {
                Some(' ' | '\t' | '\r') => {
                    self.bump();
                    continue;
                }
                Some('#') => {
                    self.skip_line();
                    continue;
                }
                Some('/') if self.peek2() == Some('/') => {
                    self.skip_line();
                    continue;
                }
                Some('/') if self.peek2() == Some('*') => {
                    let (line, col) = (self.line, self.col);
                    self.bump();
                    self.bump();
                    loop {
                        match self.bump() {
                            None => {
                                return Err(self.error(line, col, "unterminated block comment"))
                            }
                            Some('*') if self.peek() == Some('/') => {
                                self.bump();
                                break;
                            }
                            _ => {}
                        }
                    }
                    continue;
                }
                _ => {}
            }

            let (line, col) = (self.line, self.col);
            let Some(c) = self.peek() else {
                out.push(Spanned {
                    tok: Tok::Eof,
                    line,
                    col,
                });
                return Ok(out);
            };

            let tok = match c {
                '\n' => {
                    self.bump();
                    Tok::Newline
                }
                '"' => self.string(line, col)?,
                '<' if self.peek2() == Some('<') => self.heredoc(line, col)?,
                '{' => self.punct(Tok::LBrace),
                '}' => self.punct(Tok::RBrace),
                '[' => self.punct(Tok::LBrack),
                ']' => self.punct(Tok::RBrack),
                '(' => self.punct(Tok::LParen),
                ')' => self.punct(Tok::RParen),
                '=' => self.punct(Tok::Equals),
                ',' => self.punct(Tok::Comma),
                '.' => self.punct(Tok::Dot),
                c if c.is_ascii_digit() => self.number(line, col)?,
                c if is_ident_start(c) => Tok::Ident(self.ident()),
                other => {
                    self.bump();
                    Tok::Other(other)
                }
            };
            out.push(Spanned { tok, line, col });
        }
    }

    fn punct(&mut self, tok: Tok) -> Tok {
        self.bump();
        tok
    }

    fn skip_line(&mut self) {
        while let Some(c) = self.peek() {
            if c == '\n' {
                break;
            }
            self.bump();
        }
    }

    fn ident(&mut self) -> String {
        let mut s = String::new();
        while let Some(c) = self.peek() {
            if is_ident_char(c) {
                s.push(c);
                self.bump();
            } else {
                break;
            }
        }
        s
    }

    fn number(&mut self, line: usize, col: usize) -> Result<Tok, HclError> {
        let mut text = String::new();
        while let Some(c) = self.peek() {
            if c.is_ascii_digit() || c == '.' || c == 'e' || c == 'E' || c == '+' || c == '-' {
                // Only take `.` when a digit follows, so `1.` stays a number then a dot.
                if c == '.' && !self.peek2().is_some_and(|d| d.is_ascii_digit()) {
                    break;
                }
                text.push(c);
                self.bump();
            } else {
                break;
            }
        }
        text.parse::<f64>()
            .map(Tok::Num)
            .map_err(|_| self.error(line, col, format!("invalid number `{text}`")))
    }

    fn string(&mut self, line: usize, col: usize) -> Result<Tok, HclError> {
        self.bump(); // opening quote
        let mut text = String::new();
        let mut template = false;
        loop {
            match self.bump() {
                None | Some('\n') => {
                    return Err(self.error(line, col, "unterminated string literal"));
                }
                Some('"') => break,
                Some('\\') => match self.bump() {
                    Some('n') => text.push('\n'),
                    Some('t') => text.push('\t'),
                    Some('r') => text.push('\r'),
                    Some('"') => text.push('"'),
                    Some('\\') => text.push('\\'),
                    Some(other) => {
                        return Err(self.error(
                            line,
                            col,
                            format!("unsupported escape sequence `\\{other}`"),
                        ))
                    }
                    None => return Err(self.error(line, col, "unterminated string literal")),
                },
                Some('$') | Some('%') if self.peek() == Some('{') => {
                    template = true;
                    text.push(self.chars[self.pos - 1]);
                }
                Some(c) => text.push(c),
            }
        }
        Ok(Tok::Str { text, template })
    }

    /// `<<EOF` or `<<-EOF`: the text runs until a line holding only `EOF`.
    fn heredoc(&mut self, line: usize, col: usize) -> Result<Tok, HclError> {
        self.bump();
        self.bump();
        if self.peek() == Some('-') {
            self.bump();
        }
        let marker = self.ident();
        if marker.is_empty() {
            return Err(self.error(line, col, "heredoc needs a marker after `<<`"));
        }
        // Skip to the end of the opening line.
        while let Some(c) = self.bump() {
            if c == '\n' {
                break;
            }
        }
        let mut text = String::new();
        loop {
            let mut row = String::new();
            loop {
                match self.bump() {
                    None => {
                        return Err(self.error(
                            line,
                            col,
                            format!("heredoc `{marker}` is not closed"),
                        ))
                    }
                    Some('\n') => break,
                    Some(c) => row.push(c),
                }
            }
            if row.trim() == marker {
                break;
            }
            text.push_str(&row);
            text.push('\n');
        }
        let template = text.contains("${") || text.contains("%{");
        Ok(Tok::Str { text, template })
    }
}

fn is_ident_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_'
}

fn is_ident_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '-'
}

/// Parse a whole HCL file into its top-level body.
pub fn parse(src: &str) -> Result<Body, HclError> {
    let tokens = Lexer::new(src).tokenize()?;
    let mut parser = Parser { tokens, pos: 0 };
    let body = parser.body(true)?;
    Ok(body)
}

struct Parser {
    tokens: Vec<Spanned>,
    pos: usize,
}

impl Parser {
    fn peek(&self) -> &Spanned {
        &self.tokens[self.pos]
    }

    fn advance(&mut self) -> Spanned {
        let t = self.tokens[self.pos].clone();
        if self.pos + 1 < self.tokens.len() {
            self.pos += 1;
        }
        t
    }

    fn error_at(&self, t: &Spanned, message: impl Into<String>) -> HclError {
        HclError {
            line: t.line,
            col: t.col,
            message: message.into(),
        }
    }

    fn skip_newlines(&mut self) {
        while self.peek().tok == Tok::Newline {
            self.advance();
        }
    }

    /// Parse items until `}` (or end of file at the top level).
    fn body(&mut self, top_level: bool) -> Result<Body, HclError> {
        let mut items = Vec::new();
        loop {
            self.skip_newlines();
            let t = self.peek().clone();
            match &t.tok {
                Tok::Eof if top_level => break,
                Tok::Eof => return Err(self.error_at(&t, "expected `}` before end of file")),
                Tok::RBrace if !top_level => break,
                Tok::Ident(name) => {
                    let name = name.clone();
                    self.advance();
                    match self.peek().tok.clone() {
                        Tok::Equals => {
                            self.advance();
                            let value = self.value()?;
                            items.push(Item::Attr(Attr {
                                name,
                                value,
                                line: t.line,
                            }));
                        }
                        _ => {
                            let mut labels = Vec::new();
                            loop {
                                match self.peek().tok.clone() {
                                    Tok::Str { text, .. } => {
                                        self.advance();
                                        labels.push(text);
                                    }
                                    Tok::Ident(label) => {
                                        self.advance();
                                        labels.push(label);
                                    }
                                    _ => break,
                                }
                            }
                            let open = self.peek().clone();
                            if open.tok != Tok::LBrace {
                                return Err(self.error_at(
                                    &open,
                                    format!("expected `=` or `{{` after `{name}`"),
                                ));
                            }
                            self.advance();
                            let body = self.body(false)?;
                            self.advance(); // closing brace
                            items.push(Item::Block(Block {
                                kind: name,
                                labels,
                                body,
                                line: t.line,
                            }));
                        }
                    }
                }
                _ => {
                    return Err(self.error_at(&t, format!("unexpected {}", describe(&t.tok))));
                }
            }
        }
        Ok(Body { items })
    }

    fn value(&mut self) -> Result<Value, HclError> {
        let t = self.advance();
        match t.tok.clone() {
            Tok::Str { text, template } => Ok(if template {
                Value::Template(text)
            } else {
                Value::Str(text)
            }),
            Tok::Num(n) => Ok(Value::Num(n)),
            Tok::Other('-') => match self.advance().tok {
                Tok::Num(n) => Ok(Value::Num(-n)),
                _ => Err(self.error_at(&t, "expected a number after `-`")),
            },
            Tok::Ident(name) => match name.as_str() {
                "true" => Ok(Value::Bool(true)),
                "false" => Ok(Value::Bool(false)),
                "null" => Ok(Value::Null),
                _ => {
                    if self.peek().tok == Tok::LParen {
                        // A call: skip its arguments, since they are not supported.
                        let mut depth = 0usize;
                        loop {
                            let next = self.advance();
                            match next.tok {
                                Tok::LParen => depth += 1,
                                Tok::RParen => {
                                    depth -= 1;
                                    if depth == 0 {
                                        break;
                                    }
                                }
                                Tok::Eof => return Err(self.error_at(&next, "unclosed call")),
                                _ => {}
                            }
                        }
                        return Ok(Value::Call(name));
                    }
                    let mut path = vec![name];
                    while self.peek().tok == Tok::Dot {
                        self.advance();
                        match self.advance().tok {
                            Tok::Ident(seg) => path.push(seg),
                            _ => return Err(self.error_at(&t, "expected a name after `.`")),
                        }
                    }
                    Ok(Value::Ref(path))
                }
            },
            Tok::LBrack => {
                let mut items = Vec::new();
                loop {
                    self.skip_newlines();
                    if self.peek().tok == Tok::RBrack {
                        self.advance();
                        break;
                    }
                    items.push(self.value()?);
                    self.skip_newlines();
                    match self.peek().tok {
                        Tok::Comma => {
                            self.advance();
                        }
                        Tok::RBrack => {}
                        _ => {
                            let n = self.peek().clone();
                            return Err(self.error_at(&n, "expected `,` or `]` in list"));
                        }
                    }
                }
                Ok(Value::List(items))
            }
            Tok::LBrace => {
                let mut entries = Vec::new();
                loop {
                    self.skip_newlines();
                    if self.peek().tok == Tok::RBrace {
                        self.advance();
                        break;
                    }
                    let key = match self.advance().tok {
                        Tok::Ident(k) => k,
                        Tok::Str { text, .. } => text,
                        _ => return Err(self.error_at(&t, "expected a key in object")),
                    };
                    if self.advance().tok != Tok::Equals {
                        return Err(self.error_at(&t, "expected `=` after object key"));
                    }
                    entries.push((key, self.value()?));
                    self.skip_newlines();
                    if self.peek().tok == Tok::Comma {
                        self.advance();
                    }
                }
                Ok(Value::Object(entries))
            }
            other => {
                Err(self.error_at(&t, format!("expected a value, found {}", describe(&other))))
            }
        }
    }
}

fn describe(tok: &Tok) -> String {
    match tok {
        Tok::Ident(s) => format!("`{s}`"),
        Tok::Str { .. } => "a string".into(),
        Tok::Num(_) => "a number".into(),
        Tok::Newline => "end of line".into(),
        Tok::LBrace => "`{`".into(),
        Tok::RBrace => "`}`".into(),
        Tok::LBrack => "`[`".into(),
        Tok::RBrack => "`]`".into(),
        Tok::LParen => "`(`".into(),
        Tok::RParen => "`)`".into(),
        Tok::Equals => "`=`".into(),
        Tok::Comma => "`,`".into(),
        Tok::Dot => "`.`".into(),
        Tok::Other(c) => format!("`{c}`"),
        Tok::Eof => "end of file".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_blocks_attributes_and_references() {
        let body = parse(
            "resource \"aws_s3_bucket\" \"b\" {\n  bucket = \"x\"\n  tags = { env = \"dev\" }\n  acl = aws_s3_bucket.a.acl\n}\n",
        )
        .unwrap();
        let Item::Block(block) = &body.items[0] else {
            panic!("expected a block")
        };
        assert_eq!(block.kind, "resource");
        assert_eq!(block.labels, vec!["aws_s3_bucket", "b"]);
        assert_eq!(block.body.items.len(), 3);
        let Item::Attr(acl) = &block.body.items[2] else {
            panic!("expected an attribute")
        };
        assert_eq!(
            acl.value,
            Value::Ref(vec!["aws_s3_bucket".into(), "a".into(), "acl".into()])
        );
    }

    #[test]
    fn skips_comments_and_reads_numbers_bools_and_lists() {
        let body = parse(
            "# hash comment\n// line\n/* block\ncomment */\nvariable \"n\" {\n  count = 3\n  on = true\n  names = [\"a\", \"b\",]\n  ratio = -1.5\n}\n",
        )
        .unwrap();
        let Item::Block(block) = &body.items[0] else {
            panic!()
        };
        let values: Vec<&Value> = block
            .body
            .items
            .iter()
            .map(|i| match i {
                Item::Attr(a) => &a.value,
                _ => panic!(),
            })
            .collect();
        assert_eq!(values[0], &Value::Num(3.0));
        assert_eq!(values[1], &Value::Bool(true));
        assert_eq!(
            values[2],
            &Value::List(vec![Value::Str("a".into()), Value::Str("b".into())])
        );
        assert_eq!(values[3], &Value::Num(-1.5));
    }

    #[test]
    fn heredocs_and_templates_are_recognised() {
        let body = parse("a = <<EOF\nhello\nEOF\nb = \"x-${var.y}\"\nc = file(\"p\")\n").unwrap();
        let vals: Vec<Value> = body
            .items
            .iter()
            .map(|i| match i {
                Item::Attr(a) => a.value.clone(),
                _ => panic!(),
            })
            .collect();
        assert_eq!(vals[0], Value::Str("hello\n".into()));
        assert!(matches!(vals[1], Value::Template(_)));
        assert_eq!(vals[2], Value::Call("file".into()));
    }

    #[test]
    fn reports_position_of_a_syntax_error() {
        let err = parse("resource \"a\" \"b\" {\n  bucket = \n}\n").unwrap_err();
        // The value is missing on line 2, where the end of that line is found instead.
        assert_eq!(err.line, 2);
    }
}
