//! External converter invocation (Pandoc) for preview and export
//! (`03-behavior/03-live-preview.md`, `03-behavior/08-export.md`). The app
//! performs no Markdown conversion itself; when the converter is missing
//! or fails, the error is surfaced to the user.

use std::path::Path;
use std::process::Command;

use crate::converter;
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
    let slot = std::sync::Arc::new(std::sync::Mutex::new(None));
    let cancelled = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    to_html_fragment_cancellable(text, flavor, restricted, doc_dir, &slot, &cancelled)
}

/// `to_html_fragment` with cancellation support: `slot` receives the child
/// process so a superseded preview update can kill it, `cancelled` is set
/// while killing. Without this, typing into a multi-megabyte document
/// leaves one pandoc process per burst running to completion.
pub fn to_html_fragment_cancellable(
    text: &str,
    flavor: InputFormat,
    restricted: bool,
    doc_dir: Option<&Path>,
    slot: &std::sync::Arc<std::sync::Mutex<Option<std::process::Child>>>,
    cancelled: &std::sync::Arc<std::sync::atomic::AtomicBool>,
) -> Result<String, String> {
    let mut cmd = converter::command("pandoc", doc_dir);
    cmd.arg("--from")
        .arg(flavor.pandoc_format(restricted))
        .arg("--to")
        .arg("html5")
        .arg("--mathml")
        .arg("--wrap=none");
    run_converter(&mut cmd, text, slot, cancelled)
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

/// A converter may not run longer than this before it is killed — a hung
/// converter otherwise leaks its thread and process forever. Generous:
/// a multi-megabyte document converts in a few seconds.
const CONVERT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

fn run_converter(
    cmd: &mut Command,
    input: &str,
    slot: &std::sync::Arc<std::sync::Mutex<Option<std::process::Child>>>,
    cancelled: &std::sync::Arc<std::sync::atomic::AtomicBool>,
) -> Result<String, String> {
    use std::io::{Read, Write};
    use std::process::Stdio;
    use std::sync::atomic::Ordering;
    use std::time::{Duration, Instant};

    let mut child = cmd
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                converter::missing_converter_message()
            } else {
                format!("cannot start the converter: {e}")
            }
        })?;
    let mut stdout = child.stdout.take().expect("piped stdout");
    let mut stderr = child.stderr.take().expect("piped stderr");
    // Start the readers BEFORE feeding stdin: a converter may produce more
    // than a pipe's worth of HTML while still consuming input. Writing the
    // whole (possibly multi-megabyte) document first would then block on a
    // full stdout pipe while pandoc blocks on a full stdin pipe — a classic
    // deadlock that only shows up on large documents.
    let out_reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = stdout.read_to_end(&mut buf);
        buf
    });
    let err_reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = stderr.read_to_end(&mut buf);
        buf
    });
    {
        let mut stdin = child.stdin.take().expect("piped stdin");
        let _ = stdin.write_all(input.as_bytes());
        // Closing stdin signals EOF so the converter can finish.
    }

    // Publish the child for cancellation, then poll.
    *slot.lock().unwrap_or_else(|e| e.into_inner()) = Some(child);
    let deadline = Instant::now() + CONVERT_TIMEOUT;
    let status = loop {
        {
            let mut guard = slot.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(c) = guard.as_mut() {
                if let Ok(Some(st)) = c.try_wait() {
                    guard.take();
                    break Ok(st);
                }
            } else {
                // Someone else reaped it (should not happen); treat as loss.
                break Err("converter lost".to_owned());
            }
        }
        if cancelled.load(Ordering::SeqCst) {
            break Err("conversion cancelled".to_owned());
        }
        if Instant::now() > deadline {
            if let Ok(mut guard) = slot.lock() {
                if let Some(c) = guard.as_mut() {
                    let _ = c.kill();
                }
            }
            // Give the kill a moment to take effect.
            let hard_deadline = Instant::now() + Duration::from_secs(2);
            loop {
                let mut guard = slot.lock().unwrap_or_else(|e| e.into_inner());
                match guard.as_mut() {
                    Some(c) => {
                        if c.try_wait().map(|s| s.is_some()).unwrap_or(false) {
                            break;
                        }
                    }
                    None => break,
                }
                if Instant::now() > hard_deadline {
                    if let Some(c) = guard.as_mut() {
                        let _ = c.kill();
                    }
                    break;
                }
                drop(guard);
                std::thread::sleep(Duration::from_millis(20));
            }
            break Err("conversion timed out".to_owned());
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    // The slot is ours again; nothing may kill a process that is gone.
    *slot.lock().unwrap_or_else(|e| e.into_inner()) = None;

    let status = match status {
        Ok(st) => st,
        Err(e) => {
            // Wait for the readers so the threads never leak.
            let _ = out_reader.join();
            let _ = err_reader.join();
            return Err(e);
        }
    };
    let output_stdout = out_reader.join().unwrap_or_default();
    let output_stderr = err_reader.join().unwrap_or_default();
    if !status.success() {
        if cancelled.load(Ordering::SeqCst) {
            return Err("conversion cancelled".to_owned());
        }
        // Inside Flatpak a missing host converter is reported by the `sh`
        // wrapper as exit 127, not as a spawn failure.
        if converter::in_flatpak() && status.code() == Some(127) {
            return Err(converter::missing_converter_message());
        }
        let stderr = String::from_utf8_lossy(&output_stderr);
        return Err(format!(
            "conversion failed: {}",
            stderr.lines().next().unwrap_or("unknown error")
        ));
    }
    String::from_utf8(output_stdout).map_err(|_| "converter produced invalid UTF-8".to_owned())
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
        let slot = std::sync::Arc::new(std::sync::Mutex::new(None));
        let cancelled = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let err = run_converter(&mut cmd, "", &slot, &cancelled).unwrap_err();
        assert!(err.contains("cannot start") || err.contains("not installed"), "{err}");
    }

    #[test]
    fn large_input_does_not_deadlock_pipes() {
        // `cat` echoes stdin to stdout. With more than a pipe's worth of
        // data, feeding all input before draining stdout deadlocks: cat
        // blocks on a full stdout pipe and the writer blocks on a full
        // stdin pipe. The readers must run first (regression guard).
        let slot = std::sync::Arc::new(std::sync::Mutex::new(None));
        let cancelled = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let input = "a".repeat(3_000_000);
        let out = run_converter(&mut Command::new("cat"), &input, &slot, &cancelled)
            .expect("cat must succeed");
        assert_eq!(out.len(), input.len());
    }

    #[test]
    fn cancelled_run_is_killed() {
        // A long-running converter is killed by cancellation instead of
        // leaking the process and its threads.
        let slot = std::sync::Arc::new(std::sync::Mutex::new(None));
        let cancelled = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let slot2 = slot.clone();
        let cancelled2 = cancelled.clone();
        let handle = std::thread::spawn(move || {
            run_converter(
                &mut {
                    let mut c = Command::new("sleep");
                    c.arg("60");
                    c
                },
                "",
                &slot2,
                &cancelled2,
            )
        });
        // Wait for the child to be registered, then cancel it.
        for _ in 0..100 {
            if slot.lock().unwrap().is_some() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        cancelled.store(true, std::sync::atomic::Ordering::SeqCst);
        if let Ok(mut guard) = slot.lock() {
            if let Some(child) = guard.as_mut() {
                let _ = child.kill();
            }
        }
        let result = handle.join().expect("runner thread");
        assert!(result.is_err(), "a cancelled conversion must fail");
        assert!(slot.lock().unwrap().is_none(), "the slot must be cleared");
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
