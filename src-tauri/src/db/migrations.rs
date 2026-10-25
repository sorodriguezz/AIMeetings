//! The schema, one numbered step at a time. `PRAGMA user_version` records the last step applied, so
//! each runs exactly once per database.

use rusqlite::Connection;

const STEPS: &[&str] = &[
    // 1 — the initial schema.
    r#"
    CREATE TABLE IF NOT EXISTS app_settings (
        key   TEXT PRIMARY KEY,
        value TEXT NOT NULL
    );
    CREATE TABLE IF NOT EXISTS folders (
        id          TEXT PRIMARY KEY,
        parent_id   TEXT REFERENCES folders(id) ON DELETE CASCADE,
        name        TEXT NOT NULL,
        local_only  INTEGER NOT NULL DEFAULT 0,
        sort_order  INTEGER NOT NULL DEFAULT 0,
        created_at  TEXT NOT NULL,
        updated_at  TEXT NOT NULL
    );
    CREATE TABLE IF NOT EXISTS meetings (
        id               TEXT PRIMARY KEY,
        folder_id        TEXT REFERENCES folders(id) ON DELETE SET NULL,
        title            TEXT NOT NULL DEFAULT '',
        notes_md         TEXT NOT NULL DEFAULT '',
        local_only       INTEGER NOT NULL DEFAULT 0,
        kind             TEXT NOT NULL DEFAULT 'virtual',
        mode             TEXT NOT NULL DEFAULT 'balanced',
        transcriber      TEXT NOT NULL DEFAULT 'local',
        language         TEXT NOT NULL DEFAULT '',
        status           TEXT NOT NULL DEFAULT 'recording',
        stage            TEXT NOT NULL DEFAULT '',
        error            TEXT NOT NULL DEFAULT '',
        started_at       TEXT NOT NULL,
        ended_at         TEXT NOT NULL DEFAULT '',
        duration_ms      INTEGER NOT NULL DEFAULT 0,
        channels         TEXT NOT NULL DEFAULT '["mic"]',
        audio_file       TEXT NOT NULL DEFAULT '',
        audio_bytes      INTEGER NOT NULL DEFAULT 0,
        audio_expires_at TEXT NOT NULL DEFAULT '',
        speakers_hint    INTEGER NOT NULL DEFAULT 0,
        live_complete    INTEGER NOT NULL DEFAULT 0,
        created_at       TEXT NOT NULL,
        updated_at       TEXT NOT NULL
    );
    CREATE INDEX IF NOT EXISTS idx_meetings_folder ON meetings (folder_id, started_at);
    CREATE INDEX IF NOT EXISTS idx_meetings_started ON meetings (started_at);
    CREATE TABLE IF NOT EXISTS meeting_speakers (
        meeting_id  TEXT NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
        key         TEXT NOT NULL,
        name        TEXT NOT NULL DEFAULT '',
        color       INTEGER NOT NULL DEFAULT 0,
        is_me       INTEGER NOT NULL DEFAULT 0,
        voice_id    TEXT NOT NULL DEFAULT '',
        embedding   BLOB,
        talk_ms     INTEGER NOT NULL DEFAULT 0,
        sort_order  INTEGER NOT NULL DEFAULT 0,
        PRIMARY KEY (meeting_id, key)
    );
    CREATE TABLE IF NOT EXISTS meeting_lines (
        meeting_id  TEXT NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
        seq         INTEGER NOT NULL,
        channel     TEXT NOT NULL,
        speaker     TEXT NOT NULL,
        start_ms    INTEGER NOT NULL,
        end_ms      INTEGER NOT NULL,
        text        TEXT NOT NULL,
        pass        TEXT NOT NULL DEFAULT 'final',
        edited      INTEGER NOT NULL DEFAULT 0,
        PRIMARY KEY (meeting_id, seq)
    );
    CREATE INDEX IF NOT EXISTS idx_meeting_lines_time ON meeting_lines (meeting_id, start_ms);
    CREATE TABLE IF NOT EXISTS meeting_recipes (
        id           TEXT PRIMARY KEY,
        name         TEXT NOT NULL,
        prompt       TEXT NOT NULL,
        sort_order   INTEGER NOT NULL DEFAULT 0,
        created_at   TEXT NOT NULL,
        updated_at   TEXT NOT NULL
    );
    CREATE TABLE IF NOT EXISTS voice_profiles (
        id          TEXT PRIMARY KEY,
        name        TEXT NOT NULL,
        is_me       INTEGER NOT NULL DEFAULT 0,
        embedding   BLOB NOT NULL,
        samples     INTEGER NOT NULL DEFAULT 1,
        model       TEXT NOT NULL,
        created_at  TEXT NOT NULL,
        updated_at  TEXT NOT NULL
    );
    CREATE TABLE IF NOT EXISTS cli_providers (
        id            TEXT PRIMARY KEY,
        name          TEXT NOT NULL,
        binary        TEXT NOT NULL,
        args_template TEXT NOT NULL DEFAULT '{prompt}',
        input_mode    TEXT NOT NULL DEFAULT 'stdin',
        model         TEXT NOT NULL DEFAULT '',
        sort_order    INTEGER NOT NULL DEFAULT 0,
        created_at    TEXT NOT NULL,
        updated_at    TEXT NOT NULL
    );
    "#,
];

pub fn run(conn: &Connection) -> rusqlite::Result<()> {
    let applied: usize = conn.query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))? as usize;
    for (index, step) in STEPS.iter().enumerate().skip(applied) {
        let tx = conn.unchecked_transaction()?;
        tx.execute_batch(step)?;
        tx.pragma_update(None, "user_version", (index + 1) as i64)?;
        tx.commit()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn migrations_are_idempotent() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        super::run(&conn).unwrap();
        super::run(&conn).unwrap();
        let version: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0)).unwrap();
        assert_eq!(version, super::STEPS.len() as i64);
    }
}
