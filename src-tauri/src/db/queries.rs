//! Settings, and the one timestamp format every table uses.

use rusqlite::{params, Connection, OptionalExtension};

/// RFC 3339 in UTC, millisecond precision — sortable as text.
pub fn now() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

pub fn get_setting(conn: &Connection, key: &str) -> rusqlite::Result<Option<String>> {
    conn.query_row("SELECT value FROM app_settings WHERE key = ?1", params![key], |row| row.get(0)).optional()
}

/// Many settings in one pass.
pub fn get_settings(conn: &Connection, keys: &[String]) -> rusqlite::Result<std::collections::HashMap<String, String>> {
    let mut stmt = conn.prepare("SELECT value FROM app_settings WHERE key = ?1")?;
    let mut out = std::collections::HashMap::with_capacity(keys.len());
    for key in keys {
        let value: Option<String> = stmt.query_row(params![key], |row| row.get(0)).optional()?;
        if let Some(value) = value {
            out.insert(key.clone(), value);
        }
    }
    Ok(out)
}

pub fn set_setting(conn: &Connection, key: &str, value: &str) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO app_settings (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )?;
    Ok(())
}
