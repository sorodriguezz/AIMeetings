//! The Grok Build CLI (`grok`), text-only.
//!
//! Verified in CodeFlow against grok 0.2.117 and 1.0.4:
//!
//!   - `grok -p "<prompt>"` runs one turn non-interactively; `--prompt-file <path>` takes the whole
//!     prompt from a file — the file is the *prompt*, not one the agent must be allowed to open. The
//!     brief rides `-p` only when it is short **and** single-line; anything else goes to a private
//!     file, because a multi-line argument is unsafe on a Windows `.cmd` shim and a transcript is
//!     past the ~32k Windows command-line limit anyway.
//!   - `--output-format json` prints a single JSON object on stdout whose `text` is the reply.
//!
//! **What a text-only run is.** grok's own agent prompt is replaced by a one-line one
//! (`--system-prompt-override`; the call's own instructions stay in the brief, which may be several
//! lines), the tools narrow to one that only reads (`--tools read_file`) and MCP calls are denied
//! (`--deny MCPTool`) — all three predate the 1.0.4 baseline. Never `--always-approve`. The run
//! works in an empty folder of its own.

use super::files::CallFiles;
use super::text::{first_nonempty, quota_signal, refusal_reply, QUOTA_MARKER};
use super::{Invocation, TextCall, TEXT_ONLY_SYSTEM_PROMPT};

/// Above this many chars the brief goes to `--prompt-file` even when it is a single line.
const INLINE_LIMIT: usize = 12_000;

/// Whether `content` must leave the command line: too long, or carrying a newline.
fn needs_file(content: &str) -> bool {
    content.len() > INLINE_LIMIT || content.contains('\n')
}

pub(super) fn invocation(call: &TextCall<'_>, files: &CallFiles) -> Invocation {
    let brief = call.brief(true);
    let mut args: Vec<String> = Vec::new();
    // A failed write degrades to an inline attempt rather than failing the call outright.
    match needs_file(&brief).then(|| files.write("grok", "txt", &brief)).flatten() {
        Some(file) => args.extend(["--prompt-file".into(), file.to_string_lossy().into_owned()]),
        None => args.extend(["-p".into(), brief]),
    }
    args.extend(["--output-format".into(), "json".into()]);
    if !call.model.trim().is_empty() {
        args.extend(["--model".into(), call.model.trim().into()]);
    }
    args.extend(["--system-prompt-override".into(), TEXT_ONLY_SYSTEM_PROMPT.into()]);
    args.extend(["--tools".into(), "read_file".into()]);
    args.extend(["--deny".into(), "MCPTool".into()]);
    // The data rides in the brief; piped anyway, as the original did, and closed at EOF.
    Invocation { args, stdin: call.data.to_string(), cwd: files.dir("grok-scratch"), env: Vec::new() }
}

/// The shape of `--output-format json`. Only the fields acted on are named, so a new field upstream
/// can't fail the parse.
#[derive(serde::Deserialize)]
struct GrokReply {
    #[serde(default)]
    text: String,
    #[serde(default)]
    usage: Option<GrokUsage>,
}

#[derive(Default, serde::Deserialize)]
struct GrokUsage {
    #[serde(default)]
    output_tokens: i64,
}

/// Grok prints one JSON object on stdout in headless mode; banners and errors go to stderr.
pub(super) fn interpret(success: bool, status_label: &str, stdout: &str, stderr: &str) -> Result<String, String> {
    if !success {
        if quota_signal(stderr) {
            return Err(format!("{QUOTA_MARKER}{}", stderr.trim()));
        }
        if quota_signal(stdout) {
            return Err(format!("{QUOTA_MARKER}{}", stdout.trim()));
        }
        let detail = first_nonempty(&[stderr, stdout], "no output on stdout or stderr");
        return Err(format!("grok exited with an error ({status_label}): {detail}"));
    }
    let raw = stdout.trim();
    if raw.is_empty() {
        let err = stderr.trim();
        return Err(if err.is_empty() { "grok produced no output".to_string() } else { err.to_string() });
    }
    // A clean exit whose stdout isn't the JSON asked for means something printed over it: a usable
    // answer in the wrong shape still beats an error.
    let Ok(reply) = serde_json::from_str::<GrokReply>(raw) else {
        if refusal_reply(raw, None) {
            return Err(format!("{QUOTA_MARKER}{raw}"));
        }
        return Ok(raw.to_string());
    };
    let text = reply.text.trim();
    if text.is_empty() {
        return Err("grok produced no output".to_string());
    }
    if refusal_reply(text, reply.usage.as_ref().map(|u| u.output_tokens)) {
        return Err(format!("{QUOTA_MARKER}{text}"));
    }
    Ok(text.to_string())
}

#[cfg(test)]
mod tests {
    use super::super::test_util::{after, has_pair};
    use super::*;

    const SAMPLE: &str = r#"{
      "text": "OK",
      "stopReason": "end_turn",
      "sessionId": "019fb616-cde1-7a51-9aec-aa06273c4552",
      "requestId": "aee555aa-043a-4f56-b7d2-b68ca5de469e",
      "usage": { "input_tokens": 13820, "output_tokens": 22 },
      "num_turns": 1,
      "total_cost_usd": 0.0280792,
      "modelUsage": { "grok-4.5": { "inputTokens": 13820, "outputTokens": 22 } }
    }"#;

    #[test]
    fn reads_the_reply_from_the_json() {
        assert_eq!(interpret(true, "exit status: 0", SAMPLE, "").unwrap(), "OK");
    }

    #[test]
    fn non_json_stdout_on_a_clean_exit_is_still_treated_as_the_reply() {
        assert_eq!(interpret(true, "exit status: 0", "plain words", "").unwrap(), "plain words");
    }

    #[test]
    fn surfaces_the_failure_detail() {
        let err = interpret(false, "exit status: 1", "", "not signed in — run `grok login`").unwrap_err();
        assert_eq!(err, "grok exited with an error (exit status: 1): not signed in — run `grok login`");
    }

    #[test]
    fn a_quota_message_gets_the_marker() {
        assert!(interpret(false, "exit status: 1", "", "rate limit exceeded, try again later").unwrap_err().starts_with(QUOTA_MARKER));
    }

    #[test]
    fn empty_output_on_a_clean_exit_is_an_error_not_a_blank_reply() {
        assert_eq!(interpret(true, "exit status: 0", "   ", "").unwrap_err(), "grok produced no output");
        assert_eq!(interpret(true, "exit status: 0", r#"{"text":"  "}"#, "").unwrap_err(), "grok produced no output");
    }

    #[test]
    fn a_text_only_run_replaces_the_agent_prompt_and_keeps_one_reading_tool() {
        let files = CallFiles::new();
        let inv = invocation(&TextCall { system: "", ask: "Escribe un título", data: "", model: "grok-4.5" }, &files);
        let args = &inv.args;
        assert_eq!(after(args, "-p"), Some("Escribe un título"), "short and single-line rides -p");
        assert!(has_pair(args, "--system-prompt-override", TEXT_ONLY_SYSTEM_PROMPT));
        assert!(has_pair(args, "--tools", "read_file") && has_pair(args, "--deny", "MCPTool"));
        assert!(has_pair(args, "--output-format", "json") && has_pair(args, "--model", "grok-4.5"));
        assert!(!args.iter().any(|a| a == "--always-approve"));
        let cwd = inv.cwd.as_ref().expect("a scratch directory");
        assert_eq!(std::fs::read_dir(cwd).unwrap().count(), 0);
    }

    /// A system prompt or data makes the brief multi-line — the common case — so it goes to a file,
    /// and the file goes with the call.
    #[test]
    fn a_multiline_brief_moves_to_a_prompt_file() {
        assert!(!needs_file("hola"));
        assert!(needs_file("system prompt\n\nthe ask"));
        let files = CallFiles::new();
        let inv = invocation(&TextCall { system: "Eres quien resume.", ask: "Resume", data: "A: hola", model: "" }, &files);
        let path = std::path::PathBuf::from(after(&inv.args, "--prompt-file").expect("a file"));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "Eres quien resume.\n\nResume\n\n----- INPUT -----\n\nA: hola");
        assert!(!inv.args.iter().any(|a| a == "-p"));
        drop(files);
        assert!(!path.exists());
    }
}
