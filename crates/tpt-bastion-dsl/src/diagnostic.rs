// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Human-readable diagnostics: the message, the location, and the offending
//! source line with a caret under the identifier that caused the error.

use crate::error::DslError;

/// Render `err` for a terminal.
///
/// `source` is the text that was compiled and `file` is the name shown to the
/// user (use `<stdin>` for standard input). Errors without a location are
/// rendered as a plain message.
pub fn render(source: &str, file: &str, err: &DslError) -> String {
    let Some((line, col)) = err.location() else {
        return format!("error: {}\n  --> {file}\n", err.message());
    };

    let mut out = format!("error: {}\n", err.message());
    out.push_str(&format!("  --> {file}:{line}:{col}\n"));

    let Some(text) = source.lines().nth(line.saturating_sub(1)) else {
        return out;
    };
    let gutter = line.to_string();
    let pad = " ".repeat(gutter.len());
    out.push_str(&format!("{pad} |\n"));
    out.push_str(&format!("{gutter} | {text}\n"));

    // Underline the identifier that starts at the reported column.
    let start = col.saturating_sub(1);
    let width = text
        .chars()
        .skip(start)
        .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '.')
        .count()
        .max(1);
    out.push_str(&format!(
        "{pad} | {}{}\n",
        " ".repeat(start),
        "^".repeat(width)
    ));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn points_a_caret_at_the_reference() {
        let source = "resource aws_s3_bucket b {\n  bucket = nope.bucket\n}\n";
        let err = crate::compile_source(source).unwrap_err();
        let text = render(source, "main.bcl", &err);
        assert!(text.starts_with("error: "), "{text}");
        assert!(text.contains("--> main.bcl:2:12"), "{text}");
        assert!(text.contains("  bucket = nope.bucket"), "{text}");
        // The caret sits under `nope.bucket` (column 12).
        let caret_line = text.lines().find(|l| l.contains('^')).unwrap();
        assert_eq!(
            caret_line.find('^'),
            Some(caret_line.find('|').unwrap() + 2 + 11)
        );
        assert!(caret_line.ends_with(&"^".repeat(11)), "{caret_line}");
    }

    #[test]
    fn suggests_the_closest_resource_name() {
        let source = "resource aws_s3_bucket my_bucket { bucket = \"a\" }\n\
                      resource aws_s3_bucket b { bucket = my_buket.bucket }\n";
        let err = crate::compile_source(source).unwrap_err();
        assert!(
            err.to_string().contains("did you mean `my_bucket`?"),
            "{err}"
        );
    }

    #[test]
    fn errors_without_a_location_render_as_a_plain_message() {
        let err = DslError::Compile {
            message: "something".into(),
        };
        assert_eq!(render("", "x.bcl", &err), "error: something\n  --> x.bcl\n");
    }
}
