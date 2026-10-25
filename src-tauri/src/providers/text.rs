//! The pure text work every engine shares: cleaning what a CLI printed, unwrapping the answer, and
//! telling a provider's refusal apart from a reply.

/// Prefix the frontend can look for to render a dedicated "you're out of quota" notice instead of a
/// generic error banner. Every engine's interpreter emits it, so it lives here.
pub const QUOTA_MARKER: &str = "QUOTA_EXCEEDED::";

/// Removes terminal escape sequences from a CLI's output. The engines run headless but still
/// colourize (opencode paints its errors red), and those raw `ESC[91m` bytes would otherwise be
/// rendered literally in the UI. Handles CSI (colour/cursor) and OSC (hyperlink/title) sequences.
pub(crate) fn strip_ansi(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c != '\u{1b}' {
            out.push(c);
            continue;
        }
        match chars.next() {
            // CSI: ESC [ params… final byte in @–~
            Some('[') => {
                for byte in chars.by_ref() {
                    if ('\u{40}'..='\u{7e}').contains(&byte) {
                        break;
                    }
                }
            }
            // OSC: ESC ] … terminated by BEL or ESC \
            Some(']') => {
                while let Some(byte) = chars.next() {
                    if byte == '\u{7}' {
                        break;
                    }
                    if byte == '\u{1b}' {
                        chars.next();
                        break;
                    }
                }
            }
            // Any other two-byte escape: the second char is already consumed.
            _ => {}
        }
    }
    out
}

/// Unwraps a reply that put the *whole* answer in one fence, and leaves every other reply alone.
///
/// Models asked for Markdown often answer with ```markdown … ``` around the lot. Stripping that is
/// right. Stripping the fence off an answer that *is* a shell snippet is not — so this only unwraps
/// when the fence is unlabelled or labelled as Markdown, and only when a single fence encloses
/// everything.
pub fn strip_wrapper_fence(text: &str) -> String {
    let trimmed = text.trim();
    let Some(rest) = trimmed.strip_prefix("```") else {
        return trimmed.to_string();
    };
    let Some(newline) = rest.find('\n') else {
        return trimmed.to_string();
    };
    let label = rest[..newline].trim().to_ascii_lowercase();
    if !matches!(label.as_str(), "" | "markdown" | "md") {
        return trimmed.to_string();
    }
    let body = &rest[newline + 1..];
    let Some(body) = body.trim_end().strip_suffix("```") else {
        return trimmed.to_string();
    };
    // A second opening fence inside means the outer one was not a wrapper: the reply is a document
    // that happens to start with a code block.
    if body.contains("```") && body.matches("```").count() % 2 != 0 {
        return trimmed.to_string();
    }
    body.trim_end().to_string()
}

/// Tags a model can wrap its chain of thought in. `think` covers the DeepSeek-R1 family (and
/// everything Ollama serves that copied it); the rest turn up in Qwen, GLM, and the "reasoning"
/// variants several OpenAI-compatible gateways expose.
const REASONING_TAGS: [&str; 6] = ["think", "thinking", "thought", "reasoning", "reflection", "scratchpad"];

/// Removes `<think>…</think>`-style blocks from a reply.
///
/// A model's chain of thought is never the answer, but plenty of CLIs and gateways print it inline
/// with one. Applied in `run_text`, the one place every engine passes through, so a provider added
/// later gets this without knowing about it.
///
/// A lone *closing* tag counts too: some gateways drop the opening one, leaving
/// `…deliberación…</think>\nrespuesta`. A lone *opening* tag is left alone — there is no answer
/// after it to keep, and emptying the reply would turn a usable one into nothing.
pub(crate) fn strip_reasoning_blocks(text: &str) -> String {
    let mut out = text.to_string();
    for tag in REASONING_TAGS {
        let open = format!("<{tag}>");
        let close = format!("</{tag}>");
        loop {
            // `to_ascii_lowercase` rewrites only A–Z, so every byte index it reports is still a
            // valid index (and char boundary) into `out`.
            let lower = out.to_ascii_lowercase();
            let end = match lower.find(&close) {
                Some(end) => end + close.len(),
                None => break,
            };
            match lower.find(&open) {
                Some(start) if start < end => out.replace_range(start..end, ""),
                _ => out.replace_range(..end, ""),
            }
        }
    }
    let cleaned = out.trim();
    // A reply that was *only* reasoning leaves nothing behind; the raw text is worth more to the
    // user (and to the error paths that read it) than an empty string.
    if cleaned.is_empty() { text.trim().to_string() } else { cleaned.to_string() }
}

/// Phrases that mean "the provider refused because of a limit or your account balance", not that
/// something is broken. The billing ones matter for the credit-based CLIs (opencode bills per
/// token, so it answers with "Insufficient balance" rather than a rate limit).
const QUOTA_SIGNALS: [&str; 11] = [
    "usage limit",
    "rate limit",
    "quota exceeded",
    "resets at",
    // "try again in" is not here: it only counts with a number after it — see [`waits_on_a_clock`].
    "limit reached",
    "insufficient balance",
    "insufficient credit",
    // opencode Zen's gateway, verbatim. The whole phrase and not the looser "insufficient funds",
    // which is ordinary vocabulary — and [`refusal_reply`] reads *successful* replies.
    "insufficient account funds",
    "out of credit",
    "payment required",
    "billing",
];

/// Whether a CLI's message reads like a quota/billing refusal rather than a genuine error.
///
/// **Only ever asked of text the run already failed with** (and, through [`refusal_reply`], of a
/// reply that generated nothing at all). That is what lets it include [`limit_refusal`], whose
/// openings are ordinary English in any other position.
pub(crate) fn quota_signal(text: &str) -> bool {
    worded_as_quota(text) || limit_refusal(text)
}

/// The narrower half of [`quota_signal`]: phrases that name a limit or a balance outright.
fn worded_as_quota(text: &str) -> bool {
    let lower = text.to_lowercase();
    QUOTA_SIGNALS.iter().any(|s| lower.contains(s)) || waits_on_a_clock(&lower)
}

/// How Claude Code opens the sentence that says the plan is used up ("You've hit your session
/// limit · resets 12am", "You're out of extra usage", "Your org is out of usage · …").
const LIMIT_OPENINGS: [&str; 4] = ["you've hit your", "you've reached your", "you're out of", "your org is out of"];

/// What has to follow an opening, in the same sentence, for it to be a refusal. An opening alone is
/// everyday English — "you've hit your stride", "you're out of luck".
const LIMIT_NOUNS: [&str; 3] = ["limit", "usage", "credits"];

/// Whether `text` contains one of Claude Code's own "you are out of plan" sentences.
pub(crate) fn limit_refusal(text: &str) -> bool {
    // The CLI writes a straight apostrophe; a curly one is what the same sentence becomes the moment
    // anything typographic has touched it.
    let lower = text.to_lowercase().replace('\u{2019}', "'");
    LIMIT_OPENINGS.iter().any(|opening| {
        lower.match_indices(opening).any(|(at, _)| {
            let sentence = lower[at + opening.len()..]
                .split(['.', '\u{b7}', '\n', '!', '?', '|'])
                .next()
                .unwrap_or_default();
            LIMIT_NOUNS.iter().any(|noun| sentence.contains(noun))
        })
    })
}

/// "Try again in 3 hours" is a window that reopens on a clock. "Try again in a moment" is a server
/// that fell over — so it counts only when a number follows it.
fn waits_on_a_clock(lower: &str) -> bool {
    const PHRASE: &str = "try again in";
    lower
        .match_indices(PHRASE)
        .any(|(at, _)| lower[at + PHRASE.len()..].trim_start().starts_with(|c: char| c.is_ascii_digit()))
}

/// The longest a message can be and still be *only* a refusal. Nothing a provider says to turn a
/// run down needs a paragraph.
const MAX_REFUSAL_CHARS: usize = 400;

/// Whether a **successful** run's own reply is a refusal rather than an answer.
///
/// Stricter than [`quota_signal`]: on a run that succeeded, the phrase has to **be** the whole
/// message, because the model is also free to write about rate limits and billing — a meeting about
/// a billing system is full of them. And a reply the model generated (`output_tokens > 0`, when the
/// engine reports it) is never a refusal, however short: a provider turns a run down *instead of*
/// generating. A run that reported zero generated tokens is the provider speaking, so the failed
/// path's whole vocabulary applies; one that reported no counts gets only the narrow phrases.
pub(crate) fn refusal_reply(text: &str, output_tokens: Option<i64>) -> bool {
    if text.chars().count() > MAX_REFUSAL_CHARS {
        return false;
    }
    match output_tokens {
        Some(generated) if generated > 0 => false,
        Some(_) => quota_signal(text),
        None => worded_as_quota(text),
    }
}

/// The reason a process died to a SIGKILL without printing a byte, or `None` for every other
/// status. A kill with no output is not a failed answer, it is a process that never started — and
/// the difference is the whole message.
pub(crate) fn killed_without_output(engine: &str, status_label: &str) -> Option<String> {
    let killed = status_label.contains("signal: 9") || status_label.contains("SIGKILL");
    if !killed {
        return None;
    }
    #[cfg(target_os = "macos")]
    let cause = "On macOS that is almost never the model or the question: usually the executable's \
                 signature stopped being valid — an installer patched it after signing — and the \
                 system kills it before it starts. Run it by hand in a terminal: if it exits with \
                 code 137 and no output, that is it, and re-signing it with \
                 `codesign --force --sign -` fixes it.";
    #[cfg(not(target_os = "macos"))]
    let cause = "A process that dies like that without writing anything never really started: \
                 usually the system killed it for lack of memory. Run it by hand in a terminal to \
                 see whether it exits with code 137.";
    Some(format!("{engine} was killed by the system before writing anything ({status_label}). {cause}"))
}

/// The first non-empty of several candidate details, trimmed, or `fallback`.
pub(crate) fn first_nonempty<'a>(candidates: &[&'a str], fallback: &'a str) -> &'a str {
    candidates.iter().map(|s| s.trim()).find(|s| !s.is_empty()).unwrap_or(fallback)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_colour_codes_from_a_cli_error() {
        let raw = "\u{1b}[0m\u{1b}[91m\u{1b}[1mError: \u{1b}[0mInsufficient balance.";
        assert_eq!(strip_ansi(raw), "Error: Insufficient balance.");
    }

    #[test]
    fn strips_osc_hyperlink_sequences() {
        let raw = "see \u{1b}]8;;https://example.com\u{7}the docs\u{1b}]8;;\u{7}";
        assert_eq!(strip_ansi(raw), "see the docs");
    }

    #[test]
    fn leaves_ordinary_text_untouched() {
        assert_eq!(strip_ansi("feat: add thing [skip ci]"), "feat: add thing [skip ci]");
        assert_eq!(strip_ansi("ñandú — «acta»"), "ñandú — «acta»");
    }

    #[test]
    fn a_whole_reply_in_a_markdown_fence_comes_out_of_it() {
        assert_eq!(strip_wrapper_fence("```markdown\n# Acta\n\n- uno\n```"), "# Acta\n\n- uno");
        assert_eq!(strip_wrapper_fence("```\nhola\n```\n"), "hola");
        assert_eq!(strip_wrapper_fence("```MD\nhola\n```"), "hola");
    }

    /// A fence labelled as code is the answer itself, not a wrapper.
    #[test]
    fn a_code_fence_is_left_alone() {
        let shell = "```bash\nls -la\n```";
        assert_eq!(strip_wrapper_fence(shell), shell);
    }

    /// A document that opens with a code block and goes on is not one wrapped reply.
    #[test]
    fn a_document_that_merely_starts_with_a_fence_is_left_alone() {
        let doc = "```\nuno\n```js\nx\n```";
        assert_eq!(strip_wrapper_fence(doc), doc);
        let trailing = "```\nuno\n```\ntexto final";
        assert_eq!(strip_wrapper_fence(trailing), trailing);
        assert_eq!(strip_wrapper_fence("```\nsin cierre"), "```\nsin cierre");
        assert_eq!(strip_wrapper_fence("  sin fence  "), "sin fence");
    }

    #[test]
    fn a_reasoning_block_is_removed_whether_or_not_its_opening_tag_survived() {
        assert_eq!(strip_reasoning_blocks("<think>Hmm.</think>\nLa respuesta"), "La respuesta");
        assert_eq!(strip_reasoning_blocks("Hmm.\n</THINK>\nLa respuesta"), "La respuesta");
    }

    #[test]
    fn an_unclosed_reasoning_tag_leaves_the_reply_alone() {
        assert_eq!(strip_reasoning_blocks("<think>me quedé pensando"), "<think>me quedé pensando");
        assert_eq!(strip_reasoning_blocks("<think>solo pensamiento</think>"), "<think>solo pensamiento</think>");
    }

    #[test]
    fn claude_codes_limit_sentences_are_recognised_and_ordinary_english_is_not() {
        for refusal in [
            "You've hit your session limit · resets 12am (America/Santiago)",
            "You've hit your weekly limit · resets Mon 9am",
            "You\u{2019}ve reached your usage limit",
            "API Error: 400 You're out of extra usage. Add more at claude.ai/settings/usage",
            "You're out of usage credits",
            "Your org is out of usage · contact your admin",
        ] {
            assert!(limit_refusal(refusal), "missed: {refusal}");
            assert!(quota_signal(refusal), "the failed path must see it: {refusal}");
        }
        for prose in [
            "You've hit your stride with this refactor.",
            "You're out of luck: the file is gone.",
            "You've reached your destination. The limit of the loop is 10.",
        ] {
            assert!(!limit_refusal(prose), "misread: {prose}");
        }
    }

    #[test]
    fn the_new_wordings_follow_the_token_evidence_rule() {
        let session = "You've hit your session limit · resets 12am (America/Santiago)";
        assert!(refusal_reply(session, Some(0)), "zero tokens is the provider speaking");
        assert!(!refusal_reply(session, None), "no counts at all: only the narrow phrases decide");
        assert!(!refusal_reply(session, Some(12)), "a generated reply is an answer");
        assert!(!refusal_reply("feat: add rate limiting and idempotency to order creation", Some(547)));
        assert!(refusal_reply("Claude AI usage limit reached|1751234567", Some(0)));
        assert!(refusal_reply("Insufficient balance. Manage your billing here.", None));
    }

    #[test]
    fn billing_refusals_count_and_genuine_failures_do_not() {
        assert!(quota_signal("Error: Insufficient balance. Manage your billing here: https://x/billing"));
        assert!(quota_signal("APIError: Upstream request failed: Insufficient account funds"));
        assert!(!quota_signal("error: unknown flag --nope"));
        assert!(!quota_signal("the transfer fails with insufficient funds and returns 402"));
        assert!(!refusal_reply("`TransferError::InsufficientFunds` is what that branch returns.", None));
    }

    /// A finished summary whose own prose mentions a rate limit is an answer, not a refusal.
    #[test]
    fn an_answer_that_merely_talks_about_limits_is_not_a_refusal() {
        let summary = format!(
            "La búsqueda de citas es una llamada HTTP a SAP que tiene rate limit, y el módulo de \
             billing queda fuera del alcance. {}",
            "Detalle adicional del acta. ".repeat(20),
        );
        assert!(quota_signal(&summary), "the phrases really are in there");
        assert!(!refusal_reply(&summary, None));
    }

    #[test]
    fn only_a_retry_with_a_time_is_a_quota_signal() {
        assert!(!quota_signal(
            "API Error: 529 Overloaded. This is a server-side issue, usually temporary — try again in a \
             moment. If it persists, check https://status.claude.com."
        ));
        assert!(quota_signal("Please try again in 20 minutes."));
        assert!(quota_signal("slow down; try again in  3h"));
    }

    #[test]
    fn a_kill_with_no_output_is_explained_and_nothing_else_is() {
        let killed = killed_without_output("cline", "signal: 9 (SIGKILL)").expect("explained");
        assert!(killed.contains("137"), "{killed}");
        assert!(killed_without_output("cline", "exit status: 1").is_none());
    }
}
