//! External converter invocation (Pandoc) for preview and export
//! (`03-behavior/03-live-preview.md`, `03-behavior/08-export.md`). The app
//! performs no Markdown conversion itself; when the converter is missing
//! or fails, the error is surfaced to the user.

use std::path::Path;
use std::process::Command;

use crate::settings::InputFormat;

/// Converter flavor identifiers accepted by Pandoc.
impl InputFormat {
    pub fn pandoc_format(&self, restricted: bool) -> String {
        let base = match self {
            InputFormat::Markdown => "markdown",
            InputFormat::CommonMark => "commonmark",
            InputFormat::Gfm => "gfm",
            InputFormat::MarkdownMmd => "markdown_mmd",
            InputFormat::MarkdownStrict => "markdown_strict",
        };
        if restricted {
            format!("{base}-raw_html")
        } else {
            base.to_owned()
        }
    }
}

/// Convert `text` to an HTML fragment. Relative paths resolve against
/// `doc_dir` (the process runs with that working directory for the
/// converter).
pub fn to_html_fragment(
    text: &str,
    flavor: InputFormat,
    restricted: bool,
    doc_dir: Option<&Path>,
) -> Result<String, String> {
    let mut cmd = Command::new("pandoc");
    cmd.arg("--from")
        .arg(flavor.pandoc_format(restricted))
        .arg("--to")
        .arg("html5")
        .arg("--mathml")
        .arg("--wrap=none");
    if let Some(dir) = doc_dir {
        cmd.current_dir(dir);
    }
    run_converter(&mut cmd, text)
}

/// Standalone HTML document with the given stylesheet embedded (used by
/// Copy HTML's standalone variant and HTML export).
pub fn to_standalone_html(
    text: &str,
    flavor: InputFormat,
    restricted: bool,
    doc_dir: Option<&Path>,
    css: &str,
    title: &str,
) -> Result<String, String> {
    let fragment = to_html_fragment(text, flavor, restricted, doc_dir)?;
    Ok(format!(
        "<!DOCTYPE html>\n<html>\n<head>\n<meta charset=\"utf-8\">\n<title>{title}</title>\n<style>\n{css}\n</style>\n</head>\n<body>\n{fragment}\n</body>\n</html>\n"
    ))
}

/// Wrap a converted fragment into a full preview document with the scheme
/// stylesheets embedded (base + scheme).
pub fn preview_document(fragment: &str, base_css: &str, scheme_css: &str, dark: bool) -> String {
    let meta_color = if dark { "dark" } else { "light" };
    format!(
        "<!DOCTYPE html>\n<html>\n<head>\n<meta charset=\"utf-8\">\n\
         <meta name=\"color-scheme\" content=\"{meta_color}\">\n\
         <style>\n{base_css}\n</style>\n<style>\n{scheme_css}\n</style>\n\
         </head>\n<body>\n{fragment}\n</body>\n</html>\n"
    )
}

fn run_converter(cmd: &mut Command, input: &str) -> Result<String, String> {
    use std::io::Write;
    use std::process::Stdio;

    let mut child = cmd
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                "The document converter (pandoc) is not installed. Install it to use the preview and export."
                    .to_owned()
            } else {
                format!("cannot start the converter: {e}")
            }
        })?;
    {
        let mut stdin = child.stdin.take().expect("piped stdin");
        let _ = stdin.write_all(input.as_bytes());
    }
    let output = child
        .wait_with_output()
        .map_err(|e| format!("converter failed: {e}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "conversion failed: {}",
            stderr.lines().next().unwrap_or("unknown error")
        ));
    }
    String::from_utf8(output.stdout).map_err(|_| "converter produced invalid UTF-8".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_with_restriction() {
        assert_eq!(
            InputFormat::Markdown.pandoc_format(true),
            "markdown-raw_html"
        );
        assert_eq!(InputFormat::Gfm.pandoc_format(false), "gfm");
        assert_eq!(
            InputFormat::MarkdownStrict.pandoc_format(true),
            "markdown_strict-raw_html"
        );
    }

    #[test]
    fn converter_missing_is_reported() {
        // A nonexistent converter binary produces a clear error.
        let mut cmd = Command::new("definitely-not-a-real-converter-xyz");
        cmd.arg("--version");
        let err = run_converter(&mut cmd, "").unwrap_err();
        assert!(err.contains("cannot start") || err.contains("not installed"), "{err}");
    }

    #[test]
    fn pandoc_conversion_works() {
        // Requires pandoc on PATH; skipped silently when absent.
        if Command::new("pandoc").arg("--version").output().is_err() {
            eprintln!("pandoc not available; skipping");
            return;
        }
        let html = to_html_fragment("# Hi\n\ntext", InputFormat::Markdown, true, None).unwrap();
        assert!(html.contains("<h1"), "{html}");
        assert!(html.contains("text"), "{html}");
    }

    #[test]
    fn restricted_keeps_html_as_text() {
        if Command::new("pandoc").arg("--version").output().is_err() {
            eprintln!("pandoc not available; skipping");
            return;
        }
        let html =
            to_html_fragment("<b>raw</b>", InputFormat::Markdown, true, None).unwrap();
        assert!(!html.contains("<b>raw</b>"), "{html}");
        let html2 =
            to_html_fragment("<b>raw</b>", InputFormat::Markdown, false, None).unwrap();
        assert!(html2.contains("<b>"), "{html2}");
    }

    #[test]
    fn preview_document_structure() {
        let doc = preview_document("<p>x</p>", "body{}", ":root{}", false);
        assert!(doc.contains("<!DOCTYPE html>"));
        assert!(doc.contains("body{}"));
        assert!(doc.contains(":root{}"));
        assert!(doc.contains("<p>x</p>"));
    }
}
