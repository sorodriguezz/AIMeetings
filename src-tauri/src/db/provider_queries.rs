//! The CLI providers the user added by hand — any command-line program that reads a prompt and
//! prints an answer. The built-in ones (claude, codex, gemini…) are not rows: they are code, and
//! their settings live in `app_settings`.

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::queries::now;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CliProviderRow {
    pub id: String,
    pub name: String,
    /// A program name on `PATH` or a full path.
    pub binary: String,
    /// The arguments, with `{prompt}`, `{model}` and `{file}` filled in at run time.
    pub args_template: String,
    /// How the transcript reaches the program: `stdin`, `file` (a temporary file named by `{file}`)
    /// or `arg` (everything inside `{prompt}`).
    pub input_mode: String,
    pub model: String,
    pub sort_order: i64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CliProviderInput {
    pub id: Option<String>,
    pub name: String,
    pub binary: String,
    pub args_template: String,
    pub input_mode: String,
    #[serde(default)]
    pub model: String,
}

fn from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<CliProviderRow> {
    Ok(CliProviderRow {
        id: row.get(0)?,
        name: row.get(1)?,
        binary: row.get(2)?,
        args_template: row.get(3)?,
        input_mode: row.get(4)?,
        model: row.get(5)?,
        sort_order: row.get(6)?,
    })
}

const COLUMNS: &str = "id, name, binary, args_template, input_mode, model, sort_order";

pub fn list(conn: &Connection) -> rusqlite::Result<Vec<CliProviderRow>> {
    let mut statement = conn.prepare(&format!("SELECT {COLUMNS} FROM cli_providers ORDER BY sort_order, name COLLATE NOCASE"))?;
    let rows = statement.query_map([], from_row)?.collect();
    rows
}

pub fn get(conn: &Connection, id: &str) -> rusqlite::Result<Option<CliProviderRow>> {
    conn.query_row(&format!("SELECT {COLUMNS} FROM cli_providers WHERE id = ?1"), params![id], from_row).optional()
}

pub fn save(conn: &Connection, input: &CliProviderInput) -> rusqlite::Result<CliProviderRow> {
    let at = now();
    let mode = match input.input_mode.as_str() {
        "file" | "arg" => input.input_mode.as_str(),
        _ => "stdin",
    };
    let id = match input.id.as_deref().filter(|id| !id.is_empty()) {
        Some(id) => {
            conn.execute(
                "UPDATE cli_providers SET name = ?2, binary = ?3, args_template = ?4, input_mode = ?5, model = ?6, updated_at = ?7 WHERE id = ?1",
                params![id, input.name.trim(), input.binary.trim(), input.args_template.trim(), mode, input.model.trim(), at],
            )?;
            id.to_string()
        }
        None => {
            let id = format!("cli-{}", Uuid::new_v4().simple());
            let order: i64 = conn.query_row("SELECT COUNT(*) FROM cli_providers", [], |row| row.get(0))?;
            conn.execute(
                "INSERT INTO cli_providers (id, name, binary, args_template, input_mode, model, sort_order, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)",
                params![id, input.name.trim(), input.binary.trim(), input.args_template.trim(), mode, input.model.trim(), order, at],
            )?;
            id
        }
    };
    get(conn, &id).map(|row| row.expect("just written"))
}

pub fn delete(conn: &Connection, id: &str) -> rusqlite::Result<()> {
    conn.execute("DELETE FROM cli_providers WHERE id = ?1", params![id])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn providers_round_trip_and_unknown_modes_fall_back_to_stdin() {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::migrations::run(&conn).unwrap();
        let saved = save(
            &conn,
            &CliProviderInput { id: None, name: "Ollama".into(), binary: "ollama".into(), args_template: "run {model}".into(), input_mode: "weird".into(), model: "llama3".into() },
        )
        .unwrap();
        assert_eq!(saved.input_mode, "stdin");
        let edited = save(
            &conn,
            &CliProviderInput { id: Some(saved.id.clone()), name: "Ollama".into(), binary: "ollama".into(), args_template: "run {model} {prompt}".into(), input_mode: "arg".into(), model: "llama3".into() },
        )
        .unwrap();
        assert_eq!(edited.input_mode, "arg");
        assert_eq!(list(&conn).unwrap().len(), 1);
        delete(&conn, &saved.id).unwrap();
        assert!(list(&conn).unwrap().is_empty());
    }
}
