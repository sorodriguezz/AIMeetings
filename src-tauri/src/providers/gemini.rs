//! The Antigravity CLI (`agy`) — surfaced as "Gemini" — text-only.
//!
//! Google retired the standalone `gemini` CLI for subscription accounts and replaced it with `agy`,
//! which runs Gemini 3.x (and a few Claude / GPT-OSS models) against a Google-account login. This
//! drives it in headless `-p` mode, verified in CodeFlow against agy 1.1.7–1.2.16:
//!
//!   - `-p` does **not** read stdin, and there is no `--system-prompt` or `--file` flag, so the
//!     whole brief (system + ask + data) is either the `-p` argument itself or a file agy is told
//!     to read. [`delivery`] picks: **inline** when it fits under [`INLINE_LIMIT`] and is a single
//!     line — or multi-line on a program known not to be a `.cmd`/`.bat` shim, since a text-only run
//!     is read-only; **a file** otherwise, written to a private directory of its own that is added
//!     with `--add-dir`, which needs `--dangerously-skip-permissions` for agy to read it headlessly.
//!     agy has no narrower grant, so `--sandbox` always goes with it.
//!   - `--output-format stream-json` (1.1.10+) prints one JSON event per line and closes with
//!     `{"event":"result","result":{…}}`, whose `response` is the reply. A CLI that ignored the flag
//!     prints the reply as plain text, and that stays the fallback.
//!   - A text-only run is given an empty folder of its own as its workspace (and names it to agy
//!     with `--add-dir`, which is what scopes its file access) — not wherever the app was started
//!     from. agy has no flag to drop its own agent prompt; this, the sandbox and no permission
//!     bypass where the brief can ride inline are what it allows.

use serde::Deserialize;

use super::files::{write_into, CallFiles};
use super::spawn::is_batch_shim;
use super::text::{first_nonempty, quota_signal, refusal_reply, QUOTA_MARKER};
use super::{Invocation, TextCall};

/// Above this many chars the brief is delivered via a file + `--add-dir` instead of inline, to stay
/// clear of the Windows ~32k command-line limit.
const INLINE_LIMIT: usize = 12_000;

/// How a brief reaches agy.
#[derive(Debug, PartialEq, Eq)]
enum Delivery {
    /// As the `-p` argument itself.
    Inline,
    /// In a private file agy is told to read — which needs the permission bypass.
    File,
}

/// Which delivery a brief gets, on this program. Too large for the command line always means a
/// file; a single line always fits; a **multi-line** brief goes inline only for a read-only run on a
/// program that is not a batch shim.
fn delivery(program: &str, brief: &str, read_only: bool) -> Delivery {
    if brief.len() > INLINE_LIMIT {
        return Delivery::File;
    }
    if !brief.contains('\n') || (read_only && !is_batch_shim(program)) {
        return Delivery::Inline;
    }
    Delivery::File
}

/// An id never contains whitespace, so the first token is always it — newer `agy models` listings
/// print `<id>\t<display label>`, and a stored setting may still hold the whole line.
fn model_id(raw: &str) -> &str {
    raw.split_whitespace().next().unwrap_or("")
}

/// Writes the brief into a private directory of its own — so `--add-dir` scopes agy to exactly
/// this file — and returns `(directory, file)`.
fn write_brief(files: &CallFiles, content: &str) -> Option<(std::path::PathBuf, std::path::PathBuf)> {
    let dir = files.dir("agy")?;
    let file = write_into(&dir, "brief.txt", content)?;
    Some((dir, file))
}

pub(super) fn invocation(program: &str, call: &TextCall<'_>, files: &CallFiles) -> Invocation {
    let brief = call.brief(true);
    // A text-only run reads nothing and writes nothing.
    let read_only = true;
    let mut args: Vec<String> = Vec::new();
    let mut needs_read_permission = false;
    let file = match delivery(program, &brief, read_only) {
        Delivery::Inline => None,
        Delivery::File => write_brief(files, &brief),
    };
    match file {
        Some((dir, file)) => {
            args.push("-p".into());
            args.push(format!(
                "Read the file at {} and carry out the instructions it contains, replying with only the requested output.",
                file.display()
            ));
            args.extend(["--add-dir".into(), dir.to_string_lossy().into_owned()]);
            needs_read_permission = true;
        }
        // Either it fits, or the file could not be written and an inline attempt is the better of
        // two bad outcomes.
        None => args.extend(["-p".into(), brief]),
    }
    args.extend(["--output-format".into(), "stream-json".into()]);
    let model = model_id(call.model);
    if !model.is_empty() {
        args.extend(["--model".into(), model.into()]);
    }
    if needs_read_permission {
        args.push("--dangerously-skip-permissions".into());
    }
    if needs_read_permission || read_only {
        args.push("--sandbox".into());
    }
    let scratch = files.dir("agy-scratch");
    if let Some(dir) = &scratch {
        args.extend(["--add-dir".into(), dir.to_string_lossy().into_owned()]);
    }
    // agy reads no stdin; the data rides in the brief. Piped anyway, as the original did — the pipe
    // is closed at EOF and costs nothing.
    Invocation { args, stdin: call.data.to_string(), cwd: scratch, env: Vec::new() }
}

/// The closing `{"event":"result", …}` line of a `stream-json` run.
#[derive(Deserialize)]
struct AgyResult {
    #[serde(default)]
    response: String,
    #[serde(default)]
    usage: Option<AgyUsage>,
}

#[derive(Default, Deserialize)]
struct AgyUsage {
    #[serde(default)]
    output_tokens: i64,
    #[serde(default)]
    thinking_tokens: i64,
}

/// Walks the events backwards for the run's verdict.
fn result_event(stdout: &str) -> Option<AgyResult> {
    for line in stdout.lines().rev() {
        let line = line.trim();
        if !line.starts_with('{') {
            continue;
        }
        let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else { continue };
        if value.get("event").and_then(serde_json::Value::as_str) != Some("result") {
            continue;
        }
        if let Some(result) = value.get("result") {
            return serde_json::from_value(result.clone()).ok();
        }
    }
    None
}

/// Under `stream-json` the reply is the `response` of the closing event; a CLI that ignored the
/// flag prints the reply as plain text instead, and that stays the fallback.
pub(super) fn interpret(success: bool, status_label: &str, stdout: &str, stderr: &str) -> Result<String, String> {
    if !success {
        if quota_signal(stderr) {
            return Err(format!("{QUOTA_MARKER}{}", stderr.trim()));
        }
        if quota_signal(stdout) {
            return Err(format!("{QUOTA_MARKER}{}", stdout.trim()));
        }
        let detail = first_nonempty(&[stderr, stdout], "no output on stdout or stderr");
        return Err(format!("agy exited with an error ({status_label}): {detail}"));
    }

    let parsed = result_event(stdout);
    let text = match &parsed {
        Some(result) => result.response.trim().to_string(),
        None => stdout.trim().to_string(),
    };
    if text.is_empty() {
        let err = stderr.trim();
        return Err(if err.is_empty() { "agy produced no output".to_string() } else { err.to_string() });
    }
    let generated = parsed.as_ref().and_then(|r| r.usage.as_ref()).map(|u| u.output_tokens + u.thinking_tokens);
    if refusal_reply(&text, generated) {
        return Err(format!("{QUOTA_MARKER}{text}"));
    }
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::super::test_util::after;
    use super::*;

    fn call<'a>(system: &'a str, ask: &'a str, data: &'a str) -> TextCall<'a> {
        TextCall { system, ask, data, model: "" }
    }

    #[test]
    fn a_successful_run_returns_stdout_as_the_reply() {
        assert_eq!(interpret(true, "exit status: 0", "  feat: add thing  ", "").unwrap(), "feat: add thing");
    }

    /// Captured verbatim from `agy -p … --output-format stream-json` on 1.1.10.
    const STREAM: &str = concat!(
        r#"{"event":"step_update","step_update":{"step_index":0,"state":"DONE"}}"#,
        "\n",
        r#"{"event":"result","result":{"conversation_id":"c8bb","status":"SUCCESS","response":"ok\n","#,
        r#""num_turns":1,"usage":{"input_tokens":17902,"output_tokens":16,"thinking_tokens":4,"#,
        r#""cache_read_tokens":9,"total_tokens":17918}}}"#,
    );

    #[test]
    fn the_closing_event_carries_the_reply() {
        assert_eq!(interpret(true, "exit status: 0", STREAM, "").unwrap(), "ok");
    }

    #[test]
    fn only_the_result_event_counts() {
        let noise = concat!(r#"{"event":"step_update","step_update":{"state":"DONE"}}"#, "\n", "plain tail");
        assert!(interpret(true, "exit status: 0", noise, "").unwrap().contains("plain tail"));
    }

    #[test]
    fn surfaces_the_failure_detail() {
        let err = interpret(false, "exit status: 1", "", "not signed in — run `agy` to log in").unwrap_err();
        assert_eq!(err, "agy exited with an error (exit status: 1): not signed in — run `agy` to log in");
    }

    #[test]
    fn a_quota_message_gets_the_marker() {
        let err = interpret(false, "exit status: 1", "", "quota exceeded, try again in 2h").unwrap_err();
        assert!(err.starts_with(QUOTA_MARKER), "got {err}");
    }

    #[test]
    fn empty_output_on_a_clean_exit_is_an_error_not_a_blank_reply() {
        assert_eq!(interpret(true, "exit status: 0", "   ", "").unwrap_err(), "agy produced no output");
    }

    #[test]
    fn the_delivery_follows_size_lines_and_shims() {
        assert_eq!(delivery("agy", "hola", false), Delivery::Inline);
        assert_eq!(delivery("agy", "system prompt\n\nthe ask", false), Delivery::File);
        assert_eq!(delivery("/usr/local/bin/agy", "a\nb", true), Delivery::Inline);
        assert_eq!(delivery(r"C:\npm\agy.cmd", "a\nb", true), Delivery::File);
        assert_eq!(delivery("agy", &"x".repeat(INLINE_LIMIT + 1), true), Delivery::File);
    }

    /// A short multi-line brief rides the command line, and with no file to read there is no
    /// permission bypass — only the sandbox.
    #[test]
    fn a_short_brief_is_inline_with_the_sandbox_and_no_bypass() {
        let files = CallFiles::new();
        let inv = invocation("/usr/local/bin/agy", &call("Eres un asistente.\nResponde en texto.", "¿qué es esto?", ""), &files);
        assert!(!inv.args.iter().any(|a| a == "--dangerously-skip-permissions"), "{:?}", inv.args);
        assert!(inv.args.iter().any(|a| a == "--sandbox"));
        let prompt = after(&inv.args, "-p").unwrap();
        assert!(prompt.contains("Responde en texto.") && prompt.contains("¿qué es esto?"), "{prompt}");
        assert!(super::super::test_util::has_pair(&inv.args, "--output-format", "stream-json"));
        let cwd = inv.cwd.as_ref().expect("a scratch directory");
        assert_eq!(std::fs::read_dir(cwd).unwrap().count(), 0);
        assert_eq!(after(&inv.args, "--add-dir"), Some(cwd.to_string_lossy().as_ref()));
    }

    /// A transcript is past the inline limit: the file, the bypass to read it, and the sandbox.
    #[test]
    fn a_long_brief_goes_to_a_private_file_that_goes_with_the_call() {
        let files = CallFiles::new();
        let data = "A: hola\n".repeat(3_000);
        let inv = invocation("agy", &call("Eres quien resume.", "Resume", &data), &files);
        assert!(inv.args.iter().any(|a| a == "--dangerously-skip-permissions"));
        assert!(inv.args.iter().any(|a| a == "--sandbox"));
        let dir = std::path::PathBuf::from(after(&inv.args, "--add-dir").unwrap());
        let brief = std::fs::read_to_string(dir.join("brief.txt")).unwrap();
        assert!(brief.starts_with("Eres quien resume.\n\nResume") && brief.contains("A: hola"));
        assert!(after(&inv.args, "-p").unwrap().contains("brief.txt"));
        assert!(inv.args.iter().all(|a| !a.contains('\n')));
        drop(files);
        assert!(!dir.exists());
    }

    #[test]
    fn a_stored_label_never_reaches_the_model_flag() {
        assert_eq!(model_id("gemini-3.6-flash-high\tGemini 3.6 Flash (High)"), "gemini-3.6-flash-high");
        assert_eq!(model_id("   "), "");
        let files = CallFiles::new();
        let inv = invocation("agy", &TextCall { model: "gemini-3.6-flash\tGemini 3.6 Flash", ..call("", "a", "") }, &files);
        assert_eq!(after(&inv.args, "--model"), Some("gemini-3.6-flash"));
    }
}
