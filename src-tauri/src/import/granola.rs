//! Granola CSV importer (port of `GranolaCSVImporter`). Rows become meetings
//! with `import_source = "granola:<external id>"` so re-importing the same
//! export is a no-op.

use std::collections::{HashMap, HashSet};

use serde::Serialize;

use crate::db::{Meeting, TranscriptSegment};
use crate::deepgram::MIC_SPEAKER_ID;

pub const SOURCE: &str = "granola";
pub const EXPORT_URL: &str = "https://notes.granola.ai/settings/profile";

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ImportSummary {
    pub imported: usize,
    pub duplicates: usize,
    pub skipped_rows: usize,
}

impl ImportSummary {
    pub fn message(&self) -> String {
        let mut parts = vec![if self.imported > 0 {
            format!(
                "Imported {} meeting{}",
                self.imported,
                if self.imported == 1 { "" } else { "s" }
            )
        } else {
            "No new meetings imported".to_string()
        }];
        if self.duplicates > 0 {
            parts.push(format!("{} already imported", self.duplicates));
        }
        if self.skipped_rows > 0 {
            parts.push(format!(
                "{} row{} skipped",
                self.skipped_rows,
                if self.skipped_rows == 1 { "" } else { "s" }
            ));
        }
        parts.join(" · ")
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ImportedTurn {
    pub text: String,
    pub speaker_key: String,
    pub speaker_name: Option<String>,
    pub is_self: bool,
    pub relative_ts: Option<f64>,
    pub absolute_ts: Option<i64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ImportedAttendee {
    pub name: Option<String>,
    pub email: String,
    pub is_organizer: bool,
    pub is_self: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ImportedMeeting {
    pub external_id: String,
    pub title: String,
    pub start: i64,
    pub end: i64,
    pub summary: Option<String>,
    pub notes: String,
    pub transcript: Vec<ImportedTurn>,
    pub attendees: Vec<ImportedAttendee>,
    pub calendar_event_id: Option<String>,
    pub source_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParseResult {
    pub meetings: Vec<ImportedMeeting>,
    pub skipped_rows: usize,
}

const ID_HEADERS: &[&str] = &["id", "noteid", "documentid", "meetingid", "granolaid"];
const TITLE_HEADERS: &[&str] = &["title", "notetitle", "meetingtitle", "name"];
const START_HEADERS: &[&str] = &[
    "starttime",
    "meetingstart",
    "meetingdate",
    "date",
    "createdat",
    "created",
    "scheduledstarttime",
    "calendareventstart",
];
const END_HEADERS: &[&str] = &[
    "endtime",
    "meetingend",
    "updatedat",
    "updated",
    "scheduledendtime",
    "calendareventend",
];
const SUMMARY_HEADERS: &[&str] = &["summarytext", "summary", "notesummary", "aisummary"];
const NOTES_HEADERS: &[&str] = &[
    "notes",
    "notemarkdown",
    "notesmarkdown",
    "summarymarkdown",
    "enhancednotes",
    "privatenotes",
];
const TRANSCRIPT_HEADERS: &[&str] = &[
    "transcript",
    "rawtranscript",
    "fulltranscript",
    "transcription",
];
const ATTENDEE_HEADERS: &[&str] = &["attendees", "invitees", "participants", "people"];
const OWNER_HEADERS: &[&str] = &["owneremail", "owner", "createdbyemail"];
const ORGANIZER_HEADERS: &[&str] = &["organizer", "organiser", "organizeremail", "organiseremail"];
const CALENDAR_HEADERS: &[&str] = &["calendareventid", "calendarid", "eventid"];
const URL_HEADERS: &[&str] = &["weburl", "granolaurl", "url", "link"];

fn normalize_header(h: &str) -> String {
    h.trim_start_matches('\u{feff}')
        .to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric())
        .collect()
}

/// RFC 4180-ish parser: quoted fields, doubled quotes, CR/LF/CRLF rows.
pub fn parse_csv_rows(text: &str) -> Result<Vec<Vec<String>>, String> {
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if quoted {
            if c == '"' {
                if chars.get(i + 1) == Some(&'"') {
                    field.push('"');
                    i += 1;
                } else {
                    quoted = false;
                }
            } else {
                field.push(c);
            }
        } else {
            match c {
                '"' => quoted = true,
                ',' => {
                    row.push(std::mem::take(&mut field));
                }
                '\r' => {
                    if chars.get(i + 1) == Some(&'\n') {
                        i += 1;
                    }
                    row.push(std::mem::take(&mut field));
                    rows.push(std::mem::take(&mut row));
                }
                '\n' => {
                    row.push(std::mem::take(&mut field));
                    rows.push(std::mem::take(&mut row));
                }
                _ => field.push(c),
            }
        }
        i += 1;
    }
    if quoted {
        return Err("The Granola CSV contains an unfinished quoted field.".into());
    }
    if !field.is_empty() || !row.is_empty() {
        row.push(field);
        rows.push(row);
    }
    Ok(rows
        .into_iter()
        .filter(|r| !(r.len() == 1 && r[0].trim().is_empty()))
        .collect())
}

fn value(row: &[String], headers: &[String], aliases: &[&str]) -> String {
    for a in aliases {
        if let Some(i) = headers.iter().position(|h| h == a) {
            if let Some(v) = row.get(i) {
                let v = v.trim();
                if !v.is_empty() {
                    return v.to_string();
                }
            }
        }
    }
    String::new()
}

fn nil_if_empty(s: String) -> Option<String> {
    let t = s.trim().to_string();
    if t.is_empty() {
        None
    } else {
        Some(t)
    }
}

/// Port of `parseDate`: epoch seconds/millis, Excel serials, ISO 8601, common formats.
pub fn parse_date(raw: &str) -> Option<i64> {
    let v = raw.trim();
    if v.is_empty() {
        return None;
    }
    if let Ok(n) = v.parse::<f64>() {
        if n > 1_000_000_000_000.0 {
            return Some((n / 1000.0) as i64);
        }
        if n > 1_000_000_000.0 {
            return Some(n as i64);
        }
        if n > 20_000.0 && n < 100_000.0 {
            // Excel serial day number (1899-12-30 epoch).
            return Some(-2_209_161_600 + (n as i64) * 86_400);
        }
    }
    if let Ok(d) = chrono::DateTime::parse_from_rfc3339(v) {
        return Some(d.timestamp());
    }
    for fmt in ["%Y-%m-%dT%H:%M:%S%.fZ", "%Y-%m-%dT%H:%M:%SZ"] {
        if let Ok(d) = chrono::NaiveDateTime::parse_from_str(v, fmt) {
            return Some(d.and_utc().timestamp());
        }
    }
    for fmt in ["%Y-%m-%d %H:%M:%S %z", "%Y-%m-%d %H:%M:%S%z"] {
        if let Ok(d) = chrono::DateTime::parse_from_str(v, fmt) {
            return Some(d.timestamp());
        }
    }
    let local = |d: chrono::NaiveDateTime| {
        d.and_local_timezone(chrono::Local)
            .single()
            .map(|x| x.timestamp())
    };
    for fmt in [
        "%Y-%m-%d %H:%M:%S",
        "%Y-%m-%d %H:%M",
        "%b %e, %Y %l:%M %p",
        "%b %e, %Y, %l:%M %p",
        "%m/%d/%Y %l:%M %p",
        "%m/%d/%Y %H:%M",
        "%d/%m/%Y %H:%M",
    ] {
        if let Ok(d) = chrono::NaiveDateTime::parse_from_str(v, fmt) {
            return local(d);
        }
    }
    if let Ok(d) = chrono::NaiveDate::parse_from_str(v, "%Y-%m-%d") {
        return local(d.and_hms_opt(0, 0, 0)?);
    }
    None
}

fn parse_timecode(v: &str) -> Option<f64> {
    let parts: Vec<f64> = v
        .split(':')
        .map(|p| p.trim().parse::<f64>().ok())
        .collect::<Option<Vec<_>>>()?;
    match parts.len() {
        2 => Some(parts[0] * 60.0 + parts[1]),
        3 => Some(parts[0] * 3600.0 + parts[1] * 60.0 + parts[2]),
        _ => None,
    }
}

fn extract_leading_timestamp(line: &str) -> (Option<f64>, String) {
    let t = line.trim();
    if let Some(rest) = t.strip_prefix('[') {
        if let Some(close) = rest.find(']') {
            if let Some(ts) = parse_timecode(&rest[..close]) {
                return (Some(ts), rest[close + 1..].trim().to_string());
            }
        }
    }
    if let Some(sp) = t.find(char::is_whitespace) {
        let code = t[..sp].trim_matches(|c| c == '(' || c == ')');
        if let Some(ts) = parse_timecode(code) {
            return (Some(ts), t[sp..].trim().to_string());
        }
    }
    (None, t.to_string())
}

fn is_plausible_speaker(v: &str) -> bool {
    !v.is_empty()
        && v.chars().count() <= 60
        && v.split_whitespace().count() <= 6
        && v.chars()
            .all(|c| c.is_alphanumeric() || c.is_whitespace() || "-_'.".contains(c))
}

fn split_speaker_prefix(line: &str) -> Option<(String, String)> {
    let colon = line.find(':')?;
    let speaker = line[..colon].trim();
    let text = line[colon + 1..].trim();
    if text.is_empty() || !is_plausible_speaker(speaker) {
        return None;
    }
    Some((speaker.to_string(), text.to_string()))
}

fn is_generic_speaker(v: &str) -> bool {
    let l = v.trim().to_lowercase();
    if [
        "me",
        "you",
        "myself",
        "them",
        "microphone",
        "system",
        "unknown",
    ]
    .contains(&l.as_str())
    {
        return true;
    }
    let mut parts = l.split_whitespace();
    matches!(parts.next(), Some("speaker") | Some("participant"))
        && parts.clone().count() <= 1
        && parts.all(|p| p.chars().all(|c| c.is_alphanumeric()))
}

fn speaker_metadata(raw: &str) -> (String, Option<String>, bool) {
    let v = raw.trim();
    let lower = v.to_lowercase();
    let is_self = ["me", "you", "myself", "microphone"].contains(&lower.as_str());
    let generic =
        is_generic_speaker(v) || ["them", "speaker", "system", "unknown"].contains(&lower.as_str());
    (
        if lower.is_empty() {
            "unknown".into()
        } else {
            lower
        },
        if generic { None } else { Some(v.to_string()) },
        is_self,
    )
}

fn parse_json_transcript(text: &str) -> Option<Vec<ImportedTurn>> {
    let root: serde_json::Value = serde_json::from_str(text).ok()?;
    let items: Vec<serde_json::Value> = match root {
        serde_json::Value::Array(a) => a,
        serde_json::Value::Object(ref o) => ["transcript", "segments", "turns", "items", "entries"]
            .iter()
            .find_map(|k| o.get(*k).and_then(|v| v.as_array()).cloned())?,
        _ => return None,
    };
    let get = |o: &serde_json::Map<String, serde_json::Value>, keys: &[&str]| {
        keys.iter().find_map(|k| o.get(*k).cloned())
    };
    let mut turns = Vec::new();
    for item in items {
        let Some(o) = item.as_object() else { continue };
        let text = get(o, &["text", "content", "transcript", "utterance"])
            .and_then(|v| v.as_str().map(String::from))
            .unwrap_or_default();
        if text.trim().is_empty() {
            continue;
        }
        let speaker_raw = get(
            o,
            &["speaker", "speakerName", "speaker_name", "name", "source"],
        )
        .map(|v| {
            v.as_str()
                .map(String::from)
                .unwrap_or_else(|| v.to_string())
        })
        .unwrap_or_default();
        let (key, name, is_self) = speaker_metadata(&speaker_raw);
        let ts = get(
            o,
            &[
                "timestamp",
                "start",
                "startTime",
                "start_time",
                "time",
                "offset",
            ],
        );
        let (relative, absolute) = match ts {
            Some(serde_json::Value::Number(n)) => {
                let f = n.as_f64().unwrap_or(0.0);
                if f > 1_000_000_000_000.0 {
                    (None, Some((f / 1000.0) as i64))
                } else if f > 1_000_000_000.0 {
                    (None, Some(f as i64))
                } else {
                    (Some(f), None)
                }
            }
            Some(serde_json::Value::String(s)) => {
                if (s.contains('-') || s.contains('T')) && parse_date(&s).is_some() {
                    (None, parse_date(&s))
                } else if let Some(tc) = parse_timecode(&s) {
                    (Some(tc), None)
                } else if let Ok(f) = s.parse::<f64>() {
                    (Some(f), None)
                } else {
                    (None, parse_date(&s))
                }
            }
            _ => (None, None),
        };
        turns.push(ImportedTurn {
            text: text.trim().to_string(),
            speaker_key: key,
            speaker_name: name,
            is_self,
            relative_ts: relative,
            absolute_ts: absolute,
        });
    }
    Some(turns)
}

pub fn parse_transcript(raw: &str) -> Vec<ImportedTurn> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }
    if let Some(json) = parse_json_transcript(trimmed) {
        if !json.is_empty() {
            return json;
        }
    }
    let mut turns: Vec<ImportedTurn> = Vec::new();
    for raw_line in trimmed.lines() {
        let line = raw_line.trim();
        if line.is_empty() {
            continue;
        }
        let (ts, remainder) = extract_leading_timestamp(line);
        if let Some((speaker, text)) = split_speaker_prefix(&remainder) {
            let (key, name, is_self) = speaker_metadata(&speaker);
            turns.push(ImportedTurn {
                text,
                speaker_key: key,
                speaker_name: name,
                is_self,
                relative_ts: ts,
                absolute_ts: None,
            });
        } else if let Some(last) = turns.last_mut() {
            last.text = format!("{} {}", last.text, remainder);
            if last.relative_ts.is_none() {
                last.relative_ts = ts;
            }
        } else {
            turns.push(ImportedTurn {
                text: remainder,
                speaker_key: "unknown".into(),
                speaker_name: None,
                is_self: false,
                relative_ts: ts,
                absolute_ts: None,
            });
        }
    }
    turns
        .into_iter()
        .filter(|t| !t.text.trim().is_empty())
        .collect()
}

fn extract_email(v: &str) -> Option<String> {
    // Simple RFC-ish scan: token containing '@' with a dot after it.
    v.split(|c: char| {
        c.is_whitespace()
            || c == '<'
            || c == '>'
            || c == ','
            || c == ';'
            || c == '"'
            || c == '\''
            || c == '('
            || c == ')'
    })
    .find(|t| {
        let Some(at) = t.find('@') else { return false };
        let (local, domain) = t.split_at(at);
        !local.is_empty()
            && domain.len() > 3
            && domain[1..].contains('.')
            && domain[1..]
                .chars()
                .all(|c| c.is_alphanumeric() || c == '.' || c == '-')
    })
    .map(|s| s.to_lowercase())
}

fn extract_name(v: &str) -> Option<String> {
    let email = extract_email(v)?;
    let idx = v.to_lowercase().find(&email)?;
    nil_if_empty(
        v[..idx]
            .trim_matches(|c: char| c.is_whitespace() || "<>\"'".contains(c))
            .to_string(),
    )
}

pub fn parse_attendees(
    raw: &str,
    owner: Option<&str>,
    organizer: Option<&str>,
) -> Vec<ImportedAttendee> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }
    let mut pairs: Vec<(Option<String>, String)> = Vec::new();
    if let Ok(serde_json::Value::Array(items)) = serde_json::from_str::<serde_json::Value>(trimmed)
    {
        for item in items {
            if let Some(o) = item.as_object() {
                if let Some(email) = ["email", "emailAddress", "address"]
                    .iter()
                    .find_map(|k| o.get(*k).and_then(|v| v.as_str()))
                {
                    let name = ["name", "displayName"]
                        .iter()
                        .find_map(|k| o.get(*k).and_then(|v| v.as_str()))
                        .map(String::from);
                    pairs.push((name, email.to_string()));
                }
            } else if let Some(s) = item.as_str() {
                if let Some(email) = extract_email(s) {
                    pairs.push((extract_name(s), email));
                }
            }
        }
    } else {
        for item in trimmed.split([';', '\n']) {
            if let Some(email) = extract_email(item) {
                pairs.push((extract_name(item), email));
            }
        }
        if pairs.is_empty() {
            for tok in trimmed.split(|c: char| c.is_whitespace() || c == ',') {
                if let Some(email) = extract_email(tok) {
                    pairs.push((None, email));
                }
            }
        }
    }
    let mut seen = HashSet::new();
    pairs
        .into_iter()
        .filter_map(|(name, email)| {
            let email = email.trim().to_lowercase();
            if email.is_empty() || !seen.insert(email.clone()) {
                return None;
            }
            Some(ImportedAttendee {
                name: name.and_then(nil_if_empty),
                is_organizer: organizer
                    .map(|o| o.eq_ignore_ascii_case(&email))
                    .unwrap_or(false),
                is_self: owner
                    .map(|o| o.eq_ignore_ascii_case(&email))
                    .unwrap_or(false),
                email,
            })
        })
        .collect()
}

fn stable_id(title: &str, start: i64, transcript: &str) -> String {
    // FNV-1a over the identifying fields — stable across imports of the same row.
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in format!("{title}|{start}|{transcript}").bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    format!("csv-{h:016x}")
}

fn imported_meeting(row: &[String], headers: &[String]) -> Option<ImportedMeeting> {
    let title = value(row, headers, TITLE_HEADERS);
    let transcript_text = value(row, headers, TRANSCRIPT_HEADERS);
    let transcript = parse_transcript(&transcript_text);
    let row_start = parse_date(&value(row, headers, START_HEADERS));
    let first_abs = transcript.iter().filter_map(|t| t.absolute_ts).min();
    let start = row_start.or(first_abs)?;
    let row_end = parse_date(&value(row, headers, END_HEADERS));
    let last_abs = transcript.iter().filter_map(|t| t.absolute_ts).max();
    let last_rel = transcript
        .iter()
        .filter_map(|t| t.relative_ts)
        .fold(None, |m: Option<f64>, x| Some(m.map_or(x, |y| y.max(x))));
    let end = start.max(
        row_end
            .or(last_abs)
            .or(last_rel.map(|r| start + r as i64))
            .unwrap_or(start),
    );
    let source_url = nil_if_empty(value(row, headers, URL_HEADERS));
    let raw_id = nil_if_empty(value(row, headers, ID_HEADERS));
    let external_id = raw_id
        .or_else(|| source_url.clone())
        .unwrap_or_else(|| stable_id(&title, start, &transcript_text));
    let owner = extract_email(&value(row, headers, OWNER_HEADERS));
    let organizer = extract_email(&value(row, headers, ORGANIZER_HEADERS));
    let attendees = parse_attendees(
        &value(row, headers, ATTENDEE_HEADERS),
        owner.as_deref(),
        organizer.as_deref(),
    );
    Some(ImportedMeeting {
        external_id,
        title: if title.is_empty() {
            "untitled".into()
        } else {
            title
        },
        start,
        end,
        summary: nil_if_empty(value(row, headers, SUMMARY_HEADERS)),
        notes: value(row, headers, NOTES_HEADERS),
        transcript,
        attendees,
        calendar_event_id: nil_if_empty(value(row, headers, CALENDAR_HEADERS)),
        source_url,
    })
}

pub fn parse(text: &str) -> Result<ParseResult, String> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let rows = parse_csv_rows(text)?;
    let raw_headers = rows
        .first()
        .filter(|r| !r.is_empty())
        .ok_or("This CSV does not contain recognizable Granola meeting columns.")?;
    let headers: Vec<String> = raw_headers.iter().map(|h| normalize_header(h)).collect();
    let recognized: HashSet<&str> = [
        ID_HEADERS,
        TITLE_HEADERS,
        START_HEADERS,
        END_HEADERS,
        SUMMARY_HEADERS,
        NOTES_HEADERS,
        TRANSCRIPT_HEADERS,
        ATTENDEE_HEADERS,
        OWNER_HEADERS,
        ORGANIZER_HEADERS,
        CALENDAR_HEADERS,
        URL_HEADERS,
    ]
    .iter()
    .flat_map(|h| h.iter().copied())
    .collect();
    if !headers.iter().any(|h| recognized.contains(h.as_str())) {
        return Err("This CSV does not contain recognizable Granola meeting columns.".into());
    }
    let mut meetings = Vec::new();
    let mut skipped = 0;
    for row in rows.iter().skip(1) {
        if row.iter().all(|c| c.trim().is_empty()) {
            continue;
        }
        match imported_meeting(row, &headers) {
            Some(m) => meetings.push(m),
            None => skipped += 1,
        }
    }
    if meetings.is_empty() {
        return Err("No importable meetings were found. Each row needs a meeting date.".into());
    }
    Ok(ParseResult {
        meetings,
        skipped_rows: skipped,
    })
}

/// Convert one imported record into a `Meeting` + segments (port of `makeMeeting`).
/// Self speakers map to 1000; others get sequential low ids by first appearance.
pub fn make_meeting(
    rec: &ImportedMeeting,
    default_language: &str,
) -> (Meeting, Vec<TranscriptSegment>) {
    let mut m = Meeting::new(rec.title.clone(), default_language);
    m.started_at = Some(rec.start);
    m.ended_at = Some(rec.end.max(rec.start));
    m.summary = rec.summary.clone().unwrap_or_default();
    m.notes = rec.notes.clone();
    m.import_source = Some(format!("{SOURCE}:{}", rec.external_id));
    m.calendar_event_id = rec.calendar_event_id.clone();
    if !rec.attendees.is_empty() {
        m.attendees = serde_json::to_string(
            &rec.attendees
                .iter()
                .map(|a| {
                    serde_json::json!({
                        "email": a.email,
                        "name": a.name,
                        "domain": a.email.split('@').nth(1).unwrap_or_default(),
                        "is_organizer": a.is_organizer,
                        "is_self": a.is_self,
                    })
                })
                .collect::<Vec<_>>(),
        )
        .unwrap_or_else(|_| "[]".into());
    }
    let mut ids: HashMap<String, i64> = HashMap::new();
    let mut next_remote = 0i64;
    let mut names: HashMap<String, String> = HashMap::new();
    let mut segments = Vec::new();
    let mut cursor = 0.0f64;
    for t in &rec.transcript {
        let id = if t.is_self {
            MIC_SPEAKER_ID
        } else {
            *ids.entry(t.speaker_key.clone()).or_insert_with(|| {
                let id = next_remote;
                next_remote += 1;
                id
            })
        };
        if let Some(n) = &t.speaker_name {
            names.entry(id.to_string()).or_insert_with(|| n.clone());
        }
        let start = t
            .relative_ts
            .or_else(|| t.absolute_ts.map(|a| (a - rec.start) as f64))
            .unwrap_or(cursor)
            .max(0.0);
        let words = t.text.split_whitespace().count() as f64;
        let end = start + (words / 2.5).max(0.5);
        cursor = end;
        segments.push(TranscriptSegment::new(
            &m.id,
            id,
            t.text.clone(),
            start,
            end,
            if t.is_self { "microphone" } else { "system" },
        ));
    }
    if !names.is_empty() {
        m.speaker_names = serde_json::to_string(&names).unwrap_or_else(|_| "{}".into());
    }
    (m, segments)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csv_parser_handles_quotes_and_newlines() {
        let rows =
            parse_csv_rows("a,b\r\n\"x, y\",\"line1\nline2\"\n\"he said \"\"hi\"\"\",z\n").unwrap();
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[1], vec!["x, y", "line1\nline2"]);
        assert_eq!(rows[2][0], "he said \"hi\"");
        assert!(parse_csv_rows("a,\"unterminated").is_err());
    }

    #[test]
    fn dates_in_all_supported_shapes() {
        assert_eq!(parse_date("1700000000"), Some(1_700_000_000));
        assert_eq!(parse_date("1700000000000"), Some(1_700_000_000));
        assert_eq!(parse_date("2023-11-14T22:13:20Z"), Some(1_700_000_000));
        assert_eq!(parse_date("2023-11-14T22:13:20.000Z"), Some(1_700_000_000));
        assert_eq!(parse_date("2023-11-14T23:13:20+01:00"), Some(1_700_000_000));
        assert!(parse_date("2023-11-14 22:13").is_some());
        assert!(parse_date("Nov 14, 2023 10:13 PM").is_some());
        assert!(parse_date("2023-11-14").is_some());
        assert!(parse_date("45000").is_some(), "excel serial");
        assert!(parse_date("nope").is_none());
    }

    #[test]
    fn transcript_lines_with_timestamps_speakers_and_continuations() {
        let turns = parse_transcript(
            "[00:01] Me: hello there\n(00:05) Alex Smith: hi\ncontinued line\nSpeaker 2: yo",
        );
        assert_eq!(turns.len(), 3);
        assert!(turns[0].is_self);
        assert_eq!(turns[0].relative_ts, Some(1.0));
        assert_eq!(turns[1].speaker_name.as_deref(), Some("Alex Smith"));
        assert_eq!(turns[1].text, "hi continued line");
        assert_eq!(
            turns[2].speaker_name, None,
            "generic speaker labels carry no name"
        );
        assert_eq!(turns[2].speaker_key, "speaker 2");
    }

    #[test]
    fn transcript_json_shape() {
        let turns = parse_transcript(
            r#"[{"speaker":"You","text":"hi","start":1.5},{"speaker":"Bob","text":"hey","timestamp":"2023-11-14T22:13:20Z"}]"#,
        );
        assert_eq!(turns.len(), 2);
        assert!(turns[0].is_self);
        assert_eq!(turns[0].relative_ts, Some(1.5));
        assert_eq!(turns[1].absolute_ts, Some(1_700_000_000));
    }

    #[test]
    fn attendees_from_json_and_strings() {
        let a = parse_attendees(
            r#"[{"name":"Al","email":"AL@acme.com"},"Bo <bo@x.io>"]"#,
            Some("al@acme.com"),
            Some("bo@x.io"),
        );
        assert_eq!(a.len(), 2);
        assert_eq!(a[0].email, "al@acme.com");
        assert!(a[0].is_self);
        assert_eq!(a[1].name.as_deref(), Some("Bo"));
        assert!(a[1].is_organizer);
        let b = parse_attendees("x@y.com; x@y.com; Z <z@y.com>", None, None);
        assert_eq!(b.len(), 2, "deduped");
    }

    #[test]
    fn full_parse_and_meeting_conversion() {
        let csv = "\u{feff}id,Title,Start Time,End Time,Summary,Transcript,Attendees\n\
                   n1,Kickoff,2023-11-14T22:13:20Z,2023-11-14T22:43:20Z,Went well,\"Me: hi all\nAlex: hey\nMe: ok\",\"alex@acme.com\"\n\
                   ,No date,,,,,\n";
        let r = parse(csv).unwrap();
        assert_eq!(r.meetings.len(), 1);
        assert_eq!(r.skipped_rows, 1);
        let rec = &r.meetings[0];
        assert_eq!(rec.external_id, "n1");
        assert_eq!(rec.end - rec.start, 1800);
        let (m, segs) = make_meeting(rec, "en");
        assert_eq!(m.import_source.as_deref(), Some("granola:n1"));
        assert_eq!(m.summary, "Went well");
        assert_eq!(segs.len(), 3);
        assert_eq!(segs[0].speaker, 1000);
        assert_eq!(segs[1].speaker, 0);
        assert_eq!(segs[2].speaker, 1000);
        assert!(
            segs[1].start_s >= segs[0].end_s,
            "sequential timestamps when none given"
        );
        let names: HashMap<String, String> = serde_json::from_str(&m.speaker_names).unwrap();
        assert_eq!(names["0"], "Alex");
        assert!(m.attendees.contains("alex@acme.com"));
        assert!(parse("foo,bar\n1,2\n").is_err(), "unrecognized columns");
    }

    #[test]
    fn summary_message() {
        assert_eq!(
            ImportSummary {
                imported: 2,
                duplicates: 1,
                skipped_rows: 0
            }
            .message(),
            "Imported 2 meetings · 1 already imported"
        );
        assert_eq!(
            ImportSummary {
                imported: 0,
                duplicates: 0,
                skipped_rows: 1
            }
            .message(),
            "No new meetings imported · 1 row skipped"
        );
    }
}
