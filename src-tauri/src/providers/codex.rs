//! The Codex CLI (`codex exec`), text-only — OpenAI's models through the ChatGPT subscription the
//! user logged into with `codex login`.
//!
//! Headless contract (`codex exec --json`): runs one task to completion, streams progress to
//! **stderr** and a JSONL event stream to **stdout**, with no approval prompts. Piped stdin is taken
//! as input alongside the prompt argument — and that is where the brief goes: the argument is a
//! single-line pointer ([`POINTER`]), because a CLI installed as an npm `.cmd` shim on Windows can't
//! receive a multi-line argument.
//!
//! **What a text-only run is** ([`TEXT_ONLY_OFF`] and around it, measured in CodeFlow on 0.163.0:
//! a title cost 15,161 input tokens before and 3,877 after): `model_instructions_file` replaces
//! Codex's instructions with the call's system prompt; the `include_*` keys drop the blocks written
//! around them, the `features.*` keys drop the tools, `skills.*` the skills listing. Every one is a
//! config key set with `-c`, so a build that renames one ignores it rather than refusing the run the
//! way an unknown flag would. `--ignore-user-config` keeps the user's `config.toml` out (its MCP
//! servers, plugins, hooks and `notify` program) — except when that file names its own
//! `model_provider`, which is then how this CLI reaches a model at all. An empty scratch directory
//! as the workspace, a `read-only` sandbox and `--ephemeral`: nothing to read, nothing written to
//! the user's history.
//!
//! **Output.** The reply is assembled from `item.completed` events carrying an `agent_message`; the
//! reason a turn failed is a `turn.failed` or `error` event on stdout (stderr then holds only the
//! "Reading additional input from stdin" banner). A stdout that parses as no events at all is taken
//! verbatim — the pre-`--json` behaviour, and the safety net if the flag stops being accepted.

use super::files::CallFiles;
use super::text::{first_nonempty, quota_signal, refusal_reply, QUOTA_MARKER};
use super::{Invocation, TextCall};

/// Single-line, ASCII, shim-safe. The real instructions arrive on stdin.
const POINTER: &str = "Follow the instructions in the input piped on stdin and reply with only the requested output.";

/// The config keys a text-only run switches off — see the module docs.
const TEXT_ONLY_OFF: &[&str] = &[
    "include_permissions_instructions",
    "include_apps_instructions",
    "include_environment_context",
    "include_collaboration_mode_instructions",
    "features.shell_tool",
    "features.unified_exec",
    "features.view_image",
    "features.multi_agent",
    "features.goals",
    "features.standalone_web_search",
    "features.request_permissions_tool",
    "features.default_mode_request_user_input",
    "features.apps",
    "features.memories",
    "features.image_generation",
    "features.tool_suggest",
    "features.plugins",
    "skills.include_instructions",
    "skills.bundled.enabled",
];

/// Codex's state directory: `$CODEX_HOME` when set (the CLI's own override), else `~/.codex`.
fn codex_home() -> Option<std::path::PathBuf> {
    if let Some(dir) = std::env::var_os("CODEX_HOME").filter(|d| !d.is_empty()) {
        return Some(std::path::PathBuf::from(dir));
    }
    Some(dirs::home_dir()?.join(".codex"))
}

/// A TOML basic string, for a `-c key=value` whose value is a path.
fn toml_string(text: &str) -> String {
    format!("\"{}\"", text.replace('\\', "\\\\").replace('"', "\\\""))
}

/// Whether the user's Codex config chooses its own `model_provider` (an OSS server, a gateway) —
/// the one thing `--ignore-user-config` cannot take away from a run without leaving it no model.
fn user_config_names_a_provider() -> bool {
    let Some(text) = codex_home().and_then(|h| std::fs::read_to_string(h.join("config.toml")).ok()) else { return false };
    names_a_provider(&text)
}

/// A top-level `model_provider` other than OpenAI's own. Keys after the first `[table]` belong to
/// that table.
fn names_a_provider(config: &str) -> bool {
    for line in config.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            return false;
        }
        if let Some(value) = line.strip_prefix("model_provider").map(str::trim_start).and_then(|rest| rest.strip_prefix('=')) {
            let value = value.trim().trim_matches(|c| c == '"' || c == '\'');
            return !value.is_empty() && value != "openai";
        }
    }
    false
}

pub(super) fn invocation(call: &TextCall<'_>, files: &CallFiles) -> Invocation {
    invocation_with(call, files, user_config_names_a_provider())
}

fn invocation_with(call: &TextCall<'_>, files: &CallFiles, keep_user_config: bool) -> Invocation {
    let mut args: Vec<String> = vec!["exec".into(), POINTER.into()];
    if !call.model.trim().is_empty() {
        args.extend(["--model".into(), call.model.trim().into()]);
    }
    let instructions = files.write("codex-instructions", "md", call.system_or_default());
    if let Some(path) = &instructions {
        args.extend(["-c".into(), format!("model_instructions_file={}", toml_string(&path.to_string_lossy()))]);
    }
    for key in TEXT_ONLY_OFF {
        args.extend(["-c".into(), format!("{key}=false")]);
    }
    args.extend(["-c".into(), "web_search=\"disabled\"".into()]);
    if !keep_user_config {
        args.push("--ignore-user-config".into());
    }
    args.push("--ephemeral".into());
    args.extend(["--sandbox".into(), "read-only".into()]);
    // Headless runs can't answer an approval prompt. Through `-c` rather than
    // `--ask-for-approval`, which `codex exec` dropped on 0.145+.
    args.extend(["-c".into(), "approval_policy=\"never\"".into()]);
    args.push("--json".into());
    let scratch = files.dir("codex-scratch");
    if let Some(dir) = &scratch {
        // `--cd` sets the workspace root the sandbox is scoped to.
        args.extend(["--cd".into(), dir.to_string_lossy().into_owned()]);
    }
    // Codex refuses to start outside a Git repository; an empty scratch folder has nothing to lose
    // an undo for, and the sandbox is read-only.
    args.push("--skip-git-repo-check".into());

    // The system prompt is said once, in the instructions — unless that file could not be written,
    // in which case it leads the brief rather than being lost.
    let brief = call.brief(instructions.is_none());
    Invocation { args, stdin: brief, cwd: scratch, env: Vec::new() }
}

/// What one `codex exec --json` run said: the agent's reply, the tokens it generated, and why it
/// failed.
struct CodexEvents {
    message: String,
    output_tokens: Option<i64>,
    /// The last `turn.failed` reason, else the last `error` event's — already unwrapped from the
    /// API's JSON envelope when it arrived in one.
    error: Option<String>,
}

/// `stderr` without the line `codex exec` prints whenever its stdin is piped, which is every run
/// here: it says nothing about why one failed.
fn without_stdin_banner(stderr: &str) -> String {
    stderr
        .lines()
        .filter(|line| !line.trim().starts_with("Reading additional input from stdin"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// An error event's message as a person reads it. The API's own errors arrive as JSON text inside
/// the message; the sentence is the inner `message`.
fn error_text(message: &str) -> String {
    let trimmed = message.trim();
    serde_json::from_str::<serde_json::Value>(trimmed)
        .ok()
        .and_then(|value| {
            value.pointer("/error/message").or_else(|| value.get("message")).and_then(|m| m.as_str()).map(str::to_string)
        })
        .filter(|text| !text.trim().is_empty())
        .unwrap_or_else(|| trimmed.to_string())
}

/// Reads the JSONL event stream. `None` means stdout held no events at all — not that it held none
/// of interest — so the caller can tell an old-style plain-text reply from a run that said nothing.
fn parse_events(stdout: &str) -> Option<CodexEvents> {
    let mut saw_event = false;
    let mut message = String::new();
    let mut output_tokens = None;
    let mut failed: Option<String> = None;
    let mut error: Option<String> = None;

    for line in stdout.lines() {
        let line = line.trim();
        if !line.starts_with('{') {
            continue;
        }
        let Ok(event) = serde_json::from_str::<serde_json::Value>(line) else { continue };
        let Some(kind) = event.get("type").and_then(|t| t.as_str()) else { continue };
        saw_event = true;

        match kind {
            "item.completed" => {
                let item = event.get("item");
                let is_message = item.and_then(|i| i.get("type")).and_then(|t| t.as_str()) == Some("agent_message");
                if let (true, Some(text)) = (is_message, item.and_then(|i| i.get("text")).and_then(|t| t.as_str())) {
                    if !message.is_empty() {
                        message.push_str("\n\n");
                    }
                    message.push_str(text);
                }
            }
            "turn.completed" => {
                output_tokens = event.pointer("/usage/output_tokens").and_then(|v| v.as_i64()).or(output_tokens);
            }
            // An `item.completed` of type `error` is a warning the run went on past, so only these
            // two say why it stopped — `turn.failed` the more precisely.
            "turn.failed" => {
                if let Some(text) = event.pointer("/error/message").and_then(|m| m.as_str()) {
                    failed = Some(error_text(text));
                }
            }
            "error" => {
                if let Some(text) = event.get("message").and_then(|m| m.as_str()) {
                    error = Some(error_text(text));
                }
            }
            _ => {}
        }
    }

    saw_event.then_some(CodexEvents { message, output_tokens, error: failed.or(error) })
}

/// Turns one finished `codex exec --json` run into its reply or a user-facing error.
pub(super) fn interpret(success: bool, status_label: &str, stdout: &str, stderr: &str) -> Result<String, String> {
    if !success {
        if quota_signal(stderr) {
            return Err(format!("{QUOTA_MARKER}{}", stderr.trim()));
        }
        if quota_signal(stdout) {
            return Err(format!("{QUOTA_MARKER}{}", stdout.trim()));
        }
        let failure = parse_events(stdout).and_then(|events| events.error);
        let stderr = without_stdin_banner(stderr);
        let detail = first_nonempty(&[failure.as_deref().unwrap_or(""), &stderr, stdout], "no output on stdout or stderr");
        return Err(format!("codex exited with an error ({status_label}): {detail}"));
    }

    let events = parse_events(stdout);
    let text = match &events {
        Some(events) => events.message.trim().to_string(),
        None => stdout.trim().to_string(),
    };
    if text.is_empty() {
        let err = without_stdin_banner(stderr);
        let err = err.trim();
        return Err(if err.is_empty() { "codex produced no output".to_string() } else { err.to_string() });
    }
    if refusal_reply(&text, events.as_ref().and_then(|events| events.output_tokens)) {
        return Err(format!("{QUOTA_MARKER}{text}"));
    }
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::super::test_util::{after, has_pair};
    use super::*;

    #[test]
    fn a_text_only_run_brings_its_own_instructions_and_nothing_of_the_agent() {
        let files = CallFiles::new();
        let call = TextCall { system: "Eres quien resume.", ask: "Resume", data: "la reunión", model: "gpt-5.5" };
        let inv = invocation_with(&call, &files, false);
        let args = &inv.args;
        assert_eq!(&args[..2], &["exec".to_string(), POINTER.to_string()]);
        assert!(has_pair(args, "--model", "gpt-5.5"));
        let at = args.iter().position(|a| a.starts_with("model_instructions_file=")).expect("its own instructions");
        let path = args[at].trim_start_matches("model_instructions_file=").trim_matches('"').replace("\\\\", "\\");
        assert_eq!(std::fs::read_to_string(path).unwrap(), "Eres quien resume.");
        for off in ["features.shell_tool=false", "features.plugins=false", "skills.include_instructions=false", "include_environment_context=false"] {
            assert!(args.iter().any(|a| a == off), "{off}: {args:?}");
        }
        assert!(has_pair(args, "--sandbox", "read-only"));
        assert!(has_pair(args, "-c", "approval_policy=\"never\""));
        assert!(has_pair(args, "-c", "web_search=\"disabled\""));
        for flag in ["--ephemeral", "--skip-git-repo-check", "--json", "--ignore-user-config"] {
            assert!(args.iter().any(|a| a == flag), "{flag}: {args:?}");
        }
        assert!(!args.iter().any(|a| a.contains("mcp_servers.")));
        // Said once, in the instructions — not again at the head of the brief.
        assert!(!inv.stdin.contains("Eres quien resume.") && inv.stdin.starts_with("Resume") && inv.stdin.ends_with("la reunión"));
        // Its workspace is an empty folder of its own.
        let cwd = inv.cwd.as_ref().expect("a scratch directory");
        assert_eq!(after(args, "--cd"), Some(cwd.to_string_lossy().as_ref()));
        assert_eq!(std::fs::read_dir(cwd).unwrap().count(), 0);
        assert!(args.iter().all(|a| !a.contains('\n')));
    }

    #[test]
    fn a_users_own_provider_keeps_their_config() {
        let files = CallFiles::new();
        let call = TextCall { system: "", ask: "a", data: "", model: "" };
        assert!(!invocation_with(&call, &files, true).args.iter().any(|a| a == "--ignore-user-config"));
        assert!(!names_a_provider("model = \"gpt-5.6\"\n[mcp_servers.x]\nmodel_provider = \"x\"\n"));
        assert!(!names_a_provider("model_provider = \"openai\"\n"));
        assert!(names_a_provider("model = \"qwen3\"\nmodel_provider = \"ollama\"\n"));
        assert!(names_a_provider("model_provider='azure'\n"));
    }

    /// Captured verbatim from `codex exec --json` on 0.147.0.
    const EVENTS: &str = concat!(
        r#"{"type":"thread.started","thread_id":"th_1"}"#,
        "\n",
        r#"{"type":"turn.started"}"#,
        "\n",
        r#"{"type":"item.completed","item":{"id":"item_0","type":"agent_message","text":"PONG"}}"#,
        "\n",
        r#"{"type":"turn.completed","usage":{"input_tokens":12975,"cached_input_tokens":12672,"#,
        r#""cache_write_input_tokens":0,"output_tokens":30,"reasoning_output_tokens":22}}"#,
        "\n",
    );

    #[test]
    fn reads_the_reply_from_the_event_stream() {
        assert_eq!(interpret(true, "exit status: 0", EVENTS, "").unwrap(), "PONG");
    }

    #[test]
    fn plain_stdout_is_still_a_reply() {
        assert_eq!(interpret(true, "exit status: 0", "just the answer\n", "").unwrap(), "just the answer");
    }

    #[test]
    fn events_without_a_message_are_an_error() {
        let only_progress = concat!(r#"{"type":"turn.started"}"#, "\n", r#"{"type":"turn.completed"}"#, "\n");
        assert!(interpret(true, "exit status: 0", only_progress, "Reading additional input from stdin...").is_err());
    }

    #[test]
    fn multiple_agent_messages_are_joined() {
        let two = concat!(
            r#"{"type":"item.completed","item":{"type":"agent_message","text":"first"}}"#,
            "\n",
            r#"{"type":"item.completed","item":{"type":"agent_message","text":"second"}}"#,
            "\n",
        );
        assert_eq!(interpret(true, "exit status: 0", two, "").unwrap(), "first\n\nsecond");
    }

    #[test]
    fn surfaces_the_failure_detail() {
        let err = interpret(false, "exit status: 1", "", "not logged in — run `codex login`").unwrap_err();
        assert_eq!(err, "codex exited with an error (exit status: 1): not logged in — run `codex login`");
    }

    /// The reason is on stdout as events, and stderr holds only the piped-stdin banner.
    #[test]
    fn a_failed_turn_says_why_rather_than_the_stdin_banner() {
        let stdout = concat!(
            r#"{"type":"thread.started","thread_id":"01a11900-c919-7923-bd5e-aef75cde6804"}"#, "\n",
            r#"{"type":"item.completed","item":{"id":"item_0","type":"error","message":"Model metadata for `example-model` not found."}}"#, "\n",
            r#"{"type":"turn.started"}"#, "\n",
            r#"{"type":"error","message":"{\"type\":\"error\",\"status\":400,\"error\":{\"type\":\"invalid_request_error\",\"message\":\"The 'example-model' model is not supported.\"}}"}"#, "\n",
            r#"{"type":"turn.failed","error":{"message":"{\"type\":\"error\",\"status\":400,\"error\":{\"type\":\"invalid_request_error\",\"message\":\"The 'example-model' model is not supported.\"}}"}}"#, "\n",
        );
        let err = interpret(false, "exit status: 1", stdout, "Reading additional input from stdin...\n").unwrap_err();
        assert_eq!(err, "codex exited with an error (exit status: 1): The 'example-model' model is not supported.");
    }

    #[test]
    fn error_text_unwraps_only_json() {
        assert_eq!(error_text("stream disconnected"), "stream disconnected");
        let err = interpret(false, "exit status: 1", "", "Reading additional input from stdin...\nauth expired").unwrap_err();
        assert_eq!(err, "codex exited with an error (exit status: 1): auth expired");
    }

    #[test]
    fn a_plan_limit_message_gets_the_marker() {
        let err = interpret(false, "exit status: 1", "", "You've hit your usage limit").unwrap_err();
        assert!(err.starts_with(QUOTA_MARKER), "got {err}");
    }

    #[test]
    fn progress_on_stderr_is_not_mistaken_for_an_error_on_a_clean_exit() {
        assert_eq!(interpret(true, "exit status: 0", "done", "[2026] running…").unwrap(), "done");
    }
}
