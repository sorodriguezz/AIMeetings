//! The app's one SQLite database: meetings, their lines and speakers, folders, recipes, voices,
//! CLI providers and settings.

pub mod meeting_queries;
pub mod migrations;
pub mod provider_queries;
pub mod queries;

use std::sync::Mutex;

use rusqlite::Connection;

pub struct Db(pub Mutex<Connection>);

/// Opens (creating if needed) the database at `paths::db_path()` and brings its schema up to date.
pub fn open() -> rusqlite::Result<Db> {
    let path = crate::paths::db_path();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let conn = configure(Connection::open(path)?)?;
    migrations::run(&conn)?;
    Ok(Db(Mutex::new(conn)))
}

/// An in-memory database with the full schema, for tests.
#[cfg(test)]
pub fn init_scratch() -> rusqlite::Result<Db> {
    let conn = configure(Connection::open_in_memory()?)?;
    migrations::run(&conn)?;
    Ok(Db(Mutex::new(conn)))
}

fn configure(conn: Connection) -> rusqlite::Result<Connection> {
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    conn.busy_timeout(std::time::Duration::from_secs(5))?;
    Ok(conn)
}
