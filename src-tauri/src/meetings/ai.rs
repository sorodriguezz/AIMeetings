//! The AI side of a meeting: recipes that write minutes, decisions, tasks, a plan… into the
//! meeting's notes, and questions answered from the transcript.
//!
//! **Which engine.** The provider chosen for analysis — a CLI with the user's subscription
//! (`crate::providers`) or the local model (`crate::llm`) — and always the local model for a
//! meeting that is local-only, itself or by its folder. The transcript is text by then; the audio
//! never goes anywhere.
//!
//! **A transcript longer than the model's window** is the local case: a two-hour meeting is some
//! 25 000 tokens and a local model may hold 8 192. It is read in parts first — detailed notes per
//! part, with the minutes and names kept — and the recipe then runs over the notes (`map_reduce`).
//! The CLIs' models read two hours whole.

use serde::Deserialize;
use tokio_util::sync::CancellationToken;

use crate::llm::client::{self, ChatRequest};

/// The answer a local call is given room for — minutes, a plan, a follow-up mail.
const ANSWER_TOKENS: u32 = 2_048;

/// What every answer is told: be faithful, cite minutes, write Markdown in the reader's language.
const SYSTEM: &str = "Eres quien toma las notas de una reunión. Recibes los datos de la reunión y su \
transcripción automática, línea por línea con su minuto entre corchetes y quién habló. La \
transcripción puede traer palabras mal reconocidas, nombres mal escritos y hablantes sin nombre \
(«Persona 2»); corrige lo evidente por el contexto, pero no inventes: ningún acuerdo, fecha, cifra \
ni responsable que no aparezca. Cuando algo salga de un momento concreto, añade su minuto entre \
corchetes tal como aparece en la transcripción, por ejemplo [12:34]. Responde en Markdown, sin \
envolverlo en un bloque de código y sin repetir la transcripción.";

/// The built-in recipes, by id. The names the user sees are translated in the frontend
/// (`meetings.recipe.<id>`); the instructions are here, one language, and ask for the reader's.
pub fn builtin(id: &str) -> Option<&'static str> {
    Some(match id {
        "summary" => "Escribe el resumen de la reunión bajo el título «## Resumen»: de qué se habló y en qué quedó, en 5 a 10 viñetas breves, de lo más importante a lo menos.",
        "minutes" => "Escribe el acta bajo «## Acta»: una línea con fecha, duración y participantes; luego «### Temas» en el orden en que se trataron (cada tema con lo esencial y su minuto), «### Acuerdos» y «### Próximos pasos».",
        "decisions" => "Lista bajo «## Decisiones» cada decisión tomada: qué se decidió, quién la propuso o la aprobó, el motivo si se dijo, y su minuto. Si no se decidió nada, dilo en una línea.",
        "tasks" => "Lista bajo «## Tareas» los compromisos como checklist de Markdown, una tarea por línea: «- [ ] qué — responsable — fecha [minuto]». Responsable es quien se comprometió o a quien se le asignó; si no queda claro, escribe «sin responsable». La fecha solo si se dijo; si no, omítela. No inventes tareas que nadie asumió.",
        "plan" => "Arma bajo «## Planificación» el plan que sale de la reunión: una tabla Markdown con columnas Hito | Responsable | Fecha o plazo | Minuto, en orden cronológico. Lo que no tenga fecha va al final con «Sin fecha». Debajo, en dos o tres viñetas, las dependencias o riesgos del plan que se mencionaron.",
        "agenda" => "Propón bajo «## Próxima reunión» la agenda de la siguiente: los pendientes, las preguntas abiertas y los temas que quedaron por revisar, ordenados por prioridad, cada uno con una duración sugerida y quién debería traerlo.",
        "risks" => "Lista bajo «## Riesgos y preguntas abiertas» los riesgos, dudas, bloqueos y desacuerdos que se mencionaron, con quién los planteó, su minuto, y si quedó algo acordado al respecto.",
        "study" => "Convierte la reunión o clase en apuntes de estudio bajo «## Apuntes»: los temas en el orden en que se explicaron, cada uno con sus ideas clave en viñetas, definiciones en negrita, ejemplos que se dieron y su minuto. Al final, «### Para repasar» con tres a cinco puntos que conviene recordar.",
        "concepts" => "Lista bajo «## Conceptos clave» los términos, nombres y conceptos importantes que se mencionaron, cada uno con una definición breve y clara tal como se usó en la reunión o clase, y su minuto. Ordénalos alfabéticamente.",
        "quiz" => "Escribe bajo «## Preguntas de repaso» entre 8 y 12 preguntas para estudiar lo que se vio, de lo básico a lo más difícil: cada una con su respuesta corta debajo, en cursiva, y el minuto donde se explica. Mezcla preguntas de definición, de comprensión y de aplicación. Solo sobre lo que se dijo.",
        "followup" => "Redacta bajo «## Correo de seguimiento» un correo para los participantes: saludo breve, resumen en tres líneas, acuerdos, tareas con responsable y fecha, y cierre. Tono profesional y cordial. Solo el texto del correo, listo para copiar.",
        _ => return None,
    })
}

/// How a question about the meeting is asked.
const QUESTION: &str = "Responde la pregunta de abajo usando solo lo que dice la transcripción. Si la \
respuesta no está en ella, dilo en una línea. Cita los minutos de donde sale la respuesta.";

/// What the engine is told about the meeting, above the transcript.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Facts {
    pub title: String,
    pub date: String,
    pub duration: String,
    pub participants: Vec<String>,
    /// The reader's language, as a name the model understands ("español", "English").
    pub language: String,
}

pub fn context(facts: &Facts, transcript: &str) -> String {
    format!(
        "REUNIÓN: {}\nFECHA: {}\nDURACIÓN: {}\nPARTICIPANTES: {}\n\n=== TRANSCRIPCIÓN ===\n{}",
        facts.title.trim(),
        facts.date.trim(),
        facts.duration.trim(),
        facts.participants.join(", "),
        transcript
    )
}

fn ask_for(instruction: &str, facts: &Facts, question: bool) -> String {
    let language = if facts.language.trim().is_empty() { "el idioma de la reunión" } else { facts.language.trim() };
    if question {
        format!("{QUESTION}\nEscribe en {language}.\n\nPREGUNTA: {}", instruction.trim())
    } else {
        format!("{}\nEscribe en {language}.", instruction.trim())
    }
}

/// Where an answer is written.
pub enum Runner {
    Cli { provider: crate::providers::Provider, model: String },
    Local { live: crate::llm::Live },
}

impl Runner {
    /// Characters of transcript one call may carry. A CLI's models read hours; a local model's
    /// window is the limit — about 3.2 characters a token for Spanish, with room for the answer.
    fn budget(&self) -> usize {
        match self {
            Runner::Cli { .. } => 600_000,
            Runner::Local { live } => ((live.ctx.saturating_sub(ANSWER_TOKENS) as f32 * 0.8) * 3.2) as usize,
        }
    }

    async fn call(&self, ask: &str, stdin: &str, cancel: &CancellationToken) -> Result<String, String> {
        match self {
            Runner::Cli { provider, model } => {
                let call = crate::providers::TextCall { system: SYSTEM, ask, data: stdin, model };
                crate::providers::run_text(provider, call, cancel.clone()).await
            }
            Runner::Local { live } => {
                let user = format!("{ask}\n\n{stdin}");
                let request = ChatRequest { model: &live.model, system: SYSTEM, user: &user, max_tokens: ANSWER_TOKENS, temperature: 0.2 };
                let outcome = client::chat(&live.endpoint, &request, cancel).await?;
                // A server that cut the prompt to its window answers about part of the meeting only.
                let sent = (SYSTEM.len() + user.len()) as u64 / 4;
                if outcome.prompt_tokens.is_some_and(|read| read + 64 < sent / 2) {
                    return Err(format!(
                        "The local server read only {} tokens of what it was sent, so these notes would leave part of the meeting out. Raise the context under Models › Analysis.",
                        outcome.prompt_tokens.unwrap_or(0)
                    ));
                }
                if outcome.text.is_empty() {
                    return Err("The local model answered nothing. Try again, or pick a larger model.".into());
                }
                Ok(outcome.text)
            }
        }
    }
}

/// A recipe (`instruction`) or a question (`question = true`) over the transcript.
pub async fn answer(runner: &Runner, facts: &Facts, transcript: &str, instruction: &str, question: bool, cancel: &CancellationToken) -> Result<String, String> {
    let ask = ask_for(instruction, facts, question);
    let budget = runner.budget();
    if transcript.len() <= budget {
        return runner.call(&ask, &context(facts, transcript), cancel).await;
    }
    let notes = map_reduce(runner, facts, transcript, budget, cancel).await?;
    runner.call(&ask, &context(facts, &format!("(Notas detalladas de la reunión, tomadas por partes)\n{notes}")), cancel).await
}

/// Notes of a transcript too long for one call: detailed notes per part, then the notes condensed
/// again while they are still too long.
async fn map_reduce(runner: &Runner, facts: &Facts, transcript: &str, budget: usize, cancel: &CancellationToken) -> Result<String, String> {
    const MAP: &str = "Toma notas detalladas de este fragmento de la reunión, sin resumir de más: temas, \
        decisiones, tareas con su responsable, fechas, cifras, preguntas y desacuerdos. Conserva el \
        minuto entre corchetes de cada cosa y los nombres tal como aparecen. Viñetas Markdown.";
    let mut text = transcript.to_string();
    for _ in 0..4 {
        if text.len() <= budget {
            return Ok(text);
        }
        let parts = split_lines(&text, budget);
        let mut notes = Vec::with_capacity(parts.len());
        for (index, part) in parts.iter().enumerate() {
            let ask = format!("{MAP}\nParte {} de {}.", index + 1, parts.len());
            notes.push(runner.call(&ask, &context(facts, part), cancel).await?);
        }
        text = notes.join("\n\n");
    }
    Ok(text.chars().take(budget).collect())
}

/// `text` cut at line ends into parts no longer than `budget` bytes (a line longer than that is a
/// part of its own, cut).
pub fn split_lines(text: &str, budget: usize) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    for line in text.lines() {
        if !current.is_empty() && current.len() + line.len() + 1 > budget {
            parts.push(std::mem::take(&mut current));
        }
        if line.len() > budget {
            parts.push(line.chars().take(budget).collect());
            continue;
        }
        current.push_str(line);
        current.push('\n');
    }
    if !current.trim().is_empty() {
        parts.push(current);
    }
    parts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_builtin_recipe_has_instructions() {
        for id in ["summary", "study", "concepts", "quiz", "minutes", "decisions", "tasks", "plan", "agenda", "risks", "followup"] {
            assert!(builtin(id).is_some_and(|text| text.len() > 40), "{id}");
        }
        assert!(builtin("nope").is_none());
    }

    #[test]
    fn long_transcripts_split_at_line_ends() {
        let text = "[00:01] A: uno\n[00:02] B: dos\n[00:03] A: tres\n";
        let parts = split_lines(text, 30);
        assert_eq!(parts.len(), 2);
        assert!(parts.iter().all(|p| p.len() <= 30));
        assert_eq!(parts.concat(), text);
    }

    #[test]
    fn the_context_names_the_meeting_and_its_people() {
        let facts = Facts { title: "Arquitectura".into(), date: "9 oct".into(), duration: "47 min".into(), participants: vec!["Tú".into(), "María".into()], language: "español".into() };
        let text = context(&facts, "[00:01] Tú: hola\n");
        assert!(text.contains("PARTICIPANTES: Tú, María"));
        assert!(text.ends_with("[00:01] Tú: hola\n"));
        assert!(ask_for("¿qué se decidió?", &facts, true).contains("PREGUNTA: ¿qué se decidió?"));
    }
}
