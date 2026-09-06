//! SQLite persistence (PLAN.md §10). Meeting field semantics mirror the Apple
//! apps so webhooks and history behave identically. JSON-shaped fields (action
//! items, speaker names, MEDDPICC, attendees, …) are stored as TEXT blobs.

use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

pub type DbResult<T> = Result<T, rusqlite::Error>;

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS meetings (
    id                  TEXT PRIMARY KEY,
    title               TEXT NOT NULL DEFAULT '',
    started_at          INTEGER,
    ended_at            INTEGER,
    language            TEXT NOT NULL DEFAULT 'en',
    notes               TEXT NOT NULL DEFAULT '',
    summary             TEXT NOT NULL DEFAULT '',
    action_items        TEXT NOT NULL DEFAULT '[]',
    key_decisions       TEXT NOT NULL DEFAULT '[]',
    topics              TEXT NOT NULL DEFAULT '[]',
    discussion_flow     TEXT NOT NULL DEFAULT '[]',
    suggested_questions TEXT NOT NULL DEFAULT '[]',
    docs                TEXT NOT NULL DEFAULT '[]',
    speaker_names       TEXT NOT NULL DEFAULT '{}',
    self_speaker_ids    TEXT NOT NULL DEFAULT '[]',
    meddpicc            TEXT NOT NULL DEFAULT '{}',
    attendees           TEXT NOT NULL DEFAULT '[]',
    managed_session_id  TEXT,
    calendar_event_id   TEXT,
    import_source       TEXT,
    pinned              INTEGER NOT NULL DEFAULT 0,
    insights_updated_at INTEGER,
    sales_enabled       INTEGER NOT NULL DEFAULT 0,
    title_auto          INTEGER NOT NULL DEFAULT 1,
    doc_topics          TEXT NOT NULL DEFAULT '[]',
    manual_speaker_ids  TEXT NOT NULL DEFAULT '[]',
    investigations      TEXT NOT NULL DEFAULT '[]',
    created_at          INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS transcript_segments (
    id          TEXT PRIMARY KEY,
    meeting_id  TEXT NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
    speaker     INTEGER NOT NULL,
    text        TEXT NOT NULL,
    start_s     REAL NOT NULL,
    end_s       REAL NOT NULL,
    source      TEXT NOT NULL DEFAULT 'microphone',
    created_at  INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS calendar_prep (
    event_id    TEXT PRIMARY KEY,
    notes       TEXT NOT NULL DEFAULT '',
    updated_at  INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_segments_meeting ON transcript_segments(meeting_id, start_s);
CREATE INDEX IF NOT EXISTS idx_meetings_pinned ON meetings(pinned, started_at);
"#;

/// Columns added after the first schema; applied idempotently on open.
const MIGRATIONS: &[(&str, &str)] = &[
    ("meetings", "discussion_flow TEXT NOT NULL DEFAULT '[]'"),
    ("meetings", "docs TEXT NOT NULL DEFAULT '[]'"),
    ("meetings", "self_speaker_ids TEXT NOT NULL DEFAULT '[]'"),
    ("meetings", "sales_enabled INTEGER NOT NULL DEFAULT 0"),
    ("meetings", "title_auto INTEGER NOT NULL DEFAULT 1"),
    ("meetings", "doc_topics TEXT NOT NULL DEFAULT '[]'"),
    ("meetings", "manual_speaker_ids TEXT NOT NULL DEFAULT '[]'"),
    ("meetings", "investigations TEXT NOT NULL DEFAULT '[]'"),
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Meeting {
    pub id: String,
    pub title: String,
    pub started_at: Option<i64>,
    pub ended_at: Option<i64>,
    pub language: String,
    pub notes: String,
    pub summary: String,
    /// JSON array/object blobs (kept as strings; UI parses).
    pub action_items: String,
    pub key_decisions: String,
    pub topics: String,
    pub discussion_flow: String,
    pub suggested_questions: String,
    pub docs: String,
    pub speaker_names: String,
    /// JSON array of app speaker ids the user marked as "me". Empty = legacy
    /// default (1000 is You).
    pub self_speaker_ids: String,
    pub meddpicc: String,
    pub attendees: String,
    pub managed_session_id: Option<String>,
    pub calendar_event_id: Option<String>,
    pub import_source: Option<String>,
    pub pinned: bool,
    pub insights_updated_at: Option<i64>,
    /// Opt-in MEDDPICC analysis for this meeting.
    pub sales_enabled: bool,
    /// True while the title is generated (a manual rename clears it).
    pub title_auto: bool,
    /// Playbook topics with lookup state (JSON array).
    pub doc_topics: String,
    /// Speaker ids the user renamed by hand; inference never overwrites them.
    pub manual_speaker_ids: String,
    /// Investigation results (JSON array of {focus, scope, answer, sources, referenced_files, at}).
    pub investigations: String,
    pub created_at: i64,
}

impl Meeting {
    pub fn new(title: impl Into<String>, language: impl Into<String>) -> Self {
        let now = Utc::now().timestamp();
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            title: title.into(),
            started_at: Some(now),
            ended_at: None,
            language: language.into(),
            notes: String::new(),
            summary: String::new(),
            action_items: "[]".into(),
            key_decisions: "[]".into(),
            topics: "[]".into(),
            discussion_flow: "[]".into(),
            suggested_questions: "[]".into(),
            docs: "[]".into(),
            speaker_names: "{}".into(),
            self_speaker_ids: "[]".into(),
            meddpicc: "{}".into(),
            attendees: "[]".into(),
            managed_session_id: None,
            calendar_event_id: None,
            import_source: None,
            pinned: false,
            insights_updated_at: None,
            sales_enabled: false,
            title_auto: true,
            doc_topics: "[]".into(),
            manual_speaker_ids: "[]".into(),
            investigations: "[]".into(),
            created_at: now,
        }
    }

    /// `displayTitle`: strip legacy timestamp prefixes; never render empty.
    pub fn display_title(&self) -> String {
        let t = strip_legacy_timestamp_prefix(&self.title);
        if t.trim().is_empty() {
            "Untitled meeting".to_string()
        } else {
            t
        }
    }

    /// Explicit self speaker ids, or `None` for the legacy implicit default.
    pub fn self_speaker_ids(&self) -> Option<Vec<i64>> {
        serde_json::from_str::<Vec<i64>>(&self.self_speaker_ids)
            .ok()
            .filter(|v| !v.is_empty())
    }

    pub fn duration_seconds(&self) -> i64 {
        match (self.started_at, self.ended_at) {
            (Some(s), Some(e)) => (e - s).max(0),
            _ => 0,
        }
    }
}

/// Remove a leading "YYYY-MM-DD HH:MM " style prefix Apple added to some titles.
pub fn strip_legacy_timestamp_prefix(title: &str) -> String {
    let bytes = title.as_bytes();
    let looks_like_date = bytes.len() >= 11
        && bytes[0..4].iter().all(u8::is_ascii_digit)
        && bytes[4] == b'-'
        && bytes[5..7].iter().all(u8::is_ascii_digit)
        && bytes[7] == b'-'
        && bytes[8..10].iter().all(u8::is_ascii_digit);
    if looks_like_date {
        if let Some(idx) = title.find(' ') {
            let rest = title[idx + 1..].trim_start();
            let after_time = rest
                .split_once(' ')
                .filter(|(head, _)| head.contains(':'))
                .map(|(_, tail)| tail)
                .unwrap_or(rest);
            return after_time.trim().to_string();
        }
    }
    title.to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptSegment {
    pub id: String,
    pub meeting_id: String,
    pub speaker: i64,
    pub text: String,
    pub start_s: f64,
    pub end_s: f64,
    pub source: String,
}

impl TranscriptSegment {
    pub fn new(
        meeting_id: impl Into<String>,
        speaker: i64,
        text: impl Into<String>,
        start_s: f64,
        end_s: f64,
        source: impl Into<String>,
    ) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            meeting_id: meeting_id.into(),
            speaker,
            text: text.into(),
            start_s,
            end_s,
            source: source.into(),
        }
    }
}

pub fn open(path: &std::path::Path) -> DbResult<Connection> {
    let conn = Connection::open(path)?;
    init(&conn)?;
    Ok(conn)
}

pub fn open_in_memory() -> DbResult<Connection> {
    let conn = Connection::open_in_memory()?;
    init(&conn)?;
    Ok(conn)
}

fn init(conn: &Connection) -> DbResult<()> {
    conn.execute_batch("PRAGMA foreign_keys = ON; PRAGMA journal_mode = WAL;")?;
    conn.execute_batch(SCHEMA)?;
    migrate(conn)
}

fn has_column(conn: &Connection, table: &str, column: &str) -> DbResult<bool> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})"))?;
    let names = stmt.query_map([], |row| row.get::<_, String>(1))?;
    for n in names {
        if n? == column {
            return Ok(true);
        }
    }
    Ok(false)
}

fn migrate(conn: &Connection) -> DbResult<()> {
    for (table, decl) in MIGRATIONS {
        let column = decl.split_whitespace().next().unwrap_or_default();
        if !has_column(conn, table, column)? {
            conn.execute_batch(&format!("ALTER TABLE {table} ADD COLUMN {decl};"))?;
        }
    }
    Ok(())
}

const MEETING_COLS: &str = "id,title,started_at,ended_at,language,notes,summary,action_items,\
    key_decisions,topics,discussion_flow,suggested_questions,docs,speaker_names,self_speaker_ids,\
    meddpicc,attendees,managed_session_id,calendar_event_id,import_source,pinned,\
    insights_updated_at,sales_enabled,title_auto,doc_topics,manual_speaker_ids,investigations,created_at";

pub fn upsert_meeting(conn: &Connection, m: &Meeting) -> DbResult<()> {
    conn.execute(
        &format!(
            r#"INSERT INTO meetings ({MEETING_COLS}) VALUES
            (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21,?22,?23,?24,?25,?26,?27,?28)
           ON CONFLICT(id) DO UPDATE SET
             title=?2,started_at=?3,ended_at=?4,language=?5,notes=?6,summary=?7,action_items=?8,
             key_decisions=?9,topics=?10,discussion_flow=?11,suggested_questions=?12,docs=?13,
             speaker_names=?14,self_speaker_ids=?15,meddpicc=?16,attendees=?17,
             managed_session_id=?18,calendar_event_id=?19,import_source=?20,pinned=?21,
             insights_updated_at=?22,sales_enabled=?23,title_auto=?24,doc_topics=?25,
             manual_speaker_ids=?26,investigations=?27"#
        ),
        params![
            m.id, m.title, m.started_at, m.ended_at, m.language, m.notes, m.summary,
            m.action_items, m.key_decisions, m.topics, m.discussion_flow, m.suggested_questions,
            m.docs, m.speaker_names, m.self_speaker_ids, m.meddpicc, m.attendees,
            m.managed_session_id, m.calendar_event_id, m.import_source, m.pinned as i64,
            m.insights_updated_at, m.sales_enabled as i64, m.title_auto as i64, m.doc_topics,
            m.manual_speaker_ids, m.investigations, m.created_at,
        ],
    )?;
    Ok(())
}

fn row_to_meeting(row: &rusqlite::Row) -> DbResult<Meeting> {
    Ok(Meeting {
        id: row.get(0)?,
        title: row.get(1)?,
        started_at: row.get(2)?,
        ended_at: row.get(3)?,
        language: row.get(4)?,
        notes: row.get(5)?,
        summary: row.get(6)?,
        action_items: row.get(7)?,
        key_decisions: row.get(8)?,
        topics: row.get(9)?,
        discussion_flow: row.get(10)?,
        suggested_questions: row.get(11)?,
        docs: row.get(12)?,
        speaker_names: row.get(13)?,
        self_speaker_ids: row.get(14)?,
        meddpicc: row.get(15)?,
        attendees: row.get(16)?,
        managed_session_id: row.get(17)?,
        calendar_event_id: row.get(18)?,
        import_source: row.get(19)?,
        pinned: row.get::<_, i64>(20)? != 0,
        insights_updated_at: row.get(21)?,
        sales_enabled: row.get::<_, i64>(22)? != 0,
        title_auto: row.get::<_, i64>(23)? != 0,
        doc_topics: row.get(24)?,
        manual_speaker_ids: row.get(25)?,
        investigations: row.get(26)?,
        created_at: row.get(27)?,
    })
}

pub fn get_meeting(conn: &Connection, id: &str) -> DbResult<Option<Meeting>> {
    let sql = format!("SELECT {MEETING_COLS} FROM meetings WHERE id=?1");
    conn.query_row(&sql, params![id], row_to_meeting).optional()
}

/// List meetings: pinned first, then most recent.
pub fn list_meetings(conn: &Connection, limit: i64) -> DbResult<Vec<Meeting>> {
    let sql = format!(
        "SELECT {MEETING_COLS} FROM meetings \
         ORDER BY pinned DESC, COALESCE(started_at, created_at) DESC LIMIT ?1"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params![limit], row_to_meeting)?;
    rows.collect()
}

/// Escape `%`, `_` and `\` so user input is matched literally in LIKE.
fn like_escape(q: &str) -> String {
    q.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_")
}

/// Case-insensitive search over title, summary, notes, and topics.
pub fn search_meetings(conn: &Connection, query: &str, limit: i64) -> DbResult<Vec<Meeting>> {
    let like = format!("%{}%", like_escape(query));
    let sql = format!(
        "SELECT {MEETING_COLS} FROM meetings \
         WHERE title LIKE ?1 ESCAPE '\\' OR summary LIKE ?1 ESCAPE '\\' \
            OR notes LIKE ?1 ESCAPE '\\' OR topics LIKE ?1 ESCAPE '\\' \
         ORDER BY pinned DESC, COALESCE(started_at, created_at) DESC LIMIT ?2"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params![like, limit], row_to_meeting)?;
    rows.collect()
}

pub fn set_pinned(conn: &Connection, id: &str, pinned: bool) -> DbResult<()> {
    conn.execute(
        "UPDATE meetings SET pinned=?2 WHERE id=?1",
        params![id, pinned as i64],
    )?;
    Ok(())
}

/// Manual rename: also stops automatic title suggestions for this meeting.
pub fn set_title(conn: &Connection, id: &str, title: &str) -> DbResult<()> {
    conn.execute("UPDATE meetings SET title=?2, title_auto=0 WHERE id=?1", params![id, title])?;
    Ok(())
}

/// Generated title: only applied while the title is still automatic.
pub fn set_auto_title(conn: &Connection, id: &str, title: &str) -> DbResult<bool> {
    let n = conn.execute(
        "UPDATE meetings SET title=?2 WHERE id=?1 AND title_auto=1",
        params![id, title],
    )?;
    Ok(n > 0)
}

pub fn set_sales_enabled(conn: &Connection, id: &str, enabled: bool) -> DbResult<()> {
    conn.execute("UPDATE meetings SET sales_enabled=?2 WHERE id=?1", params![id, enabled as i64])?;
    Ok(())
}

/// Standard insights block.
pub fn set_standard_insights(
    conn: &Connection,
    id: &str,
    summary: &str,
    action_items_json: &str,
    topics_json: &str,
    discussion_flow_json: &str,
    key_decisions_json: Option<&str>,
) -> DbResult<()> {
    let now = Utc::now().timestamp();
    conn.execute(
        "UPDATE meetings SET summary=?2, action_items=?3, topics=?4, discussion_flow=?5, \
         key_decisions=COALESCE(?6, key_decisions), insights_updated_at=?7 WHERE id=?1",
        params![id, summary, action_items_json, topics_json, discussion_flow_json, key_decisions_json, now],
    )?;
    Ok(())
}

pub fn set_meddpicc(conn: &Connection, id: &str, meddpicc_json: &str) -> DbResult<()> {
    let now = Utc::now().timestamp();
    conn.execute(
        "UPDATE meetings SET meddpicc=?2, insights_updated_at=?3 WHERE id=?1",
        params![id, meddpicc_json, now],
    )?;
    Ok(())
}

pub fn set_questions(conn: &Connection, id: &str, questions_json: &str) -> DbResult<()> {
    let now = Utc::now().timestamp();
    conn.execute(
        "UPDATE meetings SET suggested_questions=?2, insights_updated_at=?3 WHERE id=?1",
        params![id, questions_json, now],
    )?;
    Ok(())
}

pub fn set_docs(conn: &Connection, id: &str, docs_json: &str, doc_topics_json: &str) -> DbResult<()> {
    conn.execute(
        "UPDATE meetings SET docs=?2, doc_topics=?3 WHERE id=?1",
        params![id, docs_json, doc_topics_json],
    )?;
    Ok(())
}

pub fn set_investigations(conn: &Connection, id: &str, json: &str) -> DbResult<()> {
    conn.execute("UPDATE meetings SET investigations=?2 WHERE id=?1", params![id, json])?;
    Ok(())
}

pub fn set_manual_speaker_ids(conn: &Connection, id: &str, json: &str) -> DbResult<()> {
    conn.execute("UPDATE meetings SET manual_speaker_ids=?2 WHERE id=?1", params![id, json])?;
    Ok(())
}

pub fn set_attendees(conn: &Connection, id: &str, json: &str) -> DbResult<()> {
    conn.execute("UPDATE meetings SET attendees=?2 WHERE id=?1", params![id, json])?;
    Ok(())
}

pub fn set_calendar_event_id(conn: &Connection, id: &str, event_id: Option<&str>) -> DbResult<()> {
    conn.execute("UPDATE meetings SET calendar_event_id=?2 WHERE id=?1", params![id, event_id])?;
    Ok(())
}

/// Replace a meeting's transcript wholesale (import / trim + regenerate).
pub fn replace_segments(conn: &Connection, meeting_id: &str, segments: &[TranscriptSegment]) -> DbResult<()> {
    conn.execute("DELETE FROM transcript_segments WHERE meeting_id=?1", params![meeting_id])?;
    for seg in segments {
        add_segment(conn, seg)?;
    }
    Ok(())
}

pub fn get_prep_notes(conn: &Connection, event_id: &str) -> DbResult<String> {
    conn.query_row("SELECT notes FROM calendar_prep WHERE event_id=?1", params![event_id], |r| r.get(0))
        .optional()
        .map(|o| o.unwrap_or_default())
}

pub fn set_prep_notes(conn: &Connection, event_id: &str, notes: &str) -> DbResult<()> {
    if notes.trim().is_empty() {
        conn.execute("DELETE FROM calendar_prep WHERE event_id=?1", params![event_id])?;
        return Ok(());
    }
    conn.execute(
        "INSERT INTO calendar_prep (event_id, notes, updated_at) VALUES (?1, ?2, ?3)
         ON CONFLICT(event_id) DO UPDATE SET notes=?2, updated_at=?3",
        params![event_id, notes, Utc::now().timestamp()],
    )?;
    Ok(())
}

/// Prep notes older than a week are pruned (port of `pruneExpiredCalendarPrepNotes`).
pub fn prune_prep_notes(conn: &Connection) -> DbResult<usize> {
    conn.execute(
        "DELETE FROM calendar_prep WHERE updated_at < ?1",
        params![Utc::now().timestamp() - 7 * 86_400],
    )
}

pub fn find_by_import_source(conn: &Connection, source: &str) -> DbResult<Vec<String>> {
    let mut stmt = conn.prepare("SELECT import_source FROM meetings WHERE import_source LIKE ?1")?;
    let rows = stmt.query_map(params![format!("{source}:%")], |r| r.get::<_, String>(0))?;
    rows.collect()
}

pub fn set_ended_at(conn: &Connection, id: &str, ended_at: i64) -> DbResult<()> {
    conn.execute("UPDATE meetings SET ended_at=?2 WHERE id=?1", params![id, ended_at])?;
    Ok(())
}

pub fn set_notes(conn: &Connection, id: &str, notes: &str) -> DbResult<()> {
    conn.execute("UPDATE meetings SET notes=?2 WHERE id=?1", params![id, notes])?;
    Ok(())
}

pub fn set_speaker_names(conn: &Connection, id: &str, names_json: &str) -> DbResult<()> {
    conn.execute(
        "UPDATE meetings SET speaker_names=?2 WHERE id=?1",
        params![id, names_json],
    )?;
    Ok(())
}

pub fn set_self_speaker_ids(conn: &Connection, id: &str, ids_json: &str) -> DbResult<()> {
    conn.execute(
        "UPDATE meetings SET self_speaker_ids=?2 WHERE id=?1",
        params![id, ids_json],
    )?;
    Ok(())
}

pub fn delete_meeting(conn: &Connection, id: &str) -> DbResult<()> {
    conn.execute("DELETE FROM meetings WHERE id=?1", params![id])?;
    Ok(())
}

pub fn meeting_exists(conn: &Connection, id: &str) -> DbResult<bool> {
    conn.query_row(
        "SELECT 1 FROM meetings WHERE id=?1",
        params![id],
        |_| Ok(()),
    )
    .optional()
    .map(|o| o.is_some())
}

pub fn add_segment(conn: &Connection, seg: &TranscriptSegment) -> DbResult<()> {
    conn.execute(
        "INSERT INTO transcript_segments (id,meeting_id,speaker,text,start_s,end_s,source,created_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
        params![
            seg.id, seg.meeting_id, seg.speaker, seg.text, seg.start_s, seg.end_s, seg.source,
            Utc::now().timestamp()
        ],
    )?;
    Ok(())
}

pub fn list_segments(conn: &Connection, meeting_id: &str) -> DbResult<Vec<TranscriptSegment>> {
    let mut stmt = conn.prepare(
        "SELECT id,meeting_id,speaker,text,start_s,end_s,source \
         FROM transcript_segments WHERE meeting_id=?1 ORDER BY start_s ASC, created_at ASC",
    )?;
    let rows = stmt.query_map(params![meeting_id], |row| {
        Ok(TranscriptSegment {
            id: row.get(0)?,
            meeting_id: row.get(1)?,
            speaker: row.get(2)?,
            text: row.get(3)?,
            start_s: row.get(4)?,
            end_s: row.get(5)?,
            source: row.get(6)?,
        })
    })?;
    rows.collect()
}

pub fn delete_segment(conn: &Connection, meeting_id: &str, segment_id: &str) -> DbResult<usize> {
    conn.execute(
        "DELETE FROM transcript_segments WHERE meeting_id=?1 AND id=?2",
        params![meeting_id, segment_id],
    )
}

/// Trim: drop segments starting after `to_s`.
pub fn trim_segments_after(conn: &Connection, meeting_id: &str, to_s: f64) -> DbResult<usize> {
    let n = conn.execute(
        "DELETE FROM transcript_segments WHERE meeting_id=?1 AND start_s > ?2",
        params![meeting_id, to_s],
    )?;
    Ok(n)
}

/// Segments at or after `from_s` (cheap window for nudges / catch-up).
pub fn list_segments_since(conn: &Connection, meeting_id: &str, from_s: f64) -> DbResult<Vec<TranscriptSegment>> {
    let mut stmt = conn.prepare(
        "SELECT id,meeting_id,speaker,text,start_s,end_s,source \
         FROM transcript_segments WHERE meeting_id=?1 AND end_s >= ?2 ORDER BY start_s ASC, created_at ASC",
    )?;
    let rows = stmt.query_map(params![meeting_id, from_s], |row| {
        Ok(TranscriptSegment {
            id: row.get(0)?,
            meeting_id: row.get(1)?,
            speaker: row.get(2)?,
            text: row.get(3)?,
            start_s: row.get(4)?,
            end_s: row.get(5)?,
            source: row.get(6)?,
        })
    })?;
    rows.collect()
}

/// Trim: drop segments starting before `from_s` (used by history trimming).
pub fn trim_segments_before(conn: &Connection, meeting_id: &str, from_s: f64) -> DbResult<usize> {
    let n = conn.execute(
        "DELETE FROM transcript_segments WHERE meeting_id=?1 AND start_s < ?2",
        params![meeting_id, from_s],
    )?;
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_get_and_list_roundtrip() {
        let conn = open_in_memory().unwrap();
        let mut m = Meeting::new("Standup", "en");
        upsert_meeting(&conn, &m).unwrap();
        let got = get_meeting(&conn, &m.id).unwrap().unwrap();
        assert_eq!(got.title, "Standup");
        assert_eq!(got.discussion_flow, "[]");

        m.summary = "did things".into();
        m.self_speaker_ids = "[1000,1001]".into();
        upsert_meeting(&conn, &m).unwrap();
        let got = get_meeting(&conn, &m.id).unwrap().unwrap();
        assert_eq!(got.summary, "did things");
        assert_eq!(got.self_speaker_ids(), Some(vec![1000, 1001]));

        assert_eq!(list_meetings(&conn, 10).unwrap().len(), 1);
        assert!(meeting_exists(&conn, &m.id).unwrap());
        assert!(!meeting_exists(&conn, "nope").unwrap());
    }

    #[test]
    fn migration_adds_missing_columns_to_old_schema() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE meetings (id TEXT PRIMARY KEY, title TEXT NOT NULL DEFAULT '', created_at INTEGER NOT NULL);",
        )
        .unwrap();
        assert!(!has_column(&conn, "meetings", "discussion_flow").unwrap());
        migrate(&conn).unwrap();
        assert!(has_column(&conn, "meetings", "discussion_flow").unwrap());
        assert!(has_column(&conn, "meetings", "self_speaker_ids").unwrap());
        assert!(has_column(&conn, "meetings", "doc_topics").unwrap());
        migrate(&conn).unwrap(); // idempotent
    }

    #[test]
    fn auto_title_stops_after_manual_rename() {
        let conn = open_in_memory().unwrap();
        let m = Meeting::new("New meeting", "en");
        upsert_meeting(&conn, &m).unwrap();
        assert!(set_auto_title(&conn, &m.id, "Q4 planning").unwrap());
        assert_eq!(get_meeting(&conn, &m.id).unwrap().unwrap().title, "Q4 planning");
        set_title(&conn, &m.id, "My name").unwrap();
        assert!(!set_auto_title(&conn, &m.id, "Generated").unwrap(), "manual title wins");
        assert_eq!(get_meeting(&conn, &m.id).unwrap().unwrap().title, "My name");
    }

    #[test]
    fn pinned_sorts_first() {
        let conn = open_in_memory().unwrap();
        let a = Meeting::new("A", "en");
        let mut b = Meeting::new("B", "en");
        b.started_at = Some(a.started_at.unwrap() - 100);
        upsert_meeting(&conn, &a).unwrap();
        upsert_meeting(&conn, &b).unwrap();
        set_pinned(&conn, &b.id, true).unwrap();
        let list = list_meetings(&conn, 10).unwrap();
        assert_eq!(list[0].id, b.id, "pinned meeting must come first");
    }

    #[test]
    fn search_matches_summary_and_topics_and_escapes_wildcards() {
        let conn = open_in_memory().unwrap();
        let mut m = Meeting::new("Weekly", "en");
        m.summary = "discussed churn reduction 100%".into();
        upsert_meeting(&conn, &m).unwrap();
        assert_eq!(search_meetings(&conn, "churn", 10).unwrap().len(), 1);
        assert_eq!(search_meetings(&conn, "nope", 10).unwrap().len(), 0);
        assert_eq!(search_meetings(&conn, "100%", 10).unwrap().len(), 1);
        assert_eq!(search_meetings(&conn, "%", 10).unwrap().len(), 1, "literal percent");
        assert_eq!(search_meetings(&conn, "_", 10).unwrap().len(), 0, "underscore is literal, not wildcard");
    }

    #[test]
    fn segments_cascade_and_trim() {
        let conn = open_in_memory().unwrap();
        let m = Meeting::new("M", "en");
        upsert_meeting(&conn, &m).unwrap();
        for i in 0..5 {
            let seg = TranscriptSegment::new(&m.id, 0, format!("w{i}"), i as f64, i as f64 + 0.5, "microphone");
            add_segment(&conn, &seg).unwrap();
        }
        assert_eq!(list_segments(&conn, &m.id).unwrap().len(), 5);
        let removed = trim_segments_before(&conn, &m.id, 3.0).unwrap();
        assert_eq!(removed, 3);
        assert_eq!(list_segments(&conn, &m.id).unwrap().len(), 2);

        delete_meeting(&conn, &m.id).unwrap();
        assert_eq!(list_segments(&conn, &m.id).unwrap().len(), 0, "cascade delete");
    }

    #[test]
    fn display_title_strips_legacy_prefix() {
        assert_eq!(strip_legacy_timestamp_prefix("2026-09-05 14:30 Sales sync"), "Sales sync");
        assert_eq!(strip_legacy_timestamp_prefix("Sales sync"), "Sales sync");
        let mut m = Meeting::new("", "en");
        assert_eq!(m.display_title(), "Untitled meeting");
        m.title = "2026-01-02 09:00 ".into();
        assert_eq!(m.display_title(), "Untitled meeting");
    }
}
