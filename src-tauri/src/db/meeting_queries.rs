//! The meetings in the library, their folders, lines and speakers, the saved recipes, and the
//! voices the user named — see `crate::meetings`.
//!
//! **A meeting sits in the library**, optionally inside a folder (`folder_id`; deleting the folder
//! leaves its meetings at the library's root). Its notes (`notes_md`) are the document the AI
//! results are inserted into. The audio folder a deleted meeting leaves behind is swept at boot
//! (`meetings_cmd::start`).
//!
//! **Lines name their speaker by key**, not by row id: `me`, `others`, `p0`, `p1`… — the keys the
//! pipeline produces. A merge rewrites the keys of one speaker's lines into the other's and drops
//! the first row; a second pass replaces lines and speakers together, carrying over the names the
//! user gave (`replace_result`).
//!
//! **Voices are global** — the same colleague is in many meetings — and hold an
//! embedding (a BLOB of little-endian f32) from one model, named in `model`: a voice taken with
//! another model is never compared with this one's.

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::queries::now;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MeetingRow {
    pub id: String,
    /// `""` = the library's root.
    pub folder_id: String,
    pub title: String,
    pub notes_md: String,
    /// This meeting alone is local-only (its folder may make it so too — see [`is_local_only`]).
    pub local_only: bool,
    /// `virtual` (microphone + system) · `room` (microphone only) · `import`.
    pub kind: String,
    /// `light` · `balanced` · `full` — what it was recorded with.
    pub mode: String,
    /// `local` · `cloud` — what transcribed it.
    pub transcriber: String,
    pub language: String,
    /// `recording` · `paused` · `queued` · `processing` · `ready` · `failed` · `interrupted`.
    pub status: String,
    pub stage: String,
    pub error: String,
    pub started_at: String,
    pub ended_at: String,
    pub duration_ms: i64,
    /// JSON array of the channels recorded: `["mic","system"]`.
    pub channels: String,
    /// The audio file in the meeting's folder, `""` once it is gone.
    pub audio_file: String,
    pub audio_bytes: i64,
    /// When the audio is deleted, `""` = kept.
    pub audio_expires_at: String,
    pub speakers_hint: i64,
    /// Whether the live text covered the whole meeting.
    pub live_complete: bool,
    pub created_at: String,
    pub updated_at: String,
}

const MEETING_COLUMNS: &str = "id, COALESCE(folder_id, ''), title, notes_md, local_only, kind, mode, transcriber, language, status, stage, error, \
     started_at, ended_at, duration_ms, channels, audio_file, audio_bytes, audio_expires_at, speakers_hint, \
     live_complete, created_at, updated_at";

fn meeting_from(row: &rusqlite::Row<'_>) -> rusqlite::Result<MeetingRow> {
    Ok(MeetingRow {
        id: row.get(0)?,
        folder_id: row.get(1)?,
        title: row.get(2)?,
        notes_md: row.get(3)?,
        local_only: row.get::<_, i64>(4)? != 0,
        kind: row.get(5)?,
        mode: row.get(6)?,
        transcriber: row.get(7)?,
        language: row.get(8)?,
        status: row.get(9)?,
        stage: row.get(10)?,
        error: row.get(11)?,
        started_at: row.get(12)?,
        ended_at: row.get(13)?,
        duration_ms: row.get(14)?,
        channels: row.get(15)?,
        audio_file: row.get(16)?,
        audio_bytes: row.get(17)?,
        audio_expires_at: row.get(18)?,
        speakers_hint: row.get(19)?,
        live_complete: row.get::<_, i64>(20)? != 0,
        created_at: row.get(21)?,
        updated_at: row.get(22)?,
    })
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LineRow {
    pub seq: i64,
    pub channel: String,
    pub speaker: String,
    pub start_ms: i64,
    pub end_ms: i64,
    pub text: String,
    /// `live` · `final`.
    pub pass: String,
    pub edited: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SpeakerRow {
    pub key: String,
    pub name: String,
    pub color: i64,
    pub is_me: bool,
    pub voice_id: String,
    pub talk_ms: i64,
    /// Whether a voice can be saved from it (an embedding was taken).
    pub has_voice: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RecipeRow {
    pub id: String,
    pub name: String,
    pub prompt: String,
    pub sort_order: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VoiceRow {
    pub id: String,
    pub name: String,
    pub is_me: bool,
    pub samples: i64,
    pub model: String,
    pub created_at: String,
    pub updated_at: String,
}

// ---------------------------------------------------------------------------------------------
// Meetings
// ---------------------------------------------------------------------------------------------

pub struct NewMeeting<'a> {
    /// `""` = the library's root.
    pub folder_id: &'a str,
    pub title: &'a str,
    pub kind: &'a str,
    pub mode: &'a str,
    pub transcriber: &'a str,
    pub language: &'a str,
    pub channels: &'a [&'a str],
    pub status: &'a str,
}

pub fn create_meeting(conn: &Connection, new: &NewMeeting<'_>) -> rusqlite::Result<MeetingRow> {
    let id = Uuid::new_v4().to_string();
    let at = now();
    let channels = serde_json::to_string(new.channels).unwrap_or_else(|_| "[]".into());
    conn.execute(
        "INSERT INTO meetings (id, folder_id, title, kind, mode, transcriber, language, status, started_at, channels, created_at, updated_at)
         VALUES (?1, NULLIF(?2, ''), ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?9, ?9)",
        params![id, new.folder_id, new.title, new.kind, new.mode, new.transcriber, new.language, new.status, at, channels],
    )?;
    get_meeting(conn, &id).map(|row| row.expect("just inserted"))
}

pub fn get_meeting(conn: &Connection, id: &str) -> rusqlite::Result<Option<MeetingRow>> {
    conn.query_row(&format!("SELECT {MEETING_COLUMNS} FROM meetings WHERE id = ?1"), params![id], meeting_from).optional()
}

/// What the library lists: one folder (`Some("")` = the root only, `None` = every meeting), newest
/// first, filtered by `query` over titles, notes and the transcript's text.
pub fn list_meetings(conn: &Connection, folder: Option<&str>, query: &str) -> rusqlite::Result<Vec<MeetingRow>> {
    let query = query.trim();
    let pattern = format!("%{}%", query.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_"));
    let mut statement = conn.prepare(&format!(
        "SELECT {MEETING_COLUMNS} FROM meetings m
         WHERE (?1 IS NULL OR COALESCE(m.folder_id, '') = ?1)
           AND (?2 = '' OR m.title LIKE ?3 ESCAPE '\\' OR m.notes_md LIKE ?3 ESCAPE '\\'
                OR EXISTS (SELECT 1 FROM meeting_lines l WHERE l.meeting_id = m.id AND l.text LIKE ?3 ESCAPE '\\'))
         ORDER BY m.started_at DESC"
    ))?;
    let rows = statement.query_map(params![folder, query, pattern], meeting_from)?.collect();
    rows
}

/// A transcript line that matched a search: where to jump to.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LineHit {
    pub meeting_id: String,
    pub start_ms: i64,
    pub text: String,
}

/// The first lines matching `query`, for the search results' snippets.
pub fn search_lines(conn: &Connection, query: &str, limit: i64) -> rusqlite::Result<Vec<LineHit>> {
    let pattern = format!("%{}%", query.trim().replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_"));
    let mut statement = conn.prepare(
        "SELECT meeting_id, start_ms, text FROM meeting_lines WHERE text LIKE ?1 ESCAPE '\\' ORDER BY meeting_id, start_ms LIMIT ?2",
    )?;
    let rows = statement
        .query_map(params![pattern, limit], |row| Ok(LineHit { meeting_id: row.get(0)?, start_ms: row.get(1)?, text: row.get(2)? }))?
        .collect();
    rows
}

pub fn set_title(conn: &Connection, id: &str, title: &str) -> rusqlite::Result<()> {
    conn.execute("UPDATE meetings SET title = ?2, updated_at = ?3 WHERE id = ?1", params![id, title.trim(), now()])?;
    Ok(())
}

pub fn set_notes(conn: &Connection, id: &str, notes: &str) -> rusqlite::Result<()> {
    conn.execute("UPDATE meetings SET notes_md = ?2, updated_at = ?3 WHERE id = ?1", params![id, notes, now()])?;
    Ok(())
}

/// Appends a block to the meeting's notes, a blank line apart — where an AI result goes.
pub fn append_notes(conn: &Connection, id: &str, block: &str) -> rusqlite::Result<String> {
    let current: String = conn.query_row("SELECT notes_md FROM meetings WHERE id = ?1", params![id], |row| row.get(0))?;
    let joined = if current.trim().is_empty() { block.trim().to_string() } else { format!("{}\n\n{}", current.trim_end(), block.trim()) };
    set_notes(conn, id, &joined)?;
    Ok(joined)
}

pub fn move_meeting(conn: &Connection, id: &str, folder_id: &str) -> rusqlite::Result<()> {
    conn.execute("UPDATE meetings SET folder_id = NULLIF(?2, ''), updated_at = ?3 WHERE id = ?1", params![id, folder_id, now()])?;
    Ok(())
}

pub fn set_meeting_local_only(conn: &Connection, id: &str, local_only: bool) -> rusqlite::Result<()> {
    conn.execute("UPDATE meetings SET local_only = ?2, updated_at = ?3 WHERE id = ?1", params![id, local_only, now()])?;
    Ok(())
}

pub fn meetings_with_status(conn: &Connection, statuses: &[&str]) -> rusqlite::Result<Vec<MeetingRow>> {
    let marks = statuses.iter().map(|_| "?").collect::<Vec<_>>().join(", ");
    let mut statement = conn.prepare(&format!("SELECT {MEETING_COLUMNS} FROM meetings WHERE status IN ({marks}) ORDER BY started_at"))?;
    let rows = statement.query_map(rusqlite::params_from_iter(statuses.iter()), meeting_from)?.collect();
    rows
}

pub fn set_status(conn: &Connection, id: &str, status: &str, stage: &str, error: &str) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE meetings SET status = ?2, stage = ?3, error = ?4, updated_at = ?5 WHERE id = ?1",
        params![id, status, stage, error, now()],
    )?;
    Ok(())
}

pub fn finish_recording(conn: &Connection, id: &str, duration_ms: i64, live_complete: bool) -> rusqlite::Result<()> {
    let at = now();
    conn.execute(
        "UPDATE meetings SET ended_at = ?2, duration_ms = ?3, live_complete = ?4, updated_at = ?2 WHERE id = ?1",
        params![id, at, duration_ms, live_complete],
    )?;
    Ok(())
}

pub fn set_duration(conn: &Connection, id: &str, duration_ms: i64) -> rusqlite::Result<()> {
    conn.execute("UPDATE meetings SET duration_ms = ?2 WHERE id = ?1", params![id, duration_ms])?;
    Ok(())
}

pub fn set_audio(conn: &Connection, id: &str, file: &str, bytes: i64, expires_at: &str) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE meetings SET audio_file = ?2, audio_bytes = ?3, audio_expires_at = ?4, updated_at = ?5 WHERE id = ?1",
        params![id, file, bytes, expires_at, now()],
    )?;
    Ok(())
}

pub fn set_processing_choices(conn: &Connection, id: &str, transcriber: &str, speakers_hint: i64) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE meetings SET transcriber = ?2, speakers_hint = ?3, updated_at = ?4 WHERE id = ?1",
        params![id, transcriber, speakers_hint, now()],
    )?;
    Ok(())
}

pub fn delete_meeting(conn: &Connection, id: &str) -> rusqlite::Result<()> {
    conn.execute("DELETE FROM meetings WHERE id = ?1", params![id])?;
    Ok(())
}

/// Meetings whose audio has outlived the retention: `(id, file)`.
pub fn expired_audio(conn: &Connection, at: &str) -> rusqlite::Result<Vec<(String, String)>> {
    let mut statement = conn.prepare(
        "SELECT id, audio_file FROM meetings WHERE audio_file != '' AND audio_expires_at != '' AND audio_expires_at <= ?1",
    )?;
    let rows = statement.query_map(params![at], |row| Ok((row.get(0)?, row.get(1)?)))?.collect();
    rows
}

pub fn all_meeting_ids(conn: &Connection) -> rusqlite::Result<Vec<String>> {
    let mut statement = conn.prepare("SELECT id FROM meetings")?;
    let rows = statement.query_map([], |row| row.get(0))?.collect();
    rows
}

// ---------------------------------------------------------------------------------------------
// Lines and speakers
// ---------------------------------------------------------------------------------------------

fn line_from(row: &rusqlite::Row<'_>) -> rusqlite::Result<LineRow> {
    Ok(LineRow {
        seq: row.get(0)?,
        channel: row.get(1)?,
        speaker: row.get(2)?,
        start_ms: row.get(3)?,
        end_ms: row.get(4)?,
        text: row.get(5)?,
        pass: row.get(6)?,
        edited: row.get::<_, i64>(7)? != 0,
    })
}

pub fn lines(conn: &Connection, meeting_id: &str) -> rusqlite::Result<Vec<LineRow>> {
    let mut statement = conn.prepare(
        "SELECT seq, channel, speaker, start_ms, end_ms, text, pass, edited FROM meeting_lines WHERE meeting_id = ?1 ORDER BY start_ms, seq",
    )?;
    let rows = statement.query_map(params![meeting_id], line_from)?.collect();
    rows
}

/// Appends a live line; its sequence number back.
pub fn append_line(conn: &Connection, meeting_id: &str, line: &LineRow) -> rusqlite::Result<i64> {
    let seq: i64 = conn.query_row("SELECT COALESCE(MAX(seq), 0) + 1 FROM meeting_lines WHERE meeting_id = ?1", params![meeting_id], |row| row.get(0))?;
    conn.execute(
        "INSERT INTO meeting_lines (meeting_id, seq, channel, speaker, start_ms, end_ms, text, pass, edited) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 0)",
        params![meeting_id, seq, line.channel, line.speaker, line.start_ms, line.end_ms, line.text, line.pass],
    )?;
    Ok(seq)
}

pub fn update_line_text(conn: &Connection, meeting_id: &str, seq: i64, text: &str) -> rusqlite::Result<()> {
    conn.execute("UPDATE meeting_lines SET text = ?3, edited = 1 WHERE meeting_id = ?1 AND seq = ?2", params![meeting_id, seq, text])?;
    Ok(())
}

pub fn set_line_speaker(conn: &Connection, meeting_id: &str, seq: i64, speaker: &str) -> rusqlite::Result<()> {
    conn.execute("UPDATE meeting_lines SET speaker = ?3 WHERE meeting_id = ?1 AND seq = ?2", params![meeting_id, seq, speaker])?;
    recount(conn, meeting_id)
}

fn speaker_from(row: &rusqlite::Row<'_>) -> rusqlite::Result<SpeakerRow> {
    Ok(SpeakerRow {
        key: row.get(0)?,
        name: row.get(1)?,
        color: row.get(2)?,
        is_me: row.get::<_, i64>(3)? != 0,
        voice_id: row.get(4)?,
        talk_ms: row.get(5)?,
        has_voice: row.get::<_, Option<Vec<u8>>>(6)?.is_some_and(|b| !b.is_empty()),
    })
}

pub fn speakers(conn: &Connection, meeting_id: &str) -> rusqlite::Result<Vec<SpeakerRow>> {
    let mut statement = conn.prepare(
        "SELECT key, name, color, is_me, voice_id, talk_ms, embedding FROM meeting_speakers WHERE meeting_id = ?1 ORDER BY sort_order, key",
    )?;
    let rows = statement.query_map(params![meeting_id], speaker_from)?.collect();
    rows
}

pub fn speaker_embedding(conn: &Connection, meeting_id: &str, key: &str) -> rusqlite::Result<Option<Vec<u8>>> {
    conn.query_row("SELECT embedding FROM meeting_speakers WHERE meeting_id = ?1 AND key = ?2", params![meeting_id, key], |row| row.get(0))
        .optional()
        .map(Option::flatten)
}

/// A speaker as the pipeline (or a live line) hands it in.
pub struct NewSpeaker {
    pub key: String,
    pub name: String,
    pub is_me: bool,
    pub voice_id: String,
    pub embedding: Option<Vec<u8>>,
    pub talk_ms: i64,
}

/// Makes sure a speaker row exists for `key` — a live line can name one before the final pass.
pub fn ensure_speaker(conn: &Connection, meeting_id: &str, key: &str, is_me: bool) -> rusqlite::Result<()> {
    let order: i64 = conn.query_row("SELECT COUNT(*) FROM meeting_speakers WHERE meeting_id = ?1", params![meeting_id], |row| row.get(0))?;
    conn.execute(
        "INSERT OR IGNORE INTO meeting_speakers (meeting_id, key, name, color, is_me, voice_id, talk_ms, sort_order) VALUES (?1, ?2, '', ?3, ?4, '', 0, ?3)",
        params![meeting_id, key, order, is_me],
    )?;
    Ok(())
}

/// Replaces a meeting's lines and speakers with a pass's result, in one transaction, keeping what the
/// user said about the old speakers: a new speaker overlapping an old one most in time takes its
/// name (unless the new one was matched to a saved voice), and lines the user edited by hand keep
/// their text when a new line covers the same moment on the same channel.
pub fn replace_result(conn: &Connection, meeting_id: &str, new_lines: &[LineRow], new_speakers: &[NewSpeaker]) -> rusqlite::Result<()> {
    let tx = conn.unchecked_transaction()?;
    let old_lines = lines(&tx, meeting_id)?;
    let old_speakers = speakers(&tx, meeting_id)?;
    // Old names, carried to the new key whose lines overlap the old key's lines most.
    let mut carried: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    for old in old_speakers.iter().filter(|s| !s.name.trim().is_empty()) {
        let mut overlap: std::collections::HashMap<&str, i64> = std::collections::HashMap::new();
        for a in old_lines.iter().filter(|l| l.speaker == old.key) {
            for b in new_lines.iter().filter(|l| l.channel == a.channel) {
                let shared = a.end_ms.min(b.end_ms) - a.start_ms.max(b.start_ms);
                if shared > 0 {
                    *overlap.entry(b.speaker.as_str()).or_default() += shared;
                }
            }
        }
        if let Some((key, _)) = overlap.into_iter().max_by_key(|(_, ms)| *ms) {
            carried.entry(key.to_string()).or_insert_with(|| old.name.clone());
        }
    }
    let edited: Vec<&LineRow> = old_lines.iter().filter(|l| l.edited).collect();
    tx.execute("DELETE FROM meeting_lines WHERE meeting_id = ?1", params![meeting_id])?;
    tx.execute("DELETE FROM meeting_speakers WHERE meeting_id = ?1", params![meeting_id])?;
    for (order, speaker) in new_speakers.iter().enumerate() {
        let name = if !speaker.name.trim().is_empty() { speaker.name.clone() } else { carried.get(&speaker.key).cloned().unwrap_or_default() };
        tx.execute(
            "INSERT INTO meeting_speakers (meeting_id, key, name, color, is_me, voice_id, embedding, talk_ms, sort_order) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?4)",
            params![meeting_id, speaker.key, name, order as i64, speaker.is_me, speaker.voice_id, speaker.embedding, speaker.talk_ms],
        )?;
    }
    for (index, line) in new_lines.iter().enumerate() {
        let kept = edited.iter().find(|old| old.channel == line.channel && (old.start_ms - line.start_ms).abs() < 1_500 && (old.end_ms - line.end_ms).abs() < 1_500);
        let (text, was_edited) = match kept {
            Some(old) => (old.text.as_str(), true),
            None => (line.text.as_str(), false),
        };
        tx.execute(
            "INSERT INTO meeting_lines (meeting_id, seq, channel, speaker, start_ms, end_ms, text, pass, edited) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![meeting_id, index as i64 + 1, line.channel, line.speaker, line.start_ms, line.end_ms, text, line.pass, was_edited],
        )?;
    }
    tx.commit()
}

pub fn rename_speaker(conn: &Connection, meeting_id: &str, key: &str, name: &str) -> rusqlite::Result<()> {
    conn.execute("UPDATE meeting_speakers SET name = ?3 WHERE meeting_id = ?1 AND key = ?2", params![meeting_id, key, name.trim()])?;
    Ok(())
}

pub fn set_speaker_voice(conn: &Connection, meeting_id: &str, key: &str, voice_id: &str, name: &str) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE meeting_speakers SET voice_id = ?3, name = CASE WHEN ?4 = '' THEN name ELSE ?4 END WHERE meeting_id = ?1 AND key = ?2",
        params![meeting_id, key, voice_id, name],
    )?;
    Ok(())
}

/// Every line of `from` becomes `into`'s, and `from` is gone.
pub fn merge_speakers(conn: &Connection, meeting_id: &str, from: &str, into: &str) -> rusqlite::Result<()> {
    if from == into {
        return Ok(());
    }
    let tx = conn.unchecked_transaction()?;
    tx.execute("UPDATE meeting_lines SET speaker = ?3 WHERE meeting_id = ?1 AND speaker = ?2", params![meeting_id, from, into])?;
    tx.execute("DELETE FROM meeting_speakers WHERE meeting_id = ?1 AND key = ?2", params![meeting_id, from])?;
    recount(&tx, meeting_id)?;
    tx.commit()
}

/// A new, empty speaker to move lines to — `p<n>` after the highest in use.
pub fn add_speaker(conn: &Connection, meeting_id: &str) -> rusqlite::Result<String> {
    let keys: Vec<String> = {
        let mut statement = conn.prepare("SELECT key FROM meeting_speakers WHERE meeting_id = ?1")?;
        let rows = statement.query_map(params![meeting_id], |row| row.get(0))?.collect::<rusqlite::Result<Vec<String>>>()?;
        rows
    };
    let next = keys.iter().filter_map(|k| k.strip_prefix('p').and_then(|n| n.parse::<usize>().ok())).max().map_or(0, |n| n + 1);
    let key = format!("p{next}");
    ensure_speaker(conn, meeting_id, &key, false)?;
    Ok(key)
}

/// Minutes spoken, recounted from the lines.
fn recount(conn: &Connection, meeting_id: &str) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE meeting_speakers SET talk_ms = COALESCE((SELECT SUM(end_ms - start_ms) FROM meeting_lines l WHERE l.meeting_id = meeting_speakers.meeting_id AND l.speaker = meeting_speakers.key), 0) WHERE meeting_id = ?1",
        params![meeting_id],
    )?;
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// Folders
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FolderRow {
    pub id: String,
    /// `""` = at the top.
    pub parent_id: String,
    pub name: String,
    pub local_only: bool,
    pub sort_order: i64,
    /// Meetings directly inside it.
    pub count: i64,
}

pub fn folders(conn: &Connection) -> rusqlite::Result<Vec<FolderRow>> {
    let mut statement = conn.prepare(
        "SELECT f.id, COALESCE(f.parent_id, ''), f.name, f.local_only, f.sort_order,
                (SELECT COUNT(*) FROM meetings m WHERE m.folder_id = f.id)
         FROM folders f ORDER BY f.sort_order, f.name COLLATE NOCASE",
    )?;
    let rows = statement
        .query_map([], |row| {
            Ok(FolderRow {
                id: row.get(0)?,
                parent_id: row.get(1)?,
                name: row.get(2)?,
                local_only: row.get::<_, i64>(3)? != 0,
                sort_order: row.get(4)?,
                count: row.get(5)?,
            })
        })?
        .collect();
    rows
}

pub fn create_folder(conn: &Connection, name: &str, parent_id: &str) -> rusqlite::Result<String> {
    let id = Uuid::new_v4().to_string();
    let at = now();
    let order: i64 = conn.query_row("SELECT COUNT(*) FROM folders", [], |row| row.get(0))?;
    conn.execute(
        "INSERT INTO folders (id, parent_id, name, sort_order, created_at, updated_at) VALUES (?1, NULLIF(?2, ''), ?3, ?4, ?5, ?5)",
        params![id, parent_id, name.trim(), order, at],
    )?;
    Ok(id)
}

pub fn rename_folder(conn: &Connection, id: &str, name: &str) -> rusqlite::Result<()> {
    conn.execute("UPDATE folders SET name = ?2, updated_at = ?3 WHERE id = ?1", params![id, name.trim(), now()])?;
    Ok(())
}

/// Deletes a folder and the folders inside it; their meetings move to the library's root.
pub fn delete_folder(conn: &Connection, id: &str) -> rusqlite::Result<()> {
    conn.execute("DELETE FROM folders WHERE id = ?1", params![id])?;
    Ok(())
}

pub fn set_folder_local_only(conn: &Connection, id: &str, local_only: bool) -> rusqlite::Result<()> {
    conn.execute("UPDATE folders SET local_only = ?2, updated_at = ?3 WHERE id = ?1", params![id, local_only, now()])?;
    Ok(())
}

/// Whether a folder (or any folder above it) is local-only; `""` (the root) never is.
pub fn folder_is_local_only(conn: &Connection, folder_id: &str) -> rusqlite::Result<bool> {
    if folder_id.is_empty() {
        return Ok(false);
    }
    conn.query_row(
        "WITH RECURSIVE chain(id, parent_id, local_only) AS (
             SELECT id, parent_id, local_only FROM folders WHERE id = ?1
             UNION ALL
             SELECT f.id, f.parent_id, f.local_only FROM folders f JOIN chain c ON f.id = c.parent_id
         )
         SELECT COALESCE(MAX(local_only), 0) FROM chain",
        params![folder_id],
        |row| row.get::<_, i64>(0),
    )
    .map(|v| v != 0)
}

/// Whether a meeting must stay on this computer: marked so itself, or filed in a local-only folder
/// (its own folder or any folder above it).
pub fn is_local_only(conn: &Connection, meeting_id: &str) -> rusqlite::Result<bool> {
    conn.query_row(
        "WITH RECURSIVE chain(id, parent_id, local_only) AS (
             SELECT f.id, f.parent_id, f.local_only FROM folders f JOIN meetings m ON m.folder_id = f.id WHERE m.id = ?1
             UNION ALL
             SELECT f.id, f.parent_id, f.local_only FROM folders f JOIN chain c ON f.id = c.parent_id
         )
         SELECT MAX(COALESCE((SELECT local_only FROM meetings WHERE id = ?1), 0), COALESCE((SELECT MAX(local_only) FROM chain), 0))",
        params![meeting_id],
        |row| row.get::<_, i64>(0),
    )
    .map(|v| v != 0)
}

// ---------------------------------------------------------------------------------------------
// Recipes
// ---------------------------------------------------------------------------------------------

fn recipe_from(row: &rusqlite::Row<'_>) -> rusqlite::Result<RecipeRow> {
    Ok(RecipeRow {
        id: row.get(0)?,
        name: row.get(1)?,
        prompt: row.get(2)?,
        sort_order: row.get(3)?,
        created_at: row.get(4)?,
        updated_at: row.get(5)?,
    })
}

pub fn recipes(conn: &Connection) -> rusqlite::Result<Vec<RecipeRow>> {
    let mut statement = conn.prepare("SELECT id, name, prompt, sort_order, created_at, updated_at FROM meeting_recipes ORDER BY sort_order, created_at")?;
    let rows = statement.query_map([], recipe_from)?.collect();
    rows
}

pub fn get_recipe(conn: &Connection, id: &str) -> rusqlite::Result<Option<RecipeRow>> {
    conn.query_row(
        "SELECT id, name, prompt, sort_order, created_at, updated_at FROM meeting_recipes WHERE id = ?1",
        params![id],
        recipe_from,
    )
    .optional()
}

pub fn save_recipe(conn: &Connection, id: Option<&str>, name: &str, prompt: &str) -> rusqlite::Result<RecipeRow> {
    let at = now();
    let id = match id {
        Some(id) => {
            conn.execute("UPDATE meeting_recipes SET name = ?2, prompt = ?3, updated_at = ?4 WHERE id = ?1", params![id, name.trim(), prompt.trim(), at])?;
            id.to_string()
        }
        None => {
            let id = Uuid::new_v4().to_string();
            let order: i64 = conn.query_row("SELECT COUNT(*) FROM meeting_recipes", [], |row| row.get(0))?;
            conn.execute(
                "INSERT INTO meeting_recipes (id, name, prompt, sort_order, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?5)",
                params![id, name.trim(), prompt.trim(), order, at],
            )?;
            id
        }
    };
    get_recipe(conn, &id).map(|row| row.expect("just written"))
}

pub fn delete_recipe(conn: &Connection, id: &str) -> rusqlite::Result<()> {
    conn.execute("DELETE FROM meeting_recipes WHERE id = ?1", params![id])?;
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// Voices
// ---------------------------------------------------------------------------------------------

fn voice_from(row: &rusqlite::Row<'_>) -> rusqlite::Result<VoiceRow> {
    Ok(VoiceRow {
        id: row.get(0)?,
        name: row.get(1)?,
        is_me: row.get::<_, i64>(2)? != 0,
        samples: row.get(3)?,
        model: row.get(4)?,
        created_at: row.get(5)?,
        updated_at: row.get(6)?,
    })
}

pub fn voices(conn: &Connection) -> rusqlite::Result<Vec<VoiceRow>> {
    let mut statement = conn.prepare("SELECT id, name, is_me, samples, model, created_at, updated_at FROM voice_profiles ORDER BY is_me DESC, name COLLATE NOCASE")?;
    let rows = statement.query_map([], voice_from)?.collect();
    rows
}

/// Voices of `model`, with their embeddings — what the matcher reads.
pub fn voice_embeddings(conn: &Connection, model: &str) -> rusqlite::Result<Vec<(VoiceRow, Vec<u8>)>> {
    let mut statement = conn.prepare("SELECT id, name, is_me, samples, model, created_at, updated_at, embedding FROM voice_profiles WHERE model = ?1")?;
    let rows = statement.query_map(params![model], |row| Ok((voice_from(row)?, row.get::<_, Vec<u8>>(7)?)))?.collect();
    rows
}

pub fn create_voice(conn: &Connection, name: &str, is_me: bool, embedding: &[u8], model: &str) -> rusqlite::Result<VoiceRow> {
    let id = Uuid::new_v4().to_string();
    let at = now();
    if is_me {
        // One "me".
        conn.execute("UPDATE voice_profiles SET is_me = 0 WHERE is_me = 1", [])?;
    }
    conn.execute(
        "INSERT INTO voice_profiles (id, name, is_me, embedding, samples, model, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, 1, ?5, ?6, ?6)",
        params![id, name.trim(), is_me, embedding, model, at],
    )?;
    Ok(VoiceRow { id, name: name.trim().to_string(), is_me, samples: 1, model: model.to_string(), created_at: at.clone(), updated_at: at })
}

/// Folds another sample of a voice into it: the running mean of `samples` embeddings.
pub fn update_voice_embedding(conn: &Connection, id: &str, embedding: &[u8], samples: i64) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE voice_profiles SET embedding = ?2, samples = ?3, updated_at = ?4 WHERE id = ?1",
        params![id, embedding, samples, now()],
    )?;
    Ok(())
}

pub fn get_voice_embedding(conn: &Connection, id: &str) -> rusqlite::Result<Option<(Vec<u8>, i64)>> {
    conn.query_row("SELECT embedding, samples FROM voice_profiles WHERE id = ?1", params![id], |row| Ok((row.get(0)?, row.get(1)?))).optional()
}

pub fn rename_voice(conn: &Connection, id: &str, name: &str) -> rusqlite::Result<()> {
    conn.execute("UPDATE voice_profiles SET name = ?2, updated_at = ?3 WHERE id = ?1", params![id, name.trim(), now()])?;
    conn.execute("UPDATE meeting_speakers SET name = ?2 WHERE voice_id = ?1", params![id, name.trim()])?;
    Ok(())
}

/// Forgets a voice: the embedding goes; the meetings keep the names already written.
pub fn delete_voice(conn: &Connection, id: &str) -> rusqlite::Result<()> {
    conn.execute("UPDATE meeting_speakers SET voice_id = '' WHERE voice_id = ?1", params![id])?;
    conn.execute("DELETE FROM voice_profiles WHERE id = ?1", params![id])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.pragma_update(None, "foreign_keys", "ON").unwrap();
        crate::db::migrations::run(&conn).unwrap();
        conn
    }

    fn line(channel: &str, speaker: &str, start: i64, end: i64, text: &str) -> LineRow {
        LineRow { seq: 0, channel: channel.into(), speaker: speaker.into(), start_ms: start, end_ms: end, text: text.into(), pass: "final".into(), edited: false }
    }

    #[test]
    fn a_folder_marked_local_only_covers_the_folders_inside_it() {
        let conn = db();
        let outer = create_folder(&conn, "Confidencial", "").unwrap();
        let inner = create_folder(&conn, "RR. HH.", &outer).unwrap();
        let meeting = create_meeting(&conn, &new_meeting(&inner)).unwrap();
        assert!(!is_local_only(&conn, &meeting.id).unwrap());
        set_folder_local_only(&conn, &outer, true).unwrap();
        assert!(is_local_only(&conn, &meeting.id).unwrap());
        set_folder_local_only(&conn, &outer, false).unwrap();
        set_meeting_local_only(&conn, &meeting.id, true).unwrap();
        assert!(is_local_only(&conn, &meeting.id).unwrap());
    }

    fn new_meeting(folder: &str) -> NewMeeting<'_> {
        NewMeeting { folder_id: folder, title: "Uno a uno", kind: "virtual", mode: "balanced", transcriber: "local", language: "es", channels: &["mic", "system"], status: "recording" }
    }

    #[test]
    fn the_library_lists_by_folder_and_searches_titles_notes_and_lines() {
        let conn = db();
        let folder = create_folder(&conn, "Clientes", "").unwrap();
        let a = create_meeting(&conn, &new_meeting(&folder)).unwrap();
        let b = create_meeting(&conn, &new_meeting("")).unwrap();
        set_title(&conn, &b.id, "Planning semanal").unwrap();
        append_line(&conn, &a.id, &line("system", "p0", 0, 1_000, "hablemos de 100% del presupuesto")).unwrap();
        append_notes(&conn, &b.id, "## Acta").unwrap();
        assert_eq!(append_notes(&conn, &b.id, "- tarea").unwrap(), "## Acta\n\n- tarea");
        assert_eq!(list_meetings(&conn, None, "").unwrap().len(), 2);
        assert_eq!(list_meetings(&conn, Some(""), "").unwrap().len(), 1);
        assert_eq!(list_meetings(&conn, Some(&folder), "").unwrap()[0].id, a.id);
        assert_eq!(list_meetings(&conn, None, "planning").unwrap()[0].id, b.id);
        assert_eq!(list_meetings(&conn, None, "100%").unwrap()[0].id, a.id);
        assert_eq!(list_meetings(&conn, None, "acta").unwrap()[0].id, b.id);
        assert!(list_meetings(&conn, None, "nada").unwrap().is_empty());
        assert_eq!(search_lines(&conn, "presupuesto", 5).unwrap().len(), 1);
        delete_folder(&conn, &folder).unwrap();
        assert_eq!(get_meeting(&conn, &a.id).unwrap().unwrap().folder_id, "");
    }

    #[test]
    fn a_second_pass_keeps_names_and_hand_edits_and_merges_fold_lines() {
        let conn = db();
        let meeting = create_meeting(&conn, &new_meeting("")).unwrap();
        let first = vec![line("system", "p0", 0, 4_000, "hola a todos"), line("system", "p1", 5_000, 9_000, "propongo cqrs")];
        let speakers = |keys: &[&str]| keys.iter().map(|k| NewSpeaker { key: k.to_string(), name: String::new(), is_me: false, voice_id: String::new(), embedding: Some(vec![0, 0, 128, 63]), talk_ms: 1 }).collect::<Vec<_>>();
        replace_result(&conn, &meeting.id, &first, &speakers(&["p0", "p1"])).unwrap();
        rename_speaker(&conn, &meeting.id, "p1", "María").unwrap();
        update_line_text(&conn, &meeting.id, 2, "Propongo CQRS.").unwrap();
        // The second pass numbers the voices the other way round.
        let second = vec![line("system", "p1", 100, 4_100, "hola a todos"), line("system", "p0", 5_100, 9_100, "propongo c q r s")];
        replace_result(&conn, &meeting.id, &second, &speakers(&["p0", "p1"])).unwrap();
        let after = speakers_of(&conn, &meeting.id);
        assert_eq!(after.iter().find(|s| s.key == "p0").unwrap().name, "María");
        let texts: Vec<String> = lines(&conn, &meeting.id).unwrap().into_iter().map(|l| l.text).collect();
        assert_eq!(texts, vec!["hola a todos", "Propongo CQRS."]);
        assert!(after.iter().all(|s| s.has_voice));
        merge_speakers(&conn, &meeting.id, "p1", "p0").unwrap();
        let merged = speakers_of(&conn, &meeting.id);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].talk_ms, 8_000);
        assert_eq!(add_speaker(&conn, &meeting.id).unwrap(), "p1");
    }

    fn speakers_of(conn: &Connection, id: &str) -> Vec<SpeakerRow> {
        speakers(conn, id).unwrap()
    }

    #[test]
    fn voices_rename_into_meetings_and_one_is_me() {
        let conn = db();
        let a = create_voice(&conn, "Ana", true, &[1, 2, 3, 4], "m").unwrap();
        let b = create_voice(&conn, "Yo", true, &[1, 2, 3, 4], "m").unwrap();
        let listed = voices(&conn).unwrap();
        assert_eq!(listed.iter().filter(|v| v.is_me).count(), 1);
        assert!(listed.iter().any(|v| v.id == b.id && v.is_me));
        rename_voice(&conn, &a.id, "Ana María").unwrap();
        assert_eq!(voice_embeddings(&conn, "m").unwrap().len(), 2);
        assert!(voice_embeddings(&conn, "other").unwrap().is_empty());
        delete_voice(&conn, &a.id).unwrap();
        assert_eq!(voices(&conn).unwrap().len(), 1);
    }
}
