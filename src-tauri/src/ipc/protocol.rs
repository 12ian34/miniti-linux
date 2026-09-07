//! Wire format shared by the socket server, the CLI client and the D-Bus
//! shim: one JSON object per line in each direction.
//!
//! Request: `{"cmd":"status"}`, `{"cmd":"start","title":"Weekly sync"}`, …
//! Response: `{"ok":true,"data":{…}}` or `{"ok":false,"error":"…"}`.
//! `subscribe` turns the connection into a stream of state snapshots (one
//! JSON object per line, the same shape as `state.json`).

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Protocol version carried in every snapshot; bump on breaking changes.
pub const SCHEMA: u32 = 1;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "cmd", rename_all = "kebab-case")]
pub enum Request {
    /// Liveness + identity of the instance owning the socket.
    Ping,
    /// Current snapshot (recording state, questions, prompt).
    Status,
    Start {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        title: Option<String>,
    },
    Stop,
    Toggle,
    /// Raise the main window.
    Show,
    /// Raise the main window on a saved meeting.
    OpenMeeting {
        id: String,
    },
    /// Deliver a deep link (`miniti-google://…`) to the running instance.
    OpenUrl {
        url: String,
    },
    /// Questions worth asking in the live meeting.
    Questions,
    Meetings {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        limit: Option<i64>,
    },
    Meeting {
        id: String,
    },
    /// Full Markdown export of one meeting (`last` = most recent).
    Export {
        id: String,
    },
    /// Answer the pending Smart-meeting prompt: primary | secondary | tertiary.
    Decide {
        choice: String,
    },
    /// Stream snapshots until the client disconnects.
    Subscribe,
    Quit,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Response {
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub data: Value,
}

impl Response {
    pub fn ok(data: impl Serialize) -> Self {
        Self {
            ok: true,
            error: None,
            data: serde_json::to_value(data).unwrap_or(Value::Null),
        }
    }

    pub fn err(message: impl Into<String>) -> Self {
        Self {
            ok: false,
            error: Some(message.into()),
            data: Value::Null,
        }
    }

    pub fn from_result(r: Result<impl Serialize, String>) -> Self {
        match r {
            Ok(v) => Self::ok(v),
            Err(e) => Self::err(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_use_a_cmd_tag_with_optional_fields() {
        let r: Request = serde_json::from_str(r#"{"cmd":"status"}"#).unwrap();
        assert_eq!(r, Request::Status);
        let r: Request = serde_json::from_str(r#"{"cmd":"start"}"#).unwrap();
        assert_eq!(r, Request::Start { title: None });
        let r: Request = serde_json::from_str(r#"{"cmd":"start","title":"Sync"}"#).unwrap();
        assert_eq!(
            r,
            Request::Start {
                title: Some("Sync".into())
            }
        );
        let r: Request = serde_json::from_str(r#"{"cmd":"open-meeting","id":"abc"}"#).unwrap();
        assert_eq!(r, Request::OpenMeeting { id: "abc".into() });
        assert!(serde_json::from_str::<Request>(r#"{"cmd":"explode"}"#).is_err());
        assert_eq!(
            serde_json::to_string(&Request::Toggle).unwrap(),
            r#"{"cmd":"toggle"}"#
        );
    }

    #[test]
    fn responses_omit_empty_fields() {
        assert_eq!(
            serde_json::to_string(&Response::ok(serde_json::json!({"a": 1}))).unwrap(),
            r#"{"ok":true,"data":{"a":1}}"#
        );
        assert_eq!(
            serde_json::to_string(&Response::err("no")).unwrap(),
            r#"{"ok":false,"error":"no"}"#
        );
        let r: Response = serde_json::from_str(r#"{"ok":true}"#).unwrap();
        assert!(r.ok && r.error.is_none() && r.data.is_null());
    }
}
