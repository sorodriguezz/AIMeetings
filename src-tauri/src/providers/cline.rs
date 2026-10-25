//! The Cline CLI (`cline`), text-only — a headless agent that drives whatever provider the user
//! authenticated inside it, a local model through `cline -P ollama` included. Models are addressed
//! as `provider/model`.
//!
//! Verified in CodeFlow against cline 3.0.55:
//!
//!   1. **The prompt is argv-only.** The CLI never reads stdin, so the whole brief travels as one
//!      argument — *exactly one*, and it must contain whitespace, or the CLI rejects it as
//!      "Unknown command or unquoted prompt".
//!   2. **On Windows that argument can't hold the brief**: an npm install lands as `cline.cmd`, and
//!      cmd.exe refuses arguments containing a newline. There the brief goes to a private file and
//!      the argument becomes a one-line pointer to it — Cline reads it with its own file tool.
//!   3. **`--system` replaces Cline's own system prompt**, which would throw away the agent's tool
//!      instructions (and with them its ability to read the file above), so the system prompt is
//!      folded into the brief instead.
//!
//! A text-only run gets **plan mode** (`--plan`, which hard-blocks every mutation) with
//! `--auto-approve true` — a headless run has no terminal to answer a prompt on — and an empty
//! scratch directory as `--cwd`, so "no repository" is what the run actually sees. Measured, not
//! assumed: act mode spiralled into editor loops on text tasks; plan mode cannot.
//!
//! **Output.** `--json` prints one event per line and closes with a `run_result` whose `text` is the
//! reply — or, when `finishReason` is not `completed`, the error.

use serde::Deserialize;

use super::files::CallFiles;
use super::text::{killed_without_output, quota_signal, refusal_reply, QUOTA_MARKER};
use super::{Invocation, TextCall};

/// Single-line, ASCII, shim-safe stand-in for a brief that can't be passed as an argument — the
/// *model* opens the file. It has whitespace in it, which the CLI's prompt check requires.
const FILE_POINTER: &str = "Read the file at this exact path with your file tool: it holds your complete instructions and \
     the input for this turn. Follow them exactly and reply with only the requested output. Path: ";

/// Above this many characters the brief goes to a file even where the platform would accept it
/// inline — a guard well under macOS's ~1 MB `ARG_MAX`. Linux caps a *single* argument at 128 KiB
/// (`MAX_ARG_STRLEN`), so it is lower there.
#[cfg(not(target_os = "linux"))]
const MAX_INLINE_PROMPT_CHARS: usize = 200_000;
#[cfg(target_os = "linux")]
const MAX_INLINE_PROMPT_CHARS: usize = 100_000;

/// What the user is told when both attempts came back empty. A constant because [`retry_once_on`]
/// has to recognise it: by the time the retry decision is made, the CLI's words have been replaced.
const EMPTY_ANSWER_MESSAGE: &str = "The model returned no answer, even on a second attempt. Small local models do this now \
     and then: try again, or use a larger model for this task.";

/// Whether this brief has to be handed over as a file rather than as an argument.
fn needs_file_handoff(brief: &str) -> bool {
    cfg!(windows) || brief.chars().count() > MAX_INLINE_PROMPT_CHARS
}

/// Splits on the *first* separator only: Cline's hosted catalog addresses models as
/// `vendor/model`, so a greedy split would invent a provider. No separator, no `--provider` flag.
fn split_model(model: &str) -> (Option<&str>, &str) {
    let model = model.trim();
    match model.split_once('/') {
        Some((provider, rest)) if !provider.is_empty() && !rest.is_empty() => (Some(provider), rest),
        _ => (None, model),
    }
}

pub(super) fn invocation(call: &TextCall<'_>, files: &CallFiles) -> Invocation {
    invocation_with(call, files, None)
}

/// `handoff` forces the file delivery one way or the other, for the tests; `None` decides it.
fn invocation_with(call: &TextCall<'_>, files: &CallFiles, handoff: Option<bool>) -> Invocation {
    let mut args: Vec<String> = vec!["--json".into(), "--plan".into(), "--auto-approve".into(), "true".into()];
    let scratch = files.dir("cline-scratch");
    if let Some(dir) = &scratch {
        args.extend(["--cwd".into(), dir.to_string_lossy().into_owned()]);
    }
    let (provider, model) = split_model(call.model);
    if let Some(provider) = provider {
        args.extend(["--provider".into(), provider.into()]);
    }
    if !model.is_empty() {
        args.extend(["--model".into(), model.into()]);
    }
    // The prompt is positional and goes last, after every flag. A failed write falls back to the
    // inline brief — worse on the platform that needed the file, but likelier than certain failure.
    let brief = call.brief(true);
    let prompt = if handoff.unwrap_or_else(|| needs_file_handoff(&brief)) {
        match files.write("cline", "md", &brief) {
            Some(path) => format!("{FILE_POINTER}{}", path.to_string_lossy()),
            None => brief,
        }
    } else {
        brief
    };
    args.push(prompt);
    // Nothing on stdin: the CLI ignores it entirely.
    Invocation { args, stdin: String::new(), cwd: scratch, env: Vec::new() }
}

/// The final `run_result` line of a JSON-mode run.
#[derive(Deserialize)]
struct RunResult {
    #[serde(rename = "finishReason", default)]
    finish_reason: String,
    #[serde(default)]
    text: String,
    #[serde(default)]
    usage: Option<RunUsage>,
}

#[derive(Deserialize)]
struct RunUsage {
    #[serde(rename = "outputTokens", default)]
    output_tokens: i64,
}

/// A CLI-level failure line: `{"type":"error","message":"…"}`.
#[derive(Deserialize)]
struct ErrorLine {
    #[serde(rename = "type", default)]
    kind: String,
    #[serde(default)]
    message: String,
}

/// The last `run_result` in a stream — the one that describes how the turn *ended*.
fn parse_run_result(stdout: &str) -> Option<RunResult> {
    stdout
        .lines()
        .map(str::trim)
        .filter(|line| line.starts_with('{') && line.contains("\"run_result\""))
        .filter_map(|line| serde_json::from_str::<RunResult>(line).ok())
        .next_back()
}

/// The **last** `{"type":"error"}` message in a buffer: a failing run emits its real cause after
/// whatever incidental complaint came first.
fn parse_error_line(text: &str) -> Option<String> {
    text.lines()
        .map(str::trim)
        .filter(|line| line.starts_with('{'))
        .filter_map(|line| serde_json::from_str::<ErrorLine>(line).ok())
        .filter(|error| error.kind == "error")
        .map(|error| error.message.trim().to_string())
        .rfind(|message| !message.is_empty())
}

fn nonblank(text: &str) -> Option<String> {
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_string())
}

/// Rewrites "the provider answered with nothing" — a *model* failure, common with small local
/// models — into something a person can act on.
fn empty_answer_hint(detail: &str) -> Option<String> {
    let empty = detail.contains("Model returned empty response") || detail.contains("Model stream failed");
    empty.then(|| EMPTY_ANSWER_MESSAGE.to_string())
}

/// A turn that came back with nothing in it is worth asking again, once: nothing about the request
/// is wrong, and a second attempt is the remedy a person would apply themselves.
pub(super) fn retry_once_on(error: &str) -> bool {
    empty_answer_hint(error).is_some() || error == EMPTY_ANSWER_MESSAGE
}

/// Turns one finished `cline --json` run into its reply or a user-facing error. The `run_result` is
/// authoritative in both directions, so it is consulted before the exit status and stderr.
pub(super) fn interpret(success: bool, status_label: &str, stdout: &str, stderr: &str) -> Result<String, String> {
    if let Some(result) = parse_run_result(stdout) {
        let text = result.text.trim().to_string();
        if result.finish_reason != "completed" {
            let from_stderr = parse_error_line(stderr).unwrap_or_default();
            let detail = [text.as_str(), from_stderr.as_str(), stderr.trim()]
                .into_iter()
                .find(|s| !s.is_empty())
                .unwrap_or("no detail")
                .to_string();
            if quota_signal(&detail) {
                return Err(format!("{QUOTA_MARKER}{detail}"));
            }
            return Err(empty_answer_hint(&detail)
                .unwrap_or_else(|| format!("cline finished with '{}': {detail}", result.finish_reason)));
        }
        if text.is_empty() {
            return Err("cline returned no content".to_string());
        }
        if refusal_reply(&text, result.usage.as_ref().map(|u| u.output_tokens)) {
            return Err(format!("{QUOTA_MARKER}{text}"));
        }
        return Ok(text);
    }

    // No `run_result` at all: the CLI failed before the agent ever started (a bad flag, a prompt it
    // refused to parse), which it reports as a single error line.
    let detail = parse_error_line(stderr).or_else(|| parse_error_line(stdout)).or_else(|| nonblank(stderr)).or_else(|| nonblank(stdout));
    let Some(detail) = detail else {
        return Err(if success {
            "cline returned no content".to_string()
        } else {
            killed_without_output("cline", status_label)
                .unwrap_or_else(|| format!("cline failed ({status_label}) with no output on stdout or stderr"))
        });
    };
    if quota_signal(&detail) {
        return Err(format!("{QUOTA_MARKER}{detail}"));
    }
    Err(format!("cline failed ({status_label}): {detail}"))
}

#[cfg(test)]
mod tests {
    use super::super::test_util::{after, has_pair};
    use super::*;

    #[test]
    fn a_text_only_run_is_planned_inside_a_scratch_directory() {
        let files = CallFiles::new();
        let call = TextCall { system: "Eres un redactor.", ask: "Describe.", data: "TÍTULO: Reunión", model: "ollama/qwen2.5:7b-instruct" };
        let inv = invocation_with(&call, &files, Some(false));
        let args = &inv.args;
        assert!(args.iter().any(|a| a == "--plan") && args.iter().any(|a| a == "--json"));
        assert!(has_pair(args, "--auto-approve", "true"));
        assert!(has_pair(args, "--provider", "ollama") && has_pair(args, "--model", "qwen2.5:7b-instruct"));
        let dir = std::path::Path::new(after(args, "--cwd").expect("a directory is always given"));
        assert!(dir.is_dir() && dir.read_dir().unwrap().next().is_none(), "and it is empty");
        // One positional, last, with the system prompt first and the data last.
        let prompt = args.last().unwrap();
        let (system, ask, data) = (prompt.find("Eres un redactor.").unwrap(), prompt.find("Describe.").unwrap(), prompt.find("TÍTULO").unwrap());
        assert!(system < ask && ask < data, "{prompt}");
        assert!(inv.stdin.is_empty());
        assert!(!args.iter().any(|a| a == "--system"));
    }

    /// On Windows (or past the inline limit) the argument is a one-line pointer to a private file.
    #[test]
    fn a_handed_off_brief_is_a_pointer_to_a_file() {
        let files = CallFiles::new();
        let call = TextCall { system: "S", ask: "A\nB", data: "D", model: "" };
        let inv = invocation_with(&call, &files, Some(true));
        let prompt = inv.args.last().unwrap();
        let path = prompt.strip_prefix(FILE_POINTER).expect("the pointer");
        assert_eq!(std::fs::read_to_string(path).unwrap(), "S\n\nA\nB\n\n----- INPUT -----\n\nD");
        assert!(!prompt.contains('\n'));
        assert!(needs_file_handoff(&"x".repeat(MAX_INLINE_PROMPT_CHARS + 1)));
    }

    #[test]
    fn a_model_id_carries_its_provider_and_keeps_its_own_slashes() {
        assert_eq!(split_model("ollama/qwen2.5:7b-instruct"), (Some("ollama"), "qwen2.5:7b-instruct"));
        assert_eq!(split_model("cline/dots-studio/dots-3-note-preview:free"), (Some("cline"), "dots-studio/dots-3-note-preview:free"));
        assert_eq!(split_model("gpt-5"), (None, "gpt-5"));
        assert_eq!(split_model("/x"), (None, "/x"));
    }

    const COMPLETED: &str = concat!(
        r#"{"ts":"2026-08-16T20:20:21.320Z","type":"hook_event","hookEventName":"agent_start","agentId":"agent_1","taskId":"conv_1"}"#,
        "\n",
        r#"{"ts":"2026-08-16T20:20:21.325Z","type":"agent_event","event":{"type":"iteration_start","iteration":1}}"#,
        "\n",
        r#"{"ts":"2026-08-16T20:20:53.162Z","type":"run_result","finishReason":"completed","iterations":1,"#,
        r#""usage":{"inputTokens":4368,"outputTokens":2,"cacheReadTokens":7,"cacheWriteTokens":3,"totalCost":0},"#,
        r#""durationMs":31801,"text":"  feat: add thing  ","model":{"id":"qwen2.5:7b-instruct","provider":"ollama"}}"#,
        "\n",
    );

    #[test]
    fn reads_the_reply_out_of_the_run_result() {
        assert_eq!(interpret(true, "exit status: 0", COMPLETED, "").unwrap(), "feat: add thing");
    }

    #[test]
    fn a_failed_run_reports_the_reason_from_its_run_result() {
        let stdout = concat!(
            r#"{"type":"run_result","finishReason":"error","iterations":1,"#,
            r#""usage":{"inputTokens":0,"outputTokens":0,"cacheReadTokens":0,"cacheWriteTokens":0,"totalCost":0},"#,
            r#""durationMs":20,"text":"model 'no-existe' not found","model":{"id":"no-existe","provider":"ollama"}}"#,
        );
        assert!(interpret(false, "exit status: 1", stdout, "").unwrap_err().contains("model 'no-existe' not found"));
    }

    #[test]
    fn the_last_error_line_wins_over_the_incidental_first_one() {
        let stderr = concat!(
            r#"{"ts":"1","type":"error","message":"hook dispatch failed: session.hook requires a valid hook event payload"}"#,
            "\n",
            r#"{"ts":"2","type":"error","message":"model 'no-existe' not found"}"#,
            "\n",
        );
        let err = interpret(false, "exit status: 1", "", stderr).unwrap_err();
        assert!(err.contains("model 'no-existe' not found") && !err.contains("hook dispatch"), "{err}");
    }

    #[test]
    fn an_empty_answer_is_explained_and_retried() {
        let stdout = r#"{"type":"run_result","finishReason":"error","text":"Model returned empty response","model":{"id":"q","provider":"ollama"}}"#;
        let err = interpret(false, "exit status: 1", stdout, "").unwrap_err();
        assert!(err.contains("no answer") && !err.contains("Model returned empty"), "{err}");
        assert!(retry_once_on(&err));
        assert!(!retry_once_on("cline finished with 'error': model 'x' not found"));
        assert!(!retry_once_on(&format!("{QUOTA_MARKER}rate limit")));
    }

    #[test]
    fn a_quota_refusal_gets_the_marker() {
        let stdout = r#"{"type":"run_result","finishReason":"error","text":"You have hit your rate limit, try again in 1h","model":{"id":"m","provider":"cline"}}"#;
        assert!(interpret(false, "exit status: 1", stdout, "").unwrap_err().starts_with(QUOTA_MARKER));
    }

    #[test]
    fn a_prestart_failure_is_read_from_its_error_line() {
        let stderr = r#"{"ts":"1","type":"error","message":"JSON output mode requires a prompt argument"}"#;
        assert!(interpret(false, "exit status: 1", "", stderr).unwrap_err().contains("JSON output mode requires a prompt argument"));
    }

    #[test]
    fn empty_replies_and_silent_kills_are_explained() {
        let stdout = r#"{"type":"run_result","finishReason":"completed","text":"   ","model":{"id":"m","provider":"ollama"}}"#;
        assert_eq!(interpret(true, "exit status: 0", stdout, "").unwrap_err(), "cline returned no content");
        assert!(interpret(false, "signal: 9 (SIGKILL)", "", "").unwrap_err().contains("137"));
        assert!(interpret(false, "exit status: 1", "", "").unwrap_err().contains("no output on stdout or stderr"));
    }
}
