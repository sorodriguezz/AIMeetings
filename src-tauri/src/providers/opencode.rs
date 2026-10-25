//! The opencode CLI (`opencode run`), text-only.
//!
//! [opencode](https://opencode.ai) is provider-agnostic: the user configures whatever providers
//! they want *inside opencode* and a model is addressed as `provider/model`. Three things shape the
//! command, verified in CodeFlow against opencode 1.15.7–1.18.35:
//!
//!   1. **No stdin.** `opencode run` does not read piped stdin.
//!   2. **No `--system-prompt` flag.**
//!   3. **Windows `.cmd` shim can't take newline args.**
//!
//! So the brief (ask + data) is written to a private file attached with `--file`, and the argument
//! is a short single-line pointer — which must come before `--file`, a variadic flag that would
//! otherwise swallow it.
//!
//! **What a text-only run is.** An agent of the app's own, [`TEXT_AGENT`], passed as inline config
//! (`OPENCODE_CONFIG_CONTENT`, which opencode merges over the user's own files): its `prompt`
//! *replaces* opencode's provider prompt — so the system prompt travels there, not in the brief —
//! and its permissions deny every tool, which drops them from what the model is offered. The user's
//! own MCP servers are switched off by name, external skill scans are disabled, and the run works in
//! an empty folder of its own (`--dir`). Never `--dangerously-skip-permissions`.
//!
//! **Output.** `--format json` prints one event per line, `{type, timestamp, sessionID, …}`; the
//! reply is the concatenated `text` events' `part.text`, and an `error` event — which can arrive on
//! a zero exit — is the CLI explaining itself.

use serde::Deserialize;

use super::files::CallFiles;
use super::text::{first_nonempty, quota_signal, refusal_reply, QUOTA_MARKER};
use super::{Invocation, TextCall};

/// The app's own opencode agent — see the module docs.
const TEXT_AGENT: &str = "aimeetings-text";

/// Short, single-line, ASCII pointer message — safe through the `.cmd`/cmd.exe layer.
const POINTER: &str = "The attached file contains your full instructions and the input to work on. \
     Follow them exactly and reply with only the requested output.";

pub(super) fn invocation(call: &TextCall<'_>, files: &CallFiles) -> Invocation {
    let user_config = std::env::var("OPENCODE_CONFIG_CONTENT").ok();
    invocation_with(call, files, user_config.as_deref(), &users_mcp_servers())
}

fn invocation_with(call: &TextCall<'_>, files: &CallFiles, user_config: Option<&str>, user_servers: &[String]) -> Invocation {
    // The system prompt is the agent's prompt (see `run_config`), so the brief is ask + data. If the
    // config cannot carry it — the user's own `OPENCODE_CONFIG_CONTENT` is not a JSON object — it
    // leads the brief instead.
    let config = run_config(call, user_config, user_servers);
    let brief = call.brief(config.is_none());

    let mut args: Vec<String> = vec!["run".into(), POINTER.into(), "--format".into(), "json".into()];
    if !call.model.trim().is_empty() {
        args.extend(["--model".into(), call.model.trim().into()]);
    }
    // Nothing to read, so nothing to be in: an empty folder of its own.
    if let Some(dir) = files.dir("opencode-scratch") {
        args.extend(["--dir".into(), dir.to_string_lossy().into_owned()]);
    }
    let mut env: Vec<(String, String)> = vec![
        // Skill scans of `~/.claude` and `~/.agents`: a listing a run with no tools cannot use.
        ("OPENCODE_DISABLE_EXTERNAL_SKILLS".into(), "1".into()),
        ("OPENCODE_DISABLE_CLAUDE_CODE_SKILLS".into(), "1".into()),
    ];
    if let Some(config) = config {
        args.extend(["--agent".into(), TEXT_AGENT.into()]);
        env.push(("OPENCODE_CONFIG_CONTENT".into(), config));
    }
    // `--file` last so the message positional above can't be mistaken for another attachment.
    if let Some(path) = files.write("opencode", "txt", &brief) {
        args.extend(["--file".into(), path.to_string_lossy().into_owned()]);
    }
    // opencode reads no stdin; piped anyway, as the original did, and closed at EOF.
    Invocation { args, stdin: call.data.to_string(), cwd: None, env }
}

/// The `OPENCODE_CONFIG_CONTENT` of a text-only run: the user's own value (if any) with the app's
/// agent added and the user's MCP servers switched off by name (`{"enabled": false}`, a shape the
/// config schema takes for any entry). `None` when the user's value is not a JSON object — it is
/// left alone, and the run goes without the agent rather than without the user's config.
fn run_config(call: &TextCall<'_>, user_config: Option<&str>, user_servers: &[String]) -> Option<String> {
    let mut config = match user_config {
        Some(text) if !text.trim().is_empty() => match serde_json::from_str::<serde_json::Value>(text) {
            Ok(value) if value.is_object() => value,
            _ => return None,
        },
        _ => serde_json::json!({}),
    };
    if !config["mcp"].is_object() {
        config["mcp"] = serde_json::json!({});
    }
    for name in user_servers {
        config["mcp"][name.as_str()] = serde_json::json!({ "enabled": false });
    }
    if !config["agent"].is_object() {
        config["agent"] = serde_json::json!({});
    }
    config["agent"][TEXT_AGENT] = serde_json::json!({
        "mode": "primary",
        "description": "AIMeetings: answers from what it is handed, with no tools",
        "prompt": call.system_or_default(),
        "permission": { "*": "deny" },
    });
    Some(config.to_string())
}

/// The MCP servers the user's global opencode config declares — the files under
/// `$XDG_CONFIG_HOME/opencode` (`~/.config/opencode`) — by name. A file that does not parse names
/// none: switching a server off is a saving, and a run is never refused over a comment.
fn users_mcp_servers() -> Vec<String> {
    let config_home = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|d| !d.is_empty())
        .map(std::path::PathBuf::from)
        .or_else(|| dirs::home_dir().map(|h| h.join(".config")));
    let Some(home) = config_home else { return Vec::new() };
    let mut names: Vec<String> = Vec::new();
    for name in ["config.json", "opencode.json", "opencode.jsonc"] {
        let Ok(text) = std::fs::read_to_string(home.join("opencode").join(name)) else { continue };
        for server in mcp_names(&text) {
            if !names.contains(&server) {
                names.push(server);
            }
        }
    }
    names
}

/// The keys of a config's `mcp` object — JSON, or JSONC with `//` and `/* */` comments.
fn mcp_names(text: &str) -> Vec<String> {
    let parsed = serde_json::from_str::<serde_json::Value>(text)
        .or_else(|_| serde_json::from_str::<serde_json::Value>(&strip_json_comments(text)));
    match parsed {
        Ok(value) => value.get("mcp").and_then(|m| m.as_object()).map(|m| m.keys().cloned().collect()).unwrap_or_default(),
        Err(_) => Vec::new(),
    }
}

/// `text` without its comments, strings left as they are.
fn strip_json_comments(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    let mut in_string = false;
    while let Some(ch) = chars.next() {
        if in_string {
            out.push(ch);
            if ch == '\\' {
                if let Some(next) = chars.next() {
                    out.push(next);
                }
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }
        match (ch, chars.peek()) {
            ('"', _) => {
                in_string = true;
                out.push(ch);
            }
            ('/', Some('/')) => {
                for c in chars.by_ref() {
                    if c == '\n' {
                        out.push('\n');
                        break;
                    }
                }
            }
            ('/', Some('*')) => {
                chars.next();
                let mut last = ' ';
                for c in chars.by_ref() {
                    if last == '*' && c == '/' {
                        break;
                    }
                    last = c;
                }
            }
            _ => out.push(ch),
        }
    }
    out
}

/// One line of `opencode run --format json`.
#[derive(Deserialize)]
struct Event {
    #[serde(rename = "type")]
    kind: String,
    part: Option<Part>,
    error: Option<EventError>,
}

#[derive(Deserialize)]
struct Part {
    #[serde(default)]
    text: Option<String>,
}

/// The `error` event's payload: a named error whose human-readable text sits in `data.message`.
#[derive(Deserialize)]
struct EventError {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    data: Option<EventErrorData>,
}

#[derive(Deserialize)]
struct EventErrorData {
    #[serde(default)]
    message: Option<String>,
}

/// What one JSON-formatted run reported.
struct Parsed {
    text: String,
    /// First error event, if any — the root cause, which later events tend to restate.
    error: Option<String>,
}

/// Reads the event stream. `None` means stdout held no parseable event at all, so the caller falls
/// back to treating it as plain text.
fn parse_events(stdout: &str) -> Option<Parsed> {
    let mut texts: Vec<String> = Vec::new();
    let mut error: Option<String> = None;
    let mut saw_event = false;
    for line in stdout.lines() {
        let line = line.trim();
        if !line.starts_with('{') {
            continue;
        }
        let Ok(event) = serde_json::from_str::<Event>(line) else { continue };
        saw_event = true;
        match event.kind.as_str() {
            // Concatenating the completed text parts reproduces what the default formatter writes
            // to a non-TTY stdout (each part trimmed, one per line).
            "text" => {
                if let Some(text) = event.part.and_then(|p| p.text) {
                    let text = text.trim();
                    if !text.is_empty() {
                        texts.push(text.to_string());
                    }
                }
            }
            "error" if error.is_none() => error = event.error.and_then(error_message),
            _ => {}
        }
    }
    saw_event.then(|| Parsed { text: texts.join("\n"), error })
}

/// Best available description of an `error` event: its message, qualified by the error name when
/// there is one (`APIError: Unauthorized …`).
fn error_message(error: EventError) -> Option<String> {
    let message = error.data.and_then(|d| d.message).map(|m| m.trim().to_string()).filter(|m| !m.is_empty());
    match (error.name.as_deref().map(str::trim).filter(|n| !n.is_empty()), message) {
        (Some(name), Some(message)) => Some(format!("{name}: {message}")),
        (None, Some(message)) => Some(message),
        (Some(name), None) => Some(name.to_string()),
        (None, None) => None,
    }
}

/// Turns one finished `opencode run --format json` into its reply or a user-facing error.
pub(super) fn interpret(success: bool, status_label: &str, stdout: &str, stderr: &str) -> Result<String, String> {
    if let Some(parsed) = parse_events(stdout) {
        // An `error` event can arrive on an otherwise zero-exit run, so it's judged first.
        if let Some(error) = parsed.error {
            return Err(if quota_signal(&error) { format!("{QUOTA_MARKER}{error}") } else { error });
        }
        let text = parsed.text.trim();
        if !success {
            let detail = first_nonempty(&[text, stderr], "no output");
            if quota_signal(detail) {
                return Err(format!("{QUOTA_MARKER}{detail}"));
            }
            return Err(format!("opencode exited with an error ({status_label}): {detail}"));
        }
        if text.is_empty() {
            return Err("opencode produced no output".to_string());
        }
        // The run reports no counts, so the words are all there is to judge by.
        if refusal_reply(text, None) {
            return Err(format!("{QUOTA_MARKER}{text}"));
        }
        return Ok(text.to_string());
    }

    if !success {
        if quota_signal(stderr) {
            return Err(format!("{QUOTA_MARKER}{}", stderr.trim()));
        }
        if quota_signal(stdout) {
            return Err(format!("{QUOTA_MARKER}{}", stdout.trim()));
        }
        let detail = first_nonempty(&[stderr, stdout], "no output on stdout or stderr");
        return Err(format!("opencode exited with an error ({status_label}): {detail}"));
    }
    let text = stdout.trim();
    if text.is_empty() {
        // Some builds print status on stderr; surface that rather than an empty reply.
        let err = stderr.trim();
        return Err(if err.is_empty() { "opencode produced no output".to_string() } else { err.to_string() });
    }
    if refusal_reply(text, None) {
        return Err(format!("{QUOTA_MARKER}{text}"));
    }
    Ok(text.to_string())
}

#[cfg(test)]
mod tests {
    use super::super::test_util::{after, has_pair};
    use super::*;

    fn config_of(inv: &Invocation) -> serde_json::Value {
        let text = inv.env.iter().find(|(k, _)| k == "OPENCODE_CONFIG_CONTENT").map(|(_, v)| v.clone()).expect("an inline config");
        serde_json::from_str(&text).unwrap()
    }

    #[test]
    fn a_text_only_run_is_an_agent_of_its_own_with_no_tools_and_its_own_prompt() {
        let files = CallFiles::new();
        let call = TextCall { system: "Eres quien resume.", ask: "Resume", data: "la reunión", model: "anthropic/claude-sonnet-4-5" };
        let inv = invocation_with(&call, &files, None, &["github".to_string()]);
        let args = &inv.args;
        assert_eq!(&args[..2], &["run".to_string(), POINTER.to_string()]);
        assert!(has_pair(args, "--agent", TEXT_AGENT), "{args:?}");
        assert!(has_pair(args, "--format", "json"));
        assert!(has_pair(args, "--model", "anthropic/claude-sonnet-4-5"));
        assert!(!args.iter().any(|a| a == "--dangerously-skip-permissions"));
        let config = config_of(&inv);
        assert_eq!(config["agent"][TEXT_AGENT]["prompt"], "Eres quien resume.");
        assert_eq!(config["agent"][TEXT_AGENT]["permission"]["*"], "deny");
        assert_eq!(config["mcp"]["github"]["enabled"], false);
        assert!(inv.env.iter().any(|(k, v)| k == "OPENCODE_DISABLE_EXTERNAL_SKILLS" && v == "1"));
        // Said once, as the agent's prompt — not again in the brief.
        let brief = std::fs::read_to_string(after(args, "--file").unwrap()).unwrap();
        assert!(!brief.contains("Eres quien resume.") && brief.contains("Resume") && brief.contains("la reunión"), "{brief}");
        assert_eq!(args.last().map(String::as_str), after(args, "--file"), "--file goes last");
        // In an empty folder of its own.
        let dir = after(args, "--dir").unwrap();
        assert_eq!(std::fs::read_dir(dir).unwrap().count(), 0);
        assert!(args.iter().all(|a| !a.contains('\n')));
    }

    /// The user's own inline config is kept and added to; one that is not an object is left alone,
    /// and the system prompt then rides the brief.
    #[test]
    fn the_users_inline_config_is_merged_or_left_alone() {
        let files = CallFiles::new();
        let call = TextCall { system: "S", ask: "A", data: "", model: "" };
        let merged = invocation_with(&call, &files, Some(r#"{"theme":"x"}"#), &[]);
        assert_eq!(config_of(&merged)["theme"], "x");
        let left = invocation_with(&call, &files, Some("// not json"), &[]);
        assert!(!left.env.iter().any(|(k, _)| k == "OPENCODE_CONFIG_CONTENT"));
        assert!(!left.args.iter().any(|a| a == "--agent"));
        let brief = std::fs::read_to_string(after(&left.args, "--file").unwrap()).unwrap();
        assert!(brief.starts_with("S\n\nA"), "{brief}");
    }

    #[test]
    fn the_users_servers_are_read_from_json_with_comments() {
        let text = "{\n  // mine\n  \"mcp\": { \"github\": {\"type\": \"local\"}, /* x */ \"vercel\": {\"url\": \"https://a//b\"} }\n}";
        assert_eq!(mcp_names(text), vec!["github".to_string(), "vercel".to_string()]);
        assert!(mcp_names("not json").is_empty());
    }

    const JSON_STDOUT: &str = concat!(
        r#"{"type":"step_start","timestamp":1785160196000,"sessionID":"ses_05c2930e3ffeAwuzpCYQ7CgtfG","part":{"type":"step-start"}}"#,
        "\n",
        r#"{"type":"text","timestamp":1785160196045,"sessionID":"ses_05c2930e3ffeAwuzpCYQ7CgtfG","part":{"id":"prt_1","messageID":"msg_1","type":"text","text":"  feat: add thing  ","time":{"start":1,"end":2}}}"#,
        "\n",
        r#"{"type":"step_finish","timestamp":1785160196050,"sessionID":"ses_05c2930e3ffeAwuzpCYQ7CgtfG","part":{"type":"step-finish"}}"#,
        "\n",
    );

    #[test]
    fn reads_the_reply_out_of_the_event_stream() {
        assert_eq!(interpret(true, "exit status: 0", JSON_STDOUT, "").unwrap(), "feat: add thing");
    }

    #[test]
    fn joins_multiple_text_parts_in_order() {
        let stdout = concat!(
            r#"{"type":"text","sessionID":"ses_a","part":{"type":"text","text":"first"}}"#,
            "\n",
            r#"{"type":"text","sessionID":"ses_a","part":{"type":"text","text":"second"}}"#,
            "\n",
        );
        assert_eq!(interpret(true, "exit status: 0", stdout, "").unwrap(), "first\nsecond");
    }

    #[test]
    fn an_error_event_beats_a_clean_exit_status() {
        let stdout = r#"{"type":"error","timestamp":1785160196045,"sessionID":"ses_05c2930e3ffeAwuzpCYQ7CgtfG","error":{"name":"APIError","data":{"message":"Unauthorized: unauthorized: AuthenticateToken authentication failed","statusCode":401}}}"#;
        let err = interpret(true, "exit status: 0", stdout, "").unwrap_err();
        assert_eq!(err, "APIError: Unauthorized: unauthorized: AuthenticateToken authentication failed");
    }

    #[test]
    fn quota_failures_get_the_marker() {
        let stdout = r#"{"type":"step_start","sessionID":"ses_a","part":{"type":"step-start"}}"#;
        assert!(interpret(false, "exit status: 1", stdout, "Error: Insufficient balance").unwrap_err().starts_with(QUOTA_MARKER));
        let event = r#"{"type":"error","sessionID":"ses_a","error":{"name":"APIError","data":{"message":"Upstream request failed: Insufficient account funds"}}}"#;
        assert!(interpret(true, "exit status: 0", event, "").unwrap_err().starts_with(QUOTA_MARKER));
        assert!(interpret(false, "exit status: 1", "", "You have hit your rate limit, try again in 1h").unwrap_err().starts_with(QUOTA_MARKER));
    }

    #[test]
    fn falls_back_to_plain_stdout() {
        assert_eq!(interpret(true, "exit status: 0", "  feat: add thing  ", "").unwrap(), "feat: add thing");
    }

    #[test]
    fn surfaces_the_failure_detail() {
        let err = interpret(false, "exit status: 1", "", "auth required: run `opencode auth login`").unwrap_err();
        assert_eq!(err, "opencode exited with an error (exit status: 1): auth required: run `opencode auth login`");
    }

    #[test]
    fn empty_output_on_a_clean_exit_is_an_error_not_a_blank_reply() {
        assert_eq!(interpret(true, "exit status: 0", "   ", "").unwrap_err(), "opencode produced no output");
    }
}
