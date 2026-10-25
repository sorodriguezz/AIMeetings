//! The Claude Code CLI (`claude`), text-only.
//!
//! **What a text-only run is.** Claude Code's own system prompt, its built-in tools, every MCP
//! server, plugin, hook and skill are written for an agent this run is not — measured in CodeFlow
//! on 2.1.304, a four-word title cost 26,696 tokens with them and 827 without. So the run
//! *replaces* the system prompt (`--system-prompt-file`), loads no tool (`--tools ""`), no MCP
//! server (`--strict-mcp-config` with no `--mcp-config`) and no slash commands
//! (`--disable-slash-commands`), and turns hooks off for itself alone (`--settings` with
//! `disableAllHooks`, layered over the user's own so nothing in their Claude Code changes).
//!
//! **Windows `.cmd` shim can't take newline args.** When `claude` is installed via npm the binary
//! is `claude.cmd`, which `cmd.exe` runs — and it rejects any argument containing a newline. Two
//! arguments here can carry one: the ask (`-p`) and the system prompt. A multi-line ask goes to
//! stdin behind a fixed one-line pointer ([`PROMPT_POINTER`]); the system prompt and the settings go
//! to private files deleted when the call ends (see `files`).
//!
//! **Output.** `--output-format stream-json --verbose` prints one JSON event per line and closes
//! with `{"type":"result",…}` — the same object plain `json` printed on its own — whose `result` is
//! the reply and whose `is_error` says whether it is a reply at all. The CLI reports its *own*
//! failures there and exits non-zero with stderr **empty**, so stdout is parsed before the exit
//! status is judged.

use serde::Deserialize;

use super::files::CallFiles;
use super::text::{first_nonempty, quota_signal, refusal_reply, QUOTA_MARKER};
use super::{Invocation, TextCall};

/// Single-line, ASCII, shim-safe stand-in for an ask this engine can't put on the command line as
/// given. The real ask rides on stdin instead, ahead of the data.
const PROMPT_POINTER: &str =
    "Your instructions for this turn are in the input provided via stdin. Read all of it and carry it out.";

/// The settings a text-only run layers over the user's own: no hooks. A plugin's `SessionStart`
/// hook is context injected into every session, and a run with no tools has nothing for the others
/// to watch.
const RUN_SETTINGS: &str = r#"{"disableAllHooks":true}"#;

/// Whether `prompt` is unsafe to hand to `-p` as-is on Windows (see the module docs).
fn needs_stdin_prompt(prompt: &str) -> bool {
    prompt.contains('\n')
}

pub(super) fn invocation(call: &TextCall<'_>, files: &CallFiles) -> Invocation {
    let mut args: Vec<String> = Vec::new();
    args.push("-p".into());
    args.push(if needs_stdin_prompt(call.ask) { PROMPT_POINTER.into() } else { call.ask.into() });
    // Replaced rather than appended to: Claude Code's prompt is written for an agent with tools.
    let system = call.system_or_default();
    match files.write("claude-system", "txt", system) {
        Some(path) => args.extend(["--system-prompt-file".into(), path.to_string_lossy().into_owned()]),
        None => args.extend(["--system-prompt".into(), system.into()]),
    }
    if !call.model.trim().is_empty() {
        args.extend(["--model".into(), call.model.trim().into()]);
    }
    // `stream-json` is only accepted alongside `--verbose` in `-p` mode.
    args.extend(["--output-format".into(), "stream-json".into(), "--verbose".into()]);
    args.extend(["--tools".into(), String::new()]);
    args.push("--strict-mcp-config".into());
    args.push("--disable-slash-commands".into());
    // A file rather than the JSON inline, for the same `.cmd` shim reason: quotes do not survive it.
    match files.write("claude-settings", "json", RUN_SETTINGS) {
        Some(path) => args.extend(["--settings".into(), path.to_string_lossy().into_owned()]),
        None => args.extend(["--settings".into(), RUN_SETTINGS.into()]),
    }

    let mut stdin = String::new();
    if needs_stdin_prompt(call.ask) {
        stdin.push_str(call.ask);
        stdin.push_str("\n\n");
    }
    stdin.push_str(call.data);

    Invocation {
        args,
        stdin,
        // An empty folder of its own rather than wherever the app was started from, so no
        // `CLAUDE.md` or project settings of some unrelated directory reach the run.
        cwd: files.dir("claude-scratch"),
        // A process started to answer a question must never be the one that starts the updater.
        env: vec![("DISABLE_AUTOUPDATER".into(), "1".into())],
    }
}

#[derive(Deserialize)]
struct ClaudeCliResult {
    result: Option<String>,
    #[serde(default)]
    is_error: bool,
    /// Read only to tell a refusal (zero generated tokens) from an answer.
    #[serde(default)]
    usage: Option<ClaudeUsage>,
}

#[derive(Default, Deserialize)]
struct ClaudeUsage {
    #[serde(default)]
    output_tokens: i64,
}

/// One string field of a JSON object, borrowed.
fn str_field<'v>(value: &'v serde_json::Value, key: &str) -> Option<&'v str> {
    value.get(key).and_then(serde_json::Value::as_str)
}

/// Picks the verdict out of stdout: the last `{"type":"result",…}` line of a stream, or — for a
/// CLI that ignored the flag — the whole buffer parsed as the single-blob `json` format.
fn result_payload(stdout: &str) -> Option<ClaudeCliResult> {
    for line in stdout.lines().rev() {
        let line = line.trim();
        if !line.starts_with('{') {
            continue;
        }
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(line) {
            if str_field(&value, "type") == Some("result") {
                return serde_json::from_value(value).ok();
            }
        }
    }
    serde_json::from_str::<ClaudeCliResult>(stdout).ok()
}

/// Turns one finished `claude` run into its reply or a user-facing error.
pub(super) fn interpret(success: bool, status_label: &str, stdout: &str, stderr: &str) -> Result<String, String> {
    let parsed = result_payload(stdout);
    let result_text = parsed.as_ref().and_then(|p| p.result.as_deref()).map(str::trim).filter(|t| !t.is_empty());

    if let Some(text) = result_text {
        let failed = !success || parsed.as_ref().is_some_and(|p| p.is_error);
        // On a failed run any signal anywhere explains the failure; on a successful one only a
        // message that is nothing but the refusal counts — and never one the model generated.
        let generated = parsed.as_ref().and_then(|p| p.usage.as_ref()).map(|u| u.output_tokens);
        let refused = if failed { quota_signal(text) } else { refusal_reply(text, generated) };
        if refused {
            return Err(format!("{QUOTA_MARKER}{text}"));
        }
        if failed {
            return Err(text.to_string());
        }
        return Ok(text.to_string());
    }

    // A clean verdict with nothing in it: the model answered nothing. Never the stream around it.
    if parsed.as_ref().is_some_and(|p| success && !p.is_error && p.result.is_some()) {
        return Err("claude produced no output".to_string());
    }

    let readable = readable_stdout(stdout);
    if !success {
        if quota_signal(stderr) {
            return Err(format!("{QUOTA_MARKER}{}", stderr.trim()));
        }
        if quota_signal(stdout) {
            let reason = if readable.is_empty() { stdout.trim() } else { readable.as_str() };
            return Err(format!("{QUOTA_MARKER}{reason}"));
        }
        // Neither stream carried a usable message — report the exit status rather than an error
        // string that trails off into nothing.
        let detail = first_nonempty(&[stderr, &readable], "no output on stdout or stderr");
        return Err(format!("claude exited with an error ({status_label}): {detail}"));
    }

    if readable.is_empty() {
        return Err("claude produced no output".to_string());
    }
    if refusal_reply(&readable, None) {
        return Err(format!("{QUOTA_MARKER}{readable}"));
    }
    Ok(readable)
}

/// What of stdout a person could read, for a run that left no verdict to read instead.
///
/// Plain text — a CLI that ignored the format flag — passes through as it is. An event stream is the
/// CLI's protocol and never prose: from it only the text of the last `assistant` message survives,
/// and nothing at all when there is none.
fn readable_stdout(stdout: &str) -> String {
    let first = stdout.lines().map(str::trim).find(|line| !line.is_empty());
    let is_event_stream = first.is_some_and(|line| {
        serde_json::from_str::<serde_json::Value>(line).is_ok_and(|value| str_field(&value, "type").is_some())
    });
    if !is_event_stream {
        return stdout.trim().to_string();
    }
    for line in stdout.lines().rev() {
        let Ok(value) = serde_json::from_str::<serde_json::Value>(line.trim()) else { continue };
        if str_field(&value, "type") != Some("assistant") {
            continue;
        }
        let Some(parts) = value.get("message").and_then(|m| m.get("content")).and_then(serde_json::Value::as_array) else {
            continue;
        };
        let text = parts
            .iter()
            .filter(|part| str_field(part, "type") == Some("text"))
            .filter_map(|part| str_field(part, "text"))
            .collect::<Vec<_>>()
            .join("\n");
        if !text.trim().is_empty() {
            return text.trim().to_string();
        }
    }
    String::new()
}

#[cfg(test)]
mod tests {
    use super::super::test_util::{after, has_pair};
    use super::super::TEXT_ONLY_SYSTEM_PROMPT;
    use super::*;

    fn call<'a>(system: &'a str, ask: &'a str, data: &'a str) -> TextCall<'a> {
        TextCall { system, ask, data, model: "" }
    }

    #[test]
    fn a_text_only_run_loads_nothing_but_its_own_prompt() {
        let files = CallFiles::new();
        let inv = invocation(&TextCall { model: "claude-haiku-5-5", ..call("Eres quien resume.", "Resume", "la reunión") }, &files);
        let args = &inv.args;
        assert_eq!(after(args, "-p"), Some("Resume"));
        let system = after(args, "--system-prompt-file").expect("a file");
        assert_eq!(std::fs::read_to_string(system).unwrap(), "Eres quien resume.");
        assert!(has_pair(args, "--tools", ""), "{args:?}");
        assert!(has_pair(args, "--output-format", "stream-json") && args.iter().any(|a| a == "--verbose"));
        assert!(args.iter().any(|a| a == "--strict-mcp-config") && args.iter().any(|a| a == "--disable-slash-commands"));
        assert!(has_pair(args, "--model", "claude-haiku-5-5"));
        let settings = after(args, "--settings").unwrap();
        assert_eq!(std::fs::read_to_string(settings).unwrap(), RUN_SETTINGS);
        assert!(!args.iter().any(|a| a == "--append-system-prompt-file" || a == "--permission-mode"));
        assert_eq!(inv.stdin, "la reunión");
        assert!(inv.env.iter().any(|(k, v)| k == "DISABLE_AUTOUPDATER" && v == "1"));
        let cwd = inv.cwd.as_ref().expect("a scratch directory");
        assert_eq!(std::fs::read_dir(cwd).unwrap().count(), 0);
        drop(files);
        assert!(!std::path::Path::new(system).exists(), "deleted with the call");
    }

    #[test]
    fn no_system_prompt_means_the_text_only_one() {
        let files = CallFiles::new();
        let inv = invocation(&call("", "hola", ""), &files);
        let path = after(&inv.args, "--system-prompt-file").unwrap();
        assert_eq!(std::fs::read_to_string(path).unwrap(), TEXT_ONLY_SYSTEM_PROMPT);
        assert!(!inv.args.iter().any(|a| a == "--model"));
    }

    /// Short but multi-line is exactly what a meeting ask looks like.
    #[test]
    fn a_multiline_ask_is_routed_to_stdin_ahead_of_the_data() {
        assert!(!needs_stdin_prompt("fix the null check"));
        let files = CallFiles::new();
        let inv = invocation(&call("", "line one\nline two", "the data"), &files);
        assert_eq!(after(&inv.args, "-p"), Some(PROMPT_POINTER));
        assert_eq!(inv.stdin, "line one\nline two\n\nthe data");
        assert!(inv.args.iter().all(|a| !a.contains('\n')), "nothing multi-line on the command line");
    }

    const FAILED_RUN_STDOUT: &str = r#"{"is_error":true,"stop_reason":"stop_sequence",
        "session_id":"8c166654-4807-4d62-a1d7-33909c2efd55","subtype":"success",
        "result":"Failed to authenticate: OAuth session expired and could not be refreshed"}"#;

    /// Exit 1, **empty stderr**, and the reason only on stdout.
    #[test]
    fn surfaces_the_reason_json_carries_when_stderr_is_empty() {
        let err = interpret(false, "exit status: 1", FAILED_RUN_STDOUT, "").unwrap_err();
        assert_eq!(err, "Failed to authenticate: OAuth session expired and could not be refreshed");
    }

    const STREAM_JSON_STDOUT: &str = concat!(
        r#"{"type":"system","subtype":"init","session_id":"s-1","model":"claude-opus-4-8"}"#,
        "\n",
        r#"{"type":"assistant","message":{"content":[{"type":"text","text":"thinking aloud"}]},"session_id":"s-1"}"#,
        "\n",
        r#"{"type":"result","subtype":"success","is_error":false,"session_id":"s-1","result":"fix: guard the null case","usage":{"input_tokens":10,"output_tokens":9}}"#,
        "\n",
    );

    #[test]
    fn reads_the_verdict_out_of_a_streamed_run() {
        assert_eq!(interpret(true, "exit status: 0", STREAM_JSON_STDOUT, "").unwrap(), "fix: guard the null case");
    }

    #[test]
    fn a_streamed_failure_reports_its_reason_not_the_exit_status() {
        let stdout = concat!(
            r#"{"type":"system","subtype":"init","session_id":"s-2"}"#,
            "\n",
            r#"{"type":"result","subtype":"error_during_execution","is_error":true,"result":"Unknown model 'nope'"}"#,
            "\n",
        );
        assert_eq!(interpret(false, "exit status: 1", stdout, "").unwrap_err(), "Unknown model 'nope'");
    }

    #[test]
    fn falls_back_to_the_exit_status_when_nothing_explains_the_failure() {
        let err = interpret(false, "exit status: 127", "", "").unwrap_err();
        assert_eq!(err, "claude exited with an error (exit status: 127): no output on stdout or stderr");
    }

    #[test]
    fn a_quota_failure_gets_the_marker() {
        let stdout = r#"{"is_error":true,"result":"Claude usage limit reached, resets at 5pm"}"#;
        assert!(interpret(false, "exit status: 1", stdout, "").unwrap_err().starts_with(QUOTA_MARKER));
    }

    /// A clean exit with a session-limit sentence and zero generated tokens is the provider saying
    /// no, not an answer to file as the meeting's summary.
    #[test]
    fn a_clean_exit_that_generated_nothing_can_still_be_a_refusal() {
        let stdout = r#"{"type":"result","is_error":false,"result":"You've hit your session limit · resets 12am","usage":{"output_tokens":0}}"#;
        assert!(interpret(true, "exit status: 0", stdout, "").unwrap_err().starts_with(QUOTA_MARKER));
        let answered = r#"{"type":"result","is_error":false,"result":"Se habló del rate limit de la API.","usage":{"output_tokens":40}}"#;
        assert_eq!(interpret(true, "exit status: 0", answered, "").unwrap(), "Se habló del rate limit de la API.");
    }

    #[test]
    fn a_successful_run_returns_the_trimmed_reply() {
        let stdout = r#"{"is_error":false,"session_id":"abc-123","result":"  feat: add thing  "}"#;
        assert_eq!(interpret(true, "exit status: 0", stdout, "").unwrap(), "feat: add thing");
    }

    #[test]
    fn non_json_stdout_on_a_clean_exit_is_passed_through() {
        assert_eq!(interpret(true, "exit status: 0", "plain text\n", "").unwrap(), "plain text");
    }

    #[test]
    fn an_empty_verdict_is_an_error_not_its_event_stream() {
        let stdout = [
            r#"{"type":"system","subtype":"init","session_id":"cf5e83bd"}"#,
            r#"{"type":"result","subtype":"success","is_error":false,"result":"","session_id":"cf5e83bd"}"#,
        ]
        .join("\n");
        assert_eq!(interpret(true, "exit status: 0", &stdout, "").unwrap_err(), "claude produced no output");
    }

    #[test]
    fn a_failure_with_no_reason_reports_the_status_and_not_the_stream() {
        let stdout = [
            r#"{"type":"system","subtype":"init","session_id":"s-3"}"#,
            r#"{"type":"system","subtype":"status","status":"compacting","session_id":"s-3"}"#,
        ]
        .join("\n");
        let err = interpret(false, "exit status: 1", &stdout, "").unwrap_err();
        assert_eq!(err, "claude exited with an error (exit status: 1): no output on stdout or stderr");
    }

    /// A stream cut off before its verdict keeps the answer it had already written, and is an error
    /// when it had written none — never its own events read back as prose.
    #[test]
    fn a_stream_without_a_verdict_is_read_for_its_last_answer_only() {
        let answered = [
            r#"{"type":"system","subtype":"init","session_id":"s-4"}"#,
            r#"{"type":"assistant","message":{"content":[{"type":"text","text":"Listo, el acta."}]},"session_id":"s-4"}"#,
        ]
        .join("\n");
        assert_eq!(interpret(true, "exit status: 0", &answered, "").unwrap(), "Listo, el acta.");
        let silent = r#"{"type":"system","subtype":"init","session_id":"s-5"}"#;
        assert_eq!(interpret(true, "exit status: 0", silent, "").unwrap_err(), "claude produced no output");
    }
}
