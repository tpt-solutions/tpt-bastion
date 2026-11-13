// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Canonical formatting for `.bcl` source.
//!
//! The formatter works on lines, not on the AST. The lexer discards comments,
//! so re-printing the AST would delete them. Instead, this module:
//!
//! - re-indents each line by brace/bracket/paren depth (four spaces per level),
//! - trims trailing whitespace on lines that are not inside a string,
//! - collapses runs of blank lines to one and drops blank lines at the start and
//!   end of the file,
//! - normalizes line endings to `\n` and ends the file with one newline.
//!
//! Lines that begin inside a multi-line string or a block comment are copied
//! unchanged, because their whitespace is content. Comments and token text are
//! never altered, so the formatted source compiles to the same IR.

/// Lexer state carried from one line to the next.
#[derive(Clone, Copy, PartialEq, Eq)]
enum State {
    Normal,
    Str,
    Block,
}

const INDENT: &str = "    ";

/// Return `src` formatted in canonical style. The result is idempotent:
/// formatting it again returns the same text.
pub fn format_source(src: &str) -> String {
    let normalized = src.replace("\r\n", "\n");
    let mut state = State::Normal;
    let mut depth: i64 = 0;
    let mut lines: Vec<String> = Vec::new();

    for raw in normalized.split('\n') {
        let starts_in = state;
        let trimmed = raw.trim_start();

        // A line that starts with a closer is indented one level less.
        let indent_depth = if starts_in == State::Normal
            && (trimmed.starts_with('}') || trimmed.starts_with(']') || trimmed.starts_with(')'))
        {
            (depth - 1).max(0)
        } else {
            depth.max(0)
        };

        // Scan the line to update the lexer state and the nesting depth.
        let mut chars = raw.chars().peekable();
        while let Some(c) = chars.next() {
            match state {
                State::Normal => match c {
                    '"' => state = State::Str,
                    '/' if chars.peek() == Some(&'/') => break,
                    '/' if chars.peek() == Some(&'*') => {
                        chars.next();
                        state = State::Block;
                    }
                    '{' | '[' | '(' => depth += 1,
                    '}' | ']' | ')' => depth -= 1,
                    _ => {}
                },
                State::Str => match c {
                    '\\' => {
                        chars.next();
                    }
                    '"' => state = State::Normal,
                    _ => {}
                },
                State::Block => {
                    if c == '*' && chars.peek() == Some(&'/') {
                        chars.next();
                        state = State::Normal;
                    }
                }
            }
        }

        if starts_in != State::Normal {
            // Inside a string or block comment: the text is content, keep it.
            lines.push(raw.to_string());
            continue;
        }
        if trimmed.is_empty() {
            // Collapse blank runs, and drop blank lines right after an opener.
            // The leading blank run is dropped below.
            let after_opener = lines
                .iter()
                .rev()
                .find(|l| !l.trim().is_empty())
                .is_some_and(|l| l.trim_end().ends_with(['{', '[', '(']));
            if lines.last().is_some_and(|l| !l.is_empty()) && !after_opener {
                lines.push(String::new());
            }
            continue;
        }
        if starts_in == State::Normal
            && (trimmed.starts_with('}') || trimmed.starts_with(']') || trimmed.starts_with(')'))
        {
            // No blank line directly before a closer.
            while lines.last().is_some_and(|l| l.is_empty()) {
                lines.pop();
            }
        }

        let body = if state == State::Normal {
            trimmed.trim_end()
        } else {
            // The line ends inside a string or block comment: trailing spaces
            // there are content, so keep them.
            trimmed
        };
        lines.push(format!("{}{}", INDENT.repeat(indent_depth as usize), body));
    }

    // Drop trailing blank lines, then end the file with a single newline.
    while lines.last().is_some_and(|l| l.is_empty()) {
        lines.pop();
    }
    if lines.is_empty() {
        return String::new();
    }
    let mut out = lines.join("\n");
    out.push('\n');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compile_source;
    use crate::lexer::tokenize;

    fn kinds(src: &str) -> Vec<crate::lexer::TokenKind> {
        tokenize(src).unwrap().into_iter().map(|t| t.kind).collect()
    }

    #[test]
    fn reindents_nested_blocks() {
        let input =
            "resource aws_s3_bucket b {\nbucket = \"x\"\n   tags {\n  env = \"dev\"\n}\n}\n";
        let expected =
            "resource aws_s3_bucket b {\n    bucket = \"x\"\n    tags {\n        env = \"dev\"\n    }\n}\n";
        assert_eq!(format_source(input), expected);
    }

    #[test]
    fn keeps_comments_untouched() {
        let input =
            "// top\nconfig {\n      name = \"a\" // trailing\n   /* block\n   comment */\n}\n";
        let out = format_source(input);
        assert!(out.contains("// top\n"));
        assert!(out.contains("    name = \"a\" // trailing\n"));
        assert!(out.contains("/* block\n   comment */"));
    }

    #[test]
    fn collapses_blank_lines_and_trims_edges() {
        let input = "\n\nconfig {\n}\n\n\n\nvariable v {\n}\n\n";
        assert_eq!(format_source(input), "config {\n}\n\nvariable v {\n}\n");
    }

    #[test]
    fn drops_blank_lines_next_to_braces() {
        let input = "config {\n\n  a = 1\n\n}\n";
        assert_eq!(format_source(input), "config {\n    a = 1\n}\n");
    }

    #[test]
    fn preserves_multiline_string_content() {
        let input = "config {\n  motd = \"line one   \n      line two\"\n}\n";
        let out = format_source(input);
        // Whitespace inside the string is content and must survive exactly.
        assert!(out.contains("motd = \"line one   \n      line two\""));
    }

    #[test]
    fn is_idempotent_and_preserves_tokens() {
        let src = include_str!("../../../examples/aws_s3.bcl");
        let once = format_source(src);
        assert_eq!(format_source(&once), once);
        assert_eq!(kinds(src), kinds(&once));
    }

    #[test]
    fn formatted_source_compiles_to_the_same_ir() {
        let src = include_str!("../../../examples/aws_s3.bcl");
        let formatted = format_source(src);
        let a = compile_source(src).unwrap();
        let b = compile_source(&formatted).unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn empty_input_stays_empty() {
        assert_eq!(format_source(""), "");
        assert_eq!(format_source("\n\n  \n"), "");
    }
}
