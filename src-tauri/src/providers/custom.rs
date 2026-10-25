//! Any other CLI: the provider the user configures themselves.
//!
//! The built-in engines are written against a *verified* binary — every flag was read off the real
//! CLI. That is also what stops this app from supporting the next tool that appears, so this is the
//! honest version of "any CLI": the user names the binary, the argument line and how the text gets
//! in; the app runs it and reads the answer off stdout. Nothing is guessed, and nothing is claimed.
//!
//! # The template
//!
//! One line, as it would be typed in a shell — `run {model}`, `-p {prompt}`, `--file {file} -q`.
//! `{prompt}`, `{model}` and `{file}` are filled in at run time. An argument that mentions a
//! placeholder with nothing behind it is **dropped whole**, and so is the flag right before it: with
//! no model configured, `--model {model}` disappears rather than being sent as an empty string, or
//! as a bare `--model` that would swallow the next argument as its value. Quotes group words the way
//! a shell does, so `--system "be brief"` is two arguments and not three. Placeholders are filled in
//! **one pass**: a `{model}` that is part of the prompt (a transcript discussing this very syntax)
//! is text, and stays text.
//!
//! # How the text gets in (`input_mode`)
//!
//! - `stdin` — system, ask and transcript go down the pipe. `{prompt}`, if the template has one, is
//!   a one-line pointer to that input ([`STDIN_POINTER`]): never the text itself, which would then be
//!   said twice and be multi-line on the command line.
//! - `file` — the text is written to a private file deleted after the call; `{file}` is its path
//!   (appended as the last argument when the template has no `{file}`), and `{prompt}` a one-line
//!   pointer to it.
//! - `arg` — the whole text is `{prompt}` (appended when the template has none). Refused up front on
//!   a Windows `.cmd`/`.bat` program, whose `cmd.exe` layer rejects a multi-line argument.

use super::files::CallFiles;
use super::spawn::is_batch_shim;
use super::text::first_nonempty;
use super::{Invocation, TextCall};
use crate::db::provider_queries::CliProviderRow;

/// What `{prompt}` becomes when the text itself travels on stdin.
const STDIN_POINTER: &str = "Follow the instructions in the input provided on stdin and reply with only the requested output.";

/// What `{prompt}` becomes when the text travels in a file.
fn file_pointer(path: &str) -> String {
    format!("The file at {path} holds your complete instructions and the input to work on. Follow them exactly and reply with only the requested output.")
}

pub(super) fn invocation(row: &CliProviderRow, program: &str, call: &TextCall<'_>, files: &CallFiles) -> Result<Invocation, String> {
    let template = row.args_template.as_str();
    let text = call.brief(true);
    let model = call.model;
    let (args, stdin) = match row.input_mode.as_str() {
        "file" => {
            let path = files
                .write("custom", "txt", &text)
                .ok_or_else(|| "could not write the temporary file the prompt goes in".to_string())?;
            let path = path.to_string_lossy().into_owned();
            let mut args = render_args(template, &file_pointer(&path), model, &path);
            if !template.contains("{file}") {
                args.push(path);
            }
            (args, String::new())
        }
        "arg" => {
            if is_batch_shim(program) && text.contains('\n') {
                return Err(format!(
                    "{} runs through cmd.exe, which refuses multi-line arguments — set this provider's input to \"stdin\" or \"file\"",
                    row.binary.trim()
                ));
            }
            let mut args = render_args(template, &text, model, "");
            if !template.contains("{prompt}") {
                args.push(text);
            }
            (args, String::new())
        }
        _ => (render_args(template, STDIN_POINTER, model, ""), text),
    };
    // An empty folder of its own rather than wherever the app was started from.
    Ok(Invocation { args, stdin, cwd: files.dir("custom-scratch"), env: Vec::new() })
}

/// Stdout, as it came. There is no envelope to parse because there is no agreement about one.
pub(super) fn interpret(success: bool, status_label: &str, stdout: &str, stderr: &str) -> Result<String, String> {
    let text = stdout.trim();
    if !success {
        let detail = first_nonempty(&[stderr, stdout], "");
        return Err(if detail.is_empty() {
            format!("the command failed ({status_label})")
        } else {
            format!("the command failed ({status_label}): {detail}")
        });
    }
    if text.is_empty() {
        // A zero exit and nothing on stdout: the binary ran and said nothing, which here means the
        // template or the input mode is wrong — and that is what the message has to say.
        return Err("the command printed nothing — check its arguments and input mode in Settings › AI providers".to_string());
    }
    Ok(text.to_string())
}

const PLACEHOLDERS: [&str; 3] = ["{prompt}", "{model}", "{file}"];

fn has_placeholder(word: &str) -> bool {
    PLACEHOLDERS.iter().any(|placeholder| word.contains(placeholder))
}

/// The template, as argv. Pure, and the whole of this provider's behaviour: splitting a line the
/// way a shell does and deciding what an unfilled placeholder means are the two things that decide
/// whether a user's configuration runs or fails confusingly.
pub(crate) fn render_args(template: &str, prompt: &str, model: &str, file: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    // Whether the last word pushed was a flag written as such in the template — the one an empty
    // placeholder right after it takes along.
    let mut last_was_flag = false;
    for word in split_words(template) {
        let empty = (word.contains("{model}") && model.trim().is_empty())
            || (word.contains("{prompt}") && prompt.is_empty())
            || (word.contains("{file}") && file.trim().is_empty());
        if empty {
            // ...and so is the flag it was the value of. Left behind, `--model` would take the
            // *next* argument — usually the prompt — as the model.
            if last_was_flag {
                out.pop();
            }
            last_was_flag = false;
            continue;
        }
        last_was_flag = word.starts_with('-') && !word.contains('=') && !has_placeholder(&word);
        out.push(fill(&word, prompt, model.trim(), file));
    }
    out
}

/// Every placeholder in `word` replaced, in one pass over the *template's* text: what goes in is
/// never searched again.
fn fill(word: &str, prompt: &str, model: &str, file: &str) -> String {
    let mut out = String::with_capacity(word.len() + prompt.len());
    let mut rest = word;
    while let Some(at) = rest.find('{') {
        out.push_str(&rest[..at]);
        let tail = &rest[at..];
        let hit = [("{prompt}", prompt), ("{model}", model), ("{file}", file)]
            .into_iter()
            .find(|(placeholder, _)| tail.starts_with(placeholder));
        match hit {
            Some((placeholder, value)) => {
                out.push_str(value);
                rest = &tail[placeholder.len()..];
            }
            None => {
                out.push('{');
                rest = &tail[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// Splits on whitespace, with single and double quotes grouping — the shell's rule, as far as a
/// configuration field needs it. No escapes and no variable expansion: this is an argument line,
/// not a script.
pub(crate) fn split_words(line: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    let mut started = false;
    for ch in line.chars() {
        match quote {
            Some(open) if ch == open => quote = None,
            Some(_) => current.push(ch),
            None if ch == '"' || ch == '\'' => {
                quote = Some(ch);
                // A quote is what *starts* an empty argument: `--system ""` is two words, the second
                // of them empty, and dropping it would change the command.
                started = true;
            }
            None if ch.is_whitespace() => {
                if started || !current.is_empty() {
                    words.push(std::mem::take(&mut current));
                    started = false;
                }
            }
            None => current.push(ch),
        }
    }
    if started || !current.is_empty() {
        words.push(current);
    }
    words
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(items: &[&str]) -> Vec<String> {
        items.iter().map(|i| i.to_string()).collect()
    }

    #[test]
    fn fills_the_placeholders() {
        assert_eq!(render_args("-p {prompt}", "resume la reunión", "", ""), s(&["-p", "resume la reunión"]));
        assert_eq!(render_args("--in={file} -m {model}", "", "llama3", "/tmp/x"), s(&["--in=/tmp/x", "-m", "llama3"]));
    }

    #[test]
    fn drops_an_argument_whose_placeholder_is_empty() {
        // The flag goes with its value: a bare `--model` would take "hola" as the model.
        assert_eq!(render_args("run --model {model} {prompt}", "hola", "", ""), s(&["run", "hola"]));
        // A placeholder glued to its flag is one argument, dropped alone.
        assert_eq!(render_args("run --model={model} {prompt}", "hola", "", ""), s(&["run", "hola"]));
        assert_eq!(render_args("run --model {model} {prompt}", "hola", "fast-1", ""), s(&["run", "--model", "fast-1", "hola"]));
        assert_eq!(render_args("run --file {file} -q", "hola", "", ""), s(&["run", "-q"]));
    }

    #[test]
    fn a_placeholder_inside_the_prompt_is_text() {
        assert_eq!(
            render_args("-m {model} -p {prompt}", "usa `${model}` y {file}", "fast-1", "/f"),
            s(&["-m", "fast-1", "-p", "usa `${model}` y {file}"])
        );
        assert_eq!(fill("{prompt}", "{x} {", "", ""), "{x} {");
        assert_eq!(fill("a{b}c", "", "", ""), "a{b}c");
    }

    #[test]
    fn groups_quoted_words_the_way_a_shell_does() {
        assert_eq!(render_args("--system \"be brief\" -p {prompt}", "hola", "", ""), s(&["--system", "be brief", "-p", "hola"]));
        assert_eq!(render_args("--system 'be brief'", "hola", "", ""), s(&["--system", "be brief"]));
        assert_eq!(split_words("a\"b c\"d"), s(&["ab cd"]));
    }

    #[test]
    fn keeps_an_argument_that_was_quoted_empty_on_purpose() {
        assert_eq!(render_args("--system \"\"", "hola", "", ""), vec!["--system".to_string(), String::new()]);
    }

    #[test]
    fn collapses_the_spacing_a_person_typed() {
        assert_eq!(render_args("  -p    {prompt}  ", "hola", "", ""), s(&["-p", "hola"]));
        assert!(split_words("   ").is_empty());
    }

    #[test]
    fn a_prompt_with_spaces_and_lines_stays_one_argument() {
        let args = render_args("-p {prompt}", "dos palabras\ny más", "", "");
        assert_eq!(args, s(&["-p", "dos palabras\ny más"]));
    }

    fn row(template: &str, mode: &str) -> CliProviderRow {
        CliProviderRow {
            id: "cli-x".into(),
            name: "X".into(),
            binary: "x".into(),
            args_template: template.into(),
            input_mode: mode.into(),
            model: String::new(),
            sort_order: 0,
        }
    }

    const CALL: TextCall<'static> = TextCall { system: "S", ask: "A", data: "D", model: "m1" };

    #[test]
    fn stdin_mode_pipes_the_text_and_points_at_it() {
        let files = CallFiles::new();
        let inv = invocation(&row("run {model} -p {prompt}", "stdin"), "x", &CALL, &files).unwrap();
        assert_eq!(inv.args, s(&["run", "m1", "-p", STDIN_POINTER]));
        assert_eq!(inv.stdin, "S\n\nA\n\n----- INPUT -----\n\nD");
        let bare = invocation(&row("", "stdin"), "x", &CALL, &files).unwrap();
        assert!(bare.args.is_empty());
    }

    #[test]
    fn file_mode_names_a_private_file_and_appends_it_when_the_template_does_not() {
        let files = CallFiles::new();
        let inv = invocation(&row("--input {file} -m {model}", "file"), "x", &CALL, &files).unwrap();
        assert_eq!(inv.args[0], "--input");
        assert_eq!(std::fs::read_to_string(&inv.args[1]).unwrap(), "S\n\nA\n\n----- INPUT -----\n\nD");
        assert!(inv.stdin.is_empty());
        let appended = invocation(&row("-p {prompt}", "file"), "x", &CALL, &files).unwrap();
        let path = appended.args.last().unwrap().clone();
        assert!(appended.args[1].contains(&path), "the prompt points at the file");
        assert!(std::path::Path::new(&path).is_file());
        drop(files);
        assert!(!std::path::Path::new(&path).exists());
    }

    #[test]
    fn arg_mode_puts_everything_in_the_prompt_and_refuses_a_batch_shim() {
        let files = CallFiles::new();
        let inv = invocation(&row("-p {prompt}", "arg"), "x", &CALL, &files).unwrap();
        assert_eq!(inv.args, s(&["-p", "S\n\nA\n\n----- INPUT -----\n\nD"]));
        assert!(inv.stdin.is_empty());
        let appended = invocation(&row("--quiet", "arg"), "x", &CALL, &files).unwrap();
        assert_eq!(appended.args.len(), 2);
        assert!(invocation(&row("-p {prompt}", "arg"), r"C:\npm\x.cmd", &CALL, &files).is_err());
    }

    #[test]
    fn reads_stdout_as_the_answer_and_says_what_failed() {
        assert_eq!(interpret(true, "exit status: 0", "  la respuesta\n", "").unwrap(), "la respuesta");
        assert!(interpret(false, "exit status: 2", "", "command not found").unwrap_err().contains("command not found"));
        assert_eq!(interpret(false, "exit status: 2", "", "").unwrap_err(), "the command failed (exit status: 2)");
        assert!(interpret(true, "exit status: 0", "   ", "").unwrap_err().contains("printed nothing"));
    }
}
