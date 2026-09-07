//! Unix-socket server: one JSON request per line, one JSON response per line;
//! `subscribe` upgrades the connection to a snapshot stream. Owns the socket
//! file (0600 in a 0700 dir) and refuses to start when another live instance
//! already holds it.

use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};

use super::protocol::{Request, Response};
use crate::state::{AppState, StartOptions};

pub fn spawn(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        if let Err(e) = serve(app).await {
            tracing::warn!("control socket unavailable: {e}");
        }
    });
}

async fn serve(app: AppHandle) -> std::io::Result<()> {
    super::ensure_runtime_dir()?;
    let path = super::socket_path();
    if path.exists() {
        if super::client::ping(&path).is_ok() {
            tracing::warn!(
                "another miniti owns {}; this instance has no control socket",
                path.display()
            );
            return Ok(());
        }
        let _ = std::fs::remove_file(&path);
    }
    let listener = UnixListener::bind(&path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    }
    tracing::info!("control socket: {}", path.display());
    loop {
        let (stream, _) = listener.accept().await?;
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            if let Err(e) = handle_conn(app, stream).await {
                tracing::debug!("control client: {e}");
            }
        });
    }
}

async fn write_line<W: AsyncWriteExt + Unpin>(w: &mut W, line: &str) -> std::io::Result<()> {
    w.write_all(line.as_bytes()).await?;
    w.write_all(b"\n").await?;
    w.flush().await
}

async fn handle_conn(app: AppHandle, stream: UnixStream) -> std::io::Result<()> {
    let (r, mut w) = stream.into_split();
    let mut lines = BufReader::new(r).lines();
    while let Some(line) = lines.next_line().await? {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let req = match serde_json::from_str::<Request>(line) {
            Ok(r) => r,
            Err(e) => {
                let resp = Response::err(format!("bad request: {e}"));
                write_line(&mut w, &serde_json::to_string(&resp).unwrap_or_default()).await?;
                continue;
            }
        };
        if req == Request::Subscribe {
            let Some(hub) = super::snapshot::hub(&app) else {
                let resp = Response::err("state hub unavailable");
                write_line(&mut w, &serde_json::to_string(&resp).unwrap_or_default()).await?;
                continue;
            };
            let mut rx = hub.subscribe();
            let first = hub
                .last_json()
                .unwrap_or_else(|| serde_json::to_string(&super::snapshot::current(&app)).unwrap_or_default());
            write_line(&mut w, &first).await?;
            loop {
                match rx.recv().await {
                    Ok(json) => write_line(&mut w, &json).await?,
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
            return Ok(());
        }
        let resp = handle(&app, req).await;
        write_line(&mut w, &serde_json::to_string(&resp).unwrap_or_default()).await?;
    }
    Ok(())
}

fn meeting_summary(m: &crate::db::Meeting) -> Value {
    json!({
        "id": m.id,
        "title": m.display_title(),
        "started_at": m.started_at,
        "ended_at": m.ended_at,
        "duration_seconds": m.duration_seconds(),
        "pinned": m.pinned,
        "language": m.language,
    })
}

fn resolve_meeting_id(state: &AppState, id: &str) -> Result<String, String> {
    if id != "last" {
        return Ok(id.to_string());
    }
    let conn = state.db.lock().map_err(|_| "db poisoned")?;
    crate::db::list_meetings(&conn, 1)
        .map_err(|e| e.to_string())?
        .into_iter()
        .next()
        .map(|m| m.id)
        .ok_or_else(|| "no meetings yet".to_string())
}

/// Execute one request against the running app. Shared by the socket and the
/// D-Bus interface so both behave identically.
pub async fn handle(app: &AppHandle, req: Request) -> Response {
    let state = app.state::<AppState>();
    match req {
        Request::Ping => Response::ok(json!({
            "pid": std::process::id(),
            "app_version": crate::state::APP_VERSION,
        })),
        Request::Status => Response::ok(super::snapshot::current(app)),
        Request::Start { title } => {
            let opts = StartOptions {
                title: title.filter(|t| !t.trim().is_empty()),
                ..Default::default()
            };
            Response::from_result(
                crate::state::start_meeting(app.clone(), state.clone(), opts)
                    .await
                    .map(|id| {
                        let _ = app.emit("navigate_meeting", &id);
                        json!({ "meeting_id": id })
                    }),
            )
        }
        Request::Stop => Response::from_result(
            crate::state::stop_recording(app.clone(), state.clone())
                .await
                .map(|id| json!({ "meeting_id": id })),
        ),
        Request::Toggle => {
            let running = state.session.lock().map(|s| s.running).unwrap_or(false);
            if running {
                Response::from_result(
                    crate::state::stop_recording(app.clone(), state.clone())
                        .await
                        .map(|id| json!({ "recording": false, "meeting_id": id })),
                )
            } else {
                Response::from_result(
                    crate::state::start_meeting(app.clone(), state.clone(), StartOptions::default())
                        .await
                        .map(|id| {
                            let _ = app.emit("navigate_meeting", &id);
                            json!({ "recording": true, "meeting_id": id })
                        }),
                )
            }
        }
        Request::Show => {
            crate::shell::show_main(app);
            Response::ok(Value::Null)
        }
        Request::OpenMeeting { id } => match resolve_meeting_id(&state, &id) {
            Ok(id) => {
                crate::shell::show_main(app);
                let _ = app.emit("navigate_meeting", &id);
                Response::ok(json!({ "meeting_id": id }))
            }
            Err(e) => Response::err(e),
        },
        Request::OpenUrl { url } => {
            if crate::shell::handle_open_url(app, &url) {
                Response::ok(Value::Null)
            } else {
                Response::err(format!("not a miniti link: {url}"))
            }
        }
        Request::Questions => {
            let snap = super::snapshot::current(app);
            Response::ok(json!({
                "meeting_id": snap.presence.meeting_id,
                "recording": snap.presence.is_recording,
                "questions": snap.questions,
            }))
        }
        Request::Meetings { limit } => {
            let limit = limit.unwrap_or(20).clamp(1, 500);
            let r = state
                .db
                .lock()
                .map_err(|_| "db poisoned".to_string())
                .and_then(|conn| crate::db::list_meetings(&conn, limit).map_err(|e| e.to_string()))
                .map(|ms| ms.iter().map(meeting_summary).collect::<Vec<_>>());
            Response::from_result(r)
        }
        Request::Meeting { id } => {
            let r = resolve_meeting_id(&state, &id).and_then(|id| {
                let conn = state.db.lock().map_err(|_| "db poisoned")?;
                crate::db::get_meeting(&conn, &id)
                    .map_err(|e| e.to_string())?
                    .ok_or_else(|| "meeting not found".to_string())
            });
            Response::from_result(r)
        }
        Request::Export { id } => {
            let r = resolve_meeting_id(&state, &id).and_then(|id| {
                let markdown = crate::state::meeting_markdown(state.clone(), id.clone())?;
                let file_name = {
                    let conn = state.db.lock().map_err(|_| "db poisoned")?;
                    crate::db::get_meeting(&conn, &id)
                        .map_err(|e| e.to_string())?
                        .map(|m| crate::export::export_file_name(&m))
                        .unwrap_or_else(|| format!("{id}.md"))
                };
                Ok(json!({ "meeting_id": id, "file_name": file_name, "markdown": markdown }))
            });
            Response::from_result(r)
        }
        Request::Decide { choice } => {
            Response::from_result(crate::state::decide_from_shell(app, &choice).await)
        }
        Request::Subscribe => Response::err("subscribe is a stream; use it as the only request"),
        Request::Quit => {
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_millis(150)).await;
                app.exit(0);
            });
            Response::ok(Value::Null)
        }
    }
}
