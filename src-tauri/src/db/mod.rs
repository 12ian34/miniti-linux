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
    suggested_questions TEXT NOT NULL DEFAULT '[]',
    speaker_names       TEXT NOT NULL DEFAULT '{}',
    meddpicc            TEXT NOT NULL DEFAULT '{}',
    attendees           TEXT NOT NULL DEFAULT '[]',
    managed_session_id  TEXT,
    calendar_event_id   TEXT,
    import_source       TEXT,
    pinned              INTEGER NOT NULL DEFAULT 0,
    insights_updated_at INTEGER,
    created_at          INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS transcript_segments (
    id          TEXT PRIMARY KEY,
    meeting_id  TEXT NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
    speaker     INTEGER NOT NULL,
    text        TEXT NOT NULL,
    start_s     REAL NOT NULL,
    end_s       REAL NOT NULL,
    source      TEXT NOT NULL DEFAULT 'mono',
    created_at  INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_segments_meeting ON transcript_segments(meeting_id, start_s);
CREATE INDEX IF NOT EXISTS idx_meetings_pinned ON meetings(pinned, started_at);
"#;

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
    pub suggested_questions: String,
    pub speaker_names: String,
    pub meddpicc: String,
    pub attendees: String,
    pub managed_session_id: Option<String>,
    pub calendar_event_id: Option<String>,
    pub import_source: Option<String>,
    pub pinned: bool,
    pub insights_updated_at: Option<i64>,
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
            suggested_questions: "[]".into(),
            speaker_names: "{}".into(),
            meddpicc: "{}".into(),
            attendees: "[]".into(),
            managed_session_id: None,
            calendar_event_id: None,
            import_source: None,
            pinned: false,
            insights_updated_at: None,
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
}

/// Remove a leading "YYYY-MM-DD HH:MM " style prefix Apple added to some titles.
pub fn strip_legacy_timestamp_prefix(title: &str) -> String {
    let bytes = title.as_bytes();
    // Matches "dddd-dd-dd" then a space.
    let looks_like_date = bytes.len() >= 11
        && bytes[0..4].iter().all(u8::is_ascii_digit)
        && bytes[4] == b'-'
        && bytes[5..7].iter().all(u8::is_ascii_digit)
        && bytes[7] == b'-'
        && bytes[8..10].iter().all(u8::is_ascii_digit);
    if looks_like_date {
        if let Some(idx) = title.find(' ') {
            // Drop the date and an optional following HH:MM token.
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
    conn.execute_batch("PRAGMA foreign_keys = ON;")?;
    conn.execute_batch(SCHEMA)
}

pub fn upsert_meeting(conn: &Connection, m: &Meeting) -> DbResult<()> {
    conn.execute(
        r#"INSERT INTO meetings
            (id,title,started_at,ended_at,language,notes,summary,action_items,key_decisions,
             topics,suggested_questions,speaker_names,meddpicc,attendees,managed_session_id,
             calendar_event_id,import_source,pinned,insights_updated_at,created_at)
           VALUES
            (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20)
           ON CONFLICT(id) DO UPDATE SET
             title=?2,started_at=?3,ended_at=?4,language=?5,notes=?6,summary=?7,action_items=?8,
             key_decisions=?9,topics=?10,suggested_questions=?11,speaker_names=?12,meddpicc=?13,
             attendees=?14,managed_session_id=?15,calendar_event_id=?16,import_source=?17,
             pinned=?18,insights_updated_at=?19"#,
        params![
            m.id, m.title, m.started_at, m.ended_at, m.language, m.notes, m.summary,
            m.action_items, m.key_decisions, m.topics, m.suggested_questions, m.speaker_names,
            m.meddpicc, m.attendees, m.managed_session_id, m.calendar_event_id, m.import_source,
            m.pinned as i64, m.insights_updated_at, m.created_at,
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
        suggested_questions: row.get(10)?,
        speaker_names: row.get(11)?,
        meddpicc: row.get(12)?,
        attendees: row.get(13)?,
        managed_session_id: row.get(14)?,
        calendar_event_id: row.get(15)?,
        import_source: row.get(16)?,
        pinned: row.get::<_, i64>(17)? != 0,
        insights_updated_at: row.get(18)?,
        created_at: row.get(19)?,
    })
}

const MEETING_COLS: &str = "id,title,started_at,ended_at,language,notes,summary,action_items,\
    key_decisions,topics,suggested_questions,speaker_names,meddpicc,attendees,managed_session_id,\
    calendar_event_id,import_source,pinned,insights_updated_at,created_at";

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

/// Case-insensitive search over title, summary, notes, and topics.
pub fn search_meetings(conn: &Connection, query: &str, limit: i64) -> DbResult<Vec<Meeting>> {
    let like = format!("%{}%", query);
    let sql = format!(
        "SELECT {MEETING_COLS} FROM meetings \
         WHERE title LIKE ?1 OR summary LIKE ?1 OR notes LIKE ?1 OR topics LIKE ?1 \
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

pub fn delete_meeting(conn: &Connection, id: &str) -> DbResult<()> {
    conn.execute("DELETE FROM meetings WHERE id=?1", params![id])?;
    Ok(())
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
         FROM transcript_segments WHERE meeting_id=?1 ORDER BY start_s ASC",
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

        m.summary = "did things".into();
        upsert_meeting(&conn, &m).unwrap();
        assert_eq!(get_meeting(&conn, &m.id).unwrap().unwrap().summary, "did things");

        assert_eq!(list_meetings(&conn, 10).unwrap().len(), 1);
    }

    #[test]
    fn pinned_sorts_first() {
        let conn = open_in_memory().unwrap();
        let a = Meeting::new("A", "en");
        let mut b = Meeting::new("B", "en");
        b.started_at = Some(a.started_at.unwrap() - 100); // older
        upsert_meeting(&conn, &a).unwrap();
        upsert_meeting(&conn, &b).unwrap();
        set_pinned(&conn, &b.id, true).unwrap();
        let list = list_meetings(&conn, 10).unwrap();
        assert_eq!(list[0].id, b.id, "pinned meeting must come first");
    }

    #[test]
    fn search_matches_summary_and_topics() {
        let conn = open_in_memory().unwrap();
        let mut m = Meeting::new("Weekly", "en");
        m.summary = "discussed churn reduction".into();
        upsert_meeting(&conn, &m).unwrap();
        assert_eq!(search_meetings(&conn, "churn", 10).unwrap().len(), 1);
        assert_eq!(search_meetings(&conn, "nope", 10).unwrap().len(), 0);
    }

    #[test]
    fn segments_cascade_and_trim() {
        let conn = open_in_memory().unwrap();
        let m = Meeting::new("M", "en");
        upsert_meeting(&conn, &m).unwrap();
        for i in 0..5 {
            let seg = TranscriptSegment::new(&m.id, 0, format!("w{i}"), i as f64, i as f64 + 0.5, "mono");
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
