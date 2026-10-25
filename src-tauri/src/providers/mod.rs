//! The AI CLIs a meeting can be handed to: send a system prompt, an instruction and a transcript,
//! get the answer text back.
//!
//! Ported from CodeFlow's engine layer and cut down to the one shape this app needs — a
//! **text-only, read-only** call. Everything about sessions, tools, MCP servers, skills, effort,
//! attachments, streaming and usage accounting is gone; what stays is each CLI's own text-only flag
//! set and output parsing, verified there against the real binaries, plus the plumbing that makes a
//! headless run safe: a widened `PATH`, a process group per call, stdin from its own task, both
//! pipes drained concurrently, cancel and timeout, and private prompt files deleted afterwards.
//!
//! Built-in engines are code (`claude`, `codex`, `gemini` → `agy`, `opencode`, `cline`, `grok`);
//! their settings live in `app_settings` as `ai_provider`, `{id}_binary_path` and `{id}_model`.
//! Custom providers are rows (`db::provider_queries`) whose ids start with `cli-`.
//!
//! **Multi-line text never travels as one argv on a Windows `.cmd` shim**: `cmd.exe` rejects it.
//! Each engine delivers the way its original did — a short pointer argument with the brief on stdin
//! (Claude, Codex), a private file the CLI or its model reads (agy, opencode, Cline on Windows,
//! grok), or inline only where the program is known not to be a shim.

mod claude;
mod cline;
mod codex;
mod custom;
mod files;
mod gemini;
mod grok;
mod opencode;
pub mod runs;
mod spawn;
mod text;

use std::path::PathBuf;
use std::time::Duration;

use serde::Serialize;
use tokio_util::sync::CancellationToken;

use crate::db::provider_queries::{self, CliProviderRow};
use crate::db::queries::get_setting;

pub use spawn::find_binary;
pub use text::{strip_wrapper_fence, QUOTA_MARKER};

/// The `ai_provider` value that means "the local model", which is not a CLI — the caller handles it.
pub const LOCAL: &str = "local";

/// The provider used when nothing is configured.
pub const DEFAULT_PROVIDER: &str = "claude";

/// The error a cancelled call ends with, word for word, so a caller can tell a stop from a failure.
pub const CANCELLED: &str = "cancelled";

/// The settings keys.
pub mod key {
    pub const PROVIDER: &str = "ai_provider";
    pub fn binary_path(id: &str) -> String {
        format!("{id}_binary_path")
    }
    pub fn model(id: &str) -> String {
        format!("{id}_model")
    }
}

/// How long one call may run before it is stopped. Generous: an hour-long meeting summarised by a
/// thinking model takes minutes, and a hung CLI must still not hold a run forever.
const CALL_TIMEOUT: Duration = Duration::from_secs(15 * 60);

/// The whole system prompt of a run whose caller brings none of its own. Every engine that can
/// replace its CLI's agent prompt uses this one (Claude, Codex, opencode, grok's override).
pub(crate) const TEXT_ONLY_SYSTEM_PROMPT: &str = "Eres un asistente integrado en AIMeetings. Sigue al pie de la letra las \
    instrucciones del mensaje y responde solo con lo que piden.";

/// One text-only call: how to write (`system`), what to do (`ask`), and what to do it with
/// (`data`, the transcript). `model` empty means "let the CLI pick its own default".
#[derive(Debug, Clone, Copy)]
pub struct TextCall<'a> {
    pub system: &'a str,
    pub ask: &'a str,
    pub data: &'a str,
    pub model: &'a str,
}

impl<'a> TextCall<'a> {
    /// The caller's system prompt, or [`TEXT_ONLY_SYSTEM_PROMPT`] when it brought none — for the
    /// engines whose CLI prompt is *replaced* by it.
    fn system_or_default(&self) -> &'a str {
        if self.system.trim().is_empty() { TEXT_ONLY_SYSTEM_PROMPT } else { self.system }
    }

    /// The caller's system prompt when it brought one.
    fn system(&self) -> Option<&'a str> {
        (!self.system.trim().is_empty()).then_some(self.system)
    }

    /// System → ask → data as one brief, for the CLIs that take everything in one place. Order:
    /// system first, so the ask reads as the primary instruction and the data as its input.
    fn brief(&self, with_system: bool) -> String {
        let mut brief = String::new();
        if let Some(system) = self.system().filter(|_| with_system) {
            brief.push_str(system);
            brief.push_str("\n\n");
        }
        brief.push_str(self.ask);
        if !self.data.trim().is_empty() {
            brief.push_str("\n\n----- INPUT -----\n\n");
            brief.push_str(self.data);
        }
        brief
    }
}

/// A resolved provider ready to run.
#[derive(Debug, Clone)]
pub enum Provider {
    Builtin {
        id: String,
        /// The user's override, or the engine's default.
        binary: String,
    },
    Custom(CliProviderRow),
}

impl Provider {
    pub fn id(&self) -> &str {
        match self {
            Provider::Builtin { id, .. } => id,
            Provider::Custom(row) => &row.id,
        }
    }

    pub fn binary(&self) -> &str {
        match self {
            Provider::Builtin { binary, .. } => binary.trim(),
            Provider::Custom(row) => row.binary.trim(),
        }
    }

    /// What a person reads: the engine's label, or the name the user gave the custom one.
    pub fn label(&self) -> &str {
        match self {
            Provider::Builtin { id, .. } => builtin(id).map(|b| b.label).unwrap_or(id),
            Provider::Custom(row) => &row.name,
        }
    }
}

/// A built-in engine, as Settings shows it.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BuiltinInfo {
    pub id: &'static str,
    pub label: &'static str,
    pub default_binary: &'static str,
    /// Official install / setup page.
    pub setup_url: &'static str,
    /// Canonical one-line install command, or empty when the provider has none.
    pub install_command: &'static str,
}

const BUILTINS: [BuiltinInfo; 6] = [
    BuiltinInfo {
        id: "claude",
        label: "Claude Code",
        default_binary: "claude",
        setup_url: "https://docs.claude.com/en/docs/claude-code/setup",
        install_command: "npm install -g @anthropic-ai/claude-code",
    },
    BuiltinInfo {
        id: "codex",
        label: "Codex",
        default_binary: "codex",
        setup_url: "https://developers.openai.com/codex/",
        #[cfg(target_os = "windows")]
        install_command: "winget install OpenAI.Codex",
        #[cfg(not(target_os = "windows"))]
        install_command: "npm install -g @openai/codex",
    },
    // Google's Antigravity CLI, the successor to the retired `gemini` CLI — surfaced as "Gemini".
    BuiltinInfo { id: "gemini", label: "Gemini", default_binary: "agy", setup_url: "https://antigravity.google", install_command: "" },
    BuiltinInfo {
        id: "opencode",
        label: "OpenCode",
        default_binary: "opencode",
        setup_url: "https://opencode.ai/docs/",
        install_command: "npm install -g opencode-ai",
    },
    BuiltinInfo { id: "cline", label: "Cline", default_binary: "cline", setup_url: "https://cline.bot", install_command: "npm install -g cline" },
    BuiltinInfo {
        id: "grok",
        label: "Grok",
        default_binary: "grok",
        setup_url: "https://x.ai/cli",
        install_command: "curl -fsSL https://x.ai/cli/install.sh | bash",
    },
];

/// The built-in engines, in the order Settings lists them.
pub fn builtins() -> &'static [BuiltinInfo] {
    &BUILTINS
}

pub fn builtin(id: &str) -> Option<&'static BuiltinInfo> {
    BUILTINS.iter().find(|b| b.id == id)
}

/// Whether `id` names a custom provider row rather than a built-in engine.
pub fn is_custom_id(id: &str) -> bool {
    id.starts_with("cli-")
}

fn setting(conn: &rusqlite::Connection, key: &str) -> Option<String> {
    get_setting(conn, key).ok().flatten().map(|v| v.trim().to_string()).filter(|v| !v.is_empty())
}

/// Resolves the provider configured for analysis — or `id_override` — into something runnable,
/// with the model to pass it.
///
/// `ai_provider` (default `claude`) picks it; a built-in's `{id}_binary_path` and `{id}_model`
/// configure it; a `cli-…` id is a custom row. An unknown id read from settings falls back to the
/// default, so a corrupt setting can never leave the app without an engine; an unknown id asked for
/// explicitly is an error. `local` is always an error: the caller handles the local model.
pub fn resolve(conn: &rusqlite::Connection, id_override: Option<&str>) -> Result<(Provider, String), String> {
    let explicit = id_override.map(str::trim).filter(|id| !id.is_empty());
    let id = match explicit {
        Some(id) => id.to_string(),
        None => setting(conn, key::PROVIDER).unwrap_or_else(|| DEFAULT_PROVIDER.to_string()),
    };
    if id == LOCAL {
        return Err(format!("'{LOCAL}' is the local model, not a CLI provider"));
    }
    if is_custom_id(&id) {
        let row = provider_queries::get(conn, &id).map_err(|e| e.to_string())?;
        return match row {
            Some(row) => {
                let model = row.model.trim().to_string();
                Ok((Provider::Custom(row), model))
            }
            None => Err(format!("the custom AI provider '{id}' no longer exists — pick another under Settings › AI providers")),
        };
    }
    let info = match builtin(&id) {
        Some(info) => info,
        None if explicit.is_none() => builtin(DEFAULT_PROVIDER).expect("the default is built in"),
        None => return Err(format!("unknown AI provider '{id}'")),
    };
    let binary = setting(conn, &key::binary_path(info.id)).unwrap_or_else(|| info.default_binary.to_string());
    let model = setting(conn, &key::model(info.id)).unwrap_or_default();
    Ok((Provider::Builtin { id: info.id.to_string(), binary }, model))
}

/// One prepared process: what each engine turns a [`TextCall`] into. Plain data, so each engine's
/// flags are a pure function its own tests can assert on.
#[derive(Debug, Default)]
pub(crate) struct Invocation {
    pub args: Vec<String>,
    pub stdin: String,
    pub cwd: Option<PathBuf>,
    pub env: Vec<(String, String)>,
}

/// Which engine a provider runs as.
enum Engine<'a> {
    Claude,
    Codex,
    Gemini,
    OpenCode,
    Cline,
    Grok,
    Custom(&'a CliProviderRow),
}

impl<'a> Engine<'a> {
    fn of(provider: &'a Provider) -> Result<Self, String> {
        Ok(match provider {
            Provider::Custom(row) => Engine::Custom(row),
            Provider::Builtin { id, .. } => match id.as_str() {
                "claude" => Engine::Claude,
                "codex" => Engine::Codex,
                "gemini" => Engine::Gemini,
                "opencode" => Engine::OpenCode,
                "cline" => Engine::Cline,
                "grok" => Engine::Grok,
                other => return Err(format!("unknown AI provider '{other}'")),
            },
        })
    }

    fn invocation(&self, program: &str, call: &TextCall<'_>, files: &files::CallFiles) -> Result<Invocation, String> {
        Ok(match self {
            Engine::Claude => claude::invocation(call, files),
            Engine::Codex => codex::invocation(call, files),
            Engine::Gemini => gemini::invocation(program, call, files),
            Engine::OpenCode => opencode::invocation(call, files),
            Engine::Cline => cline::invocation(call, files),
            Engine::Grok => grok::invocation(call, files),
            Engine::Custom(row) => custom::invocation(row, program, call, files)?,
        })
    }

    fn interpret(&self, done: &spawn::Finished) -> Result<String, String> {
        let (ok, status, out, err) = (done.success, done.status.as_str(), done.stdout.as_str(), done.stderr.as_str());
        match self {
            Engine::Claude => claude::interpret(ok, status, out, err),
            Engine::Codex => codex::interpret(ok, status, out, err),
            Engine::Gemini => gemini::interpret(ok, status, out, err),
            Engine::OpenCode => opencode::interpret(ok, status, out, err),
            Engine::Cline => cline::interpret(ok, status, out, err),
            Engine::Grok => grok::interpret(ok, status, out, err),
            Engine::Custom(_) => custom::interpret(ok, status, out, err),
        }
    }

    /// Whether a failed call is worth one more attempt, unchanged — only for a failure the engine
    /// can name as the model's rather than the request's. Cline's empty answer is the one today.
    fn retry_once_on(&self, error: &str) -> bool {
        matches!(self, Engine::Cline) && cline::retry_once_on(error)
    }
}

/// Runs one text-only call and returns the answer, unwrapped from a whole-reply Markdown fence.
///
/// Cancelling `cancel` stops the process tree and ends the call with `Err(CANCELLED)`; a call that
/// outlives the overall timeout is stopped and reported as such.
pub async fn run_text(provider: &Provider, call: TextCall<'_>, cancel: CancellationToken) -> Result<String, String> {
    let engine = Engine::of(provider)?;
    let binary = provider.binary();
    if binary.is_empty() {
        return Err(format!("no program is configured for {} — set it under Settings › AI providers", provider.label()));
    }
    let outcome = attempt(&engine, binary, &call, &cancel).await;
    match outcome {
        Err(error) if error != CANCELLED && engine.retry_once_on(&error) => attempt(&engine, binary, &call, &cancel).await,
        other => other,
    }
}

/// One spawn of the engine's CLI, from building the command to reading what it said.
async fn attempt(engine: &Engine<'_>, binary: &str, call: &TextCall<'_>, cancel: &CancellationToken) -> Result<String, String> {
    let dirs = spawn::search_dirs();
    let program = spawn::resolve_binary(binary, &dirs);
    // Whatever the engine writes to hand over its prompt is deleted when this attempt is over, by
    // every route out of it — held until then because the CLI reads it after it starts.
    let files = files::CallFiles::new();
    let inv = engine.invocation(&program, call, &files)?;
    let done = spawn::run(binary, &program, &dirs, inv, cancel, CALL_TIMEOUT).await?;
    let answer = engine.interpret(&done)?;
    drop(files);
    // A model's chain of thought is never the answer, whichever engine printed it.
    Ok(strip_wrapper_fence(&text::strip_reasoning_blocks(&answer)))
}

/// Removes the prompt directories a crashed session left behind. Call once at startup.
pub fn sweep_stale_prompts() {
    files::sweep_stale();
}

#[cfg(test)]
pub(crate) mod test_util {
    /// The value right after `flag` in `args`, if any.
    pub fn after<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
        args.iter().position(|a| a == flag).and_then(|at| args.get(at + 1)).map(String::as_str)
    }

    pub fn has_pair(args: &[String], flag: &str, value: &str) -> bool {
        args.windows(2).any(|pair| pair[0] == flag && pair[1] == value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn conn() -> rusqlite::Connection {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::db::migrations::run(&conn).unwrap();
        conn
    }

    #[test]
    fn nothing_configured_resolves_to_claude_with_its_default_binary() {
        let conn = conn();
        let (provider, model) = resolve(&conn, None).unwrap();
        assert!(matches!(&provider, Provider::Builtin { id, binary } if id == "claude" && binary == "claude"));
        assert_eq!(model, "");
    }

    #[test]
    fn a_builtins_own_settings_are_read() {
        let conn = conn();
        crate::db::queries::set_setting(&conn, key::PROVIDER, "gemini").unwrap();
        crate::db::queries::set_setting(&conn, "gemini_binary_path", " /opt/agy ").unwrap();
        crate::db::queries::set_setting(&conn, "gemini_model", "gemini-3.6-flash").unwrap();
        let (provider, model) = resolve(&conn, None).unwrap();
        assert!(matches!(&provider, Provider::Builtin { id, binary } if id == "gemini" && binary == "/opt/agy"));
        assert_eq!(model, "gemini-3.6-flash");
        // An override wins over the setting.
        let (provider, _) = resolve(&conn, Some("codex")).unwrap();
        assert_eq!(provider.id(), "codex");
        assert_eq!(provider.binary(), "codex");
    }

    #[test]
    fn local_and_unknown_ids_are_errors_and_a_corrupt_setting_falls_back() {
        let conn = conn();
        assert!(resolve(&conn, Some(LOCAL)).is_err());
        assert!(resolve(&conn, Some("nope")).is_err());
        crate::db::queries::set_setting(&conn, key::PROVIDER, "nope").unwrap();
        assert_eq!(resolve(&conn, None).unwrap().0.id(), "claude");
        crate::db::queries::set_setting(&conn, key::PROVIDER, LOCAL).unwrap();
        assert!(resolve(&conn, None).is_err());
    }

    #[test]
    fn a_custom_provider_comes_from_its_row() {
        let conn = conn();
        let row = provider_queries::save(
            &conn,
            &provider_queries::CliProviderInput {
                id: None,
                name: "Ollama".into(),
                binary: "ollama".into(),
                args_template: "run {model}".into(),
                input_mode: "stdin".into(),
                model: "llama3".into(),
            },
        )
        .unwrap();
        let (provider, model) = resolve(&conn, Some(&row.id)).unwrap();
        assert!(matches!(&provider, Provider::Custom(r) if r.id == row.id));
        assert_eq!(model, "llama3");
        assert_eq!(provider.label(), "Ollama");
        provider_queries::delete(&conn, &row.id).unwrap();
        assert!(resolve(&conn, Some(&row.id)).is_err());
    }

    #[test]
    fn every_builtin_has_an_engine() {
        for info in builtins() {
            let provider = Provider::Builtin { id: info.id.to_string(), binary: info.default_binary.to_string() };
            assert!(Engine::of(&provider).is_ok(), "{}", info.id);
        }
        assert_eq!(builtin("gemini").unwrap().default_binary, "agy");
    }

    #[test]
    fn the_brief_puts_the_system_prompt_first_and_the_data_last() {
        let call = TextCall { system: "Eres un redactor.", ask: "Resume.", data: "A: hola", model: "" };
        let brief = call.brief(true);
        assert_eq!(brief, "Eres un redactor.\n\nResume.\n\n----- INPUT -----\n\nA: hola");
        assert_eq!(call.brief(false), "Resume.\n\n----- INPUT -----\n\nA: hola");
        let bare = TextCall { system: " ", ask: "Di OK.", data: "", model: "" };
        assert_eq!(bare.brief(true), "Di OK.");
        assert_eq!(bare.system_or_default(), TEXT_ONLY_SYSTEM_PROMPT);
    }

    #[test]
    fn the_text_only_prompt_is_one_line() {
        assert!(!TEXT_ONLY_SYSTEM_PROMPT.contains('\n'), "it rides as an argument through grok's override");
    }

    /// The whole pipeline against a stand-in CLI: a custom provider whose "model" is `sh`, reading
    /// the brief on stdin and answering in a fence with a reasoning block in front.
    #[cfg(unix)]
    #[tokio::test]
    async fn run_text_goes_end_to_end_through_a_custom_provider() {
        let row = CliProviderRow {
            id: "cli-test".into(),
            name: "sh".into(),
            binary: "sh".into(),
            args_template: "-c 'cat >/dev/null; printf \"<think>hmm</think>\\n\\140\\140\\140markdown\\n# Acta\\n\\140\\140\\140\\n\"'".into(),
            input_mode: "stdin".into(),
            model: String::new(),
            sort_order: 0,
        };
        let call = TextCall { system: "s", ask: "a", data: "d", model: "" };
        let answer = run_text(&Provider::Custom(row), call, CancellationToken::new()).await.unwrap();
        assert_eq!(answer, "# Acta");
    }

    #[tokio::test]
    async fn an_empty_binary_is_refused_before_anything_runs() {
        let provider = Provider::Builtin { id: "claude".into(), binary: "  ".into() };
        let call = TextCall { system: "", ask: "a", data: "", model: "" };
        let error = run_text(&provider, call, CancellationToken::new()).await.unwrap_err();
        assert!(error.contains("Claude Code"), "{error}");
    }

    #[tokio::test]
    async fn a_call_cancelled_before_it_starts_runs_nothing() {
        let provider = Provider::Builtin { id: "claude".into(), binary: "claude".into() };
        let cancel = CancellationToken::new();
        cancel.cancel();
        let call = TextCall { system: "", ask: "a", data: "", model: "" };
        assert_eq!(run_text(&provider, call, cancel).await.unwrap_err(), CANCELLED);
    }
}
