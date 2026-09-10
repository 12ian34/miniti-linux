//! `miniti` on the command line. The same binary is the desktop app and its
//! CLI: with no subcommand it launches (or focuses) the app, with one it talks
//! to the running instance over the control socket (`ipc`).
//!
//! Exit codes: 0 ok, 1 the app refused or failed, 2 usage error (clap),
//! 3 miniti is not running.

use std::io::Write;
use std::path::PathBuf;

use clap::{CommandFactory, Parser, Subcommand, ValueEnum};
use serde_json::{json, Value};

use crate::ipc::client::{self, Client};
use crate::ipc::protocol::{Request, Response};

pub const EXIT_FAILED: i32 = 1;
pub const EXIT_NOT_RUNNING: i32 = 3;

const ABOUT: &str = "miniti — AI meeting assistant with live transcription, insights and coaching";
const AFTER_HELP: &str = "\
Without a subcommand miniti opens the app (or raises the running one).
Subcommands talk to the running app over $XDG_RUNTIME_DIR/miniti/miniti.sock;
the same state is in $XDG_RUNTIME_DIR/miniti/state.json and on the session bus
as com.miniti.linux. See miniti(1) and docs/desktop-integration.md.

Examples:
  miniti status --waybar        one-line JSON for a Waybar custom module
  miniti start \"Weekly sync\"    start a meeting with a title
  miniti questions              questions worth asking right now
  miniti export last -o ~/notes/last-meeting.md";

#[derive(Parser, Debug)]
#[command(name = "miniti", version = crate::state::APP_VERSION, about = ABOUT, after_help = AFTER_HELP,
          subcommand_precedence_over_arg = true)]
pub struct Cli {
    /// Machine-readable JSON output
    #[arg(long, global = true)]
    pub json: bool,

    /// Launch with the main window hidden (tray only); used by launch at login
    #[arg(long)]
    pub hidden: bool,

    /// Launch and start a meeting as soon as the app is ready
    #[arg(long = "start-meeting")]
    pub start_meeting: bool,

    /// Title for the meeting started by --start-meeting
    #[arg(long, requires = "start_meeting", hide = true)]
    pub title: Option<String>,

    /// Links to open (OAuth returns arrive here via the desktop entry's %U)
    #[arg(value_name = "URL", hide = true, value_parser = parse_link)]
    pub urls: Vec<String>,

    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Recording state, elapsed time, live questions and any pending prompt
    Status {
        /// Waybar custom-module JSON: {"text","tooltip","class","alt"}
        #[arg(long)]
        waybar: bool,
    },
    /// Start a meeting (launches miniti if it is not running)
    Start {
        /// Meeting title (otherwise miniti names it from the conversation)
        title: Option<String>,
    },
    /// Stop the meeting and save it
    Stop,
    /// Start if idle, stop if recording
    Toggle,
    /// Raise the main window
    Show,
    /// Raise the main window on a saved meeting
    Open {
        /// Meeting id, or `last` for the most recent one
        #[arg(default_value = "last")]
        id: String,
    },
    /// Questions worth asking in the live meeting
    Questions,
    /// Recent meetings
    Meetings {
        /// How many to list
        #[arg(short = 'n', long, default_value_t = 20)]
        limit: i64,
    },
    /// Markdown export of a meeting (transcript, notes, insights, coaching)
    Export {
        /// Meeting id, or `last` for the most recent one
        #[arg(default_value = "last")]
        id: String,
        /// Write to this file instead of stdout
        #[arg(short, long, value_name = "FILE")]
        output: Option<PathBuf>,
    },
    /// Answer the pending Smart-meeting prompt shown in the tray and surface
    Decide {
        #[arg(value_enum)]
        choice: Choice,
    },
    /// Stream state changes as JSON lines until interrupted
    Watch,
    /// Quit the running app
    Quit,
    /// Launch miniti at login (writes ~/.config/autostart/miniti.desktop)
    Autostart {
        #[arg(value_enum)]
        state: Option<OnOff>,
    },
    /// Where miniti keeps its files on this machine
    Paths,
    /// Forget this computer's account credentials and device identity (offline)
    ///
    /// For a computer the backend still lists as enrolled while its
    /// credentials are gone. Afterwards miniti starts on the enrollment
    /// screen as a new device; restore there with a recovery key if you have
    /// one. The old device record stays on its account until removed there.
    ResetDevice {
        /// Do it (without this flag the command only explains itself)
        #[arg(long)]
        yes: bool,
    },
    /// Shell completion script for bash, zsh, fish or elvish
    Completions {
        #[arg(value_enum)]
        shell: clap_complete::Shell,
    },
}

#[derive(ValueEnum, Clone, Copy, Debug)]
pub enum Choice {
    Primary,
    Secondary,
    Tertiary,
}

#[derive(ValueEnum, Clone, Copy, Debug)]
pub enum OnOff {
    On,
    Off,
}

/// Only `scheme://…` values are links; a stray word is a typo'd command.
fn parse_link(raw: &str) -> Result<String, String> {
    if raw.contains("://") {
        Ok(raw.to_string())
    } else {
        Err(format!("'{raw}' is not a command or a link (see --help)"))
    }
}

/// What the process should do after argument handling.
pub struct Launch {
    pub hidden: bool,
    pub start_meeting: bool,
    pub title: Option<String>,
    pub urls: Vec<String>,
}

/// Parse argv and run any subcommand. `Ok` means "start the desktop app";
/// `Err(code)` means the CLI finished and the process should exit with `code`.
pub fn run() -> Result<Launch, i32> {
    let cli = Cli::parse();
    let Some(command) = cli.command else {
        return Ok(Launch {
            hidden: cli.hidden,
            start_meeting: cli.start_meeting,
            title: cli.title.filter(|t| !t.trim().is_empty()),
            urls: cli.urls,
        });
    };
    let out = Output { json: cli.json };
    Err(match execute(command, &out) {
        Ok(()) => 0,
        Err(code) => code,
    })
}

struct Output {
    json: bool,
}

impl Output {
    fn line(&self, s: impl AsRef<str>) {
        let mut stdout = std::io::stdout().lock();
        let _ = writeln!(stdout, "{}", s.as_ref());
    }
    fn value(&self, v: &Value) {
        self.line(serde_json::to_string_pretty(v).unwrap_or_default());
    }
    fn error(&self, msg: &str) {
        if self.json {
            self.line(json!({ "ok": false, "error": msg }).to_string());
        } else {
            eprintln!("miniti: {msg}");
        }
    }
}

fn not_running(out: &Output) -> i32 {
    if out.json {
        out.line(json!({ "ok": false, "running": false, "error": "miniti is not running" }).to_string());
    } else {
        eprintln!("miniti is not running (start it with `miniti`)");
    }
    EXIT_NOT_RUNNING
}

fn connect(out: &Output) -> Result<Client, i32> {
    client::connect().map_err(|_| not_running(out))
}

fn call(client: &mut Client, req: Request, out: &Output) -> Result<Value, i32> {
    match client.call(&req) {
        Ok(Response { ok: true, data, .. }) => Ok(data),
        Ok(Response { error, .. }) => {
            out.error(error.as_deref().unwrap_or("request failed"));
            Err(EXIT_FAILED)
        }
        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock || e.kind() == std::io::ErrorKind::TimedOut => {
            out.error("miniti did not answer within two minutes (the log in `miniti paths` says what it was doing)");
            Err(EXIT_FAILED)
        }
        Err(e) => {
            out.error(&format!("lost the connection to miniti: {e}"));
            Err(EXIT_FAILED)
        }
    }
}

fn execute(command: Command, out: &Output) -> Result<(), i32> {
    match command {
        Command::Completions { shell } => {
            clap_complete::generate(shell, &mut Cli::command(), "miniti", &mut std::io::stdout());
            Ok(())
        }
        Command::ResetDevice { yes } => {
            if client::is_running() {
                out.error("quit miniti first (miniti quit)");
                return Err(EXIT_FAILED);
            }
            if !yes {
                out.line("This forgets the account credentials and device id stored on this computer.\nRun again with --yes to do it; then start miniti and restore or create a recovery key.");
                return Ok(());
            }
            let device = crate::device_id::get_or_create().unwrap_or_default();
            let auth = crate::auth::manager::AuthManager::load(crate::api::DEFAULT_BASE_URL, device, crate::state::APP_VERSION);
            auth.clear_local();
            let id = crate::device_id::regenerate().map_err(|e| {
                out.error(&format!("could not write a new device id: {e}"));
                EXIT_FAILED
            })?;
            if out.json {
                out.value(&json!({ "device_id": id }));
            } else {
                out.line(format!("credentials cleared; new device id {id}. Start miniti to enroll again."));
            }
            Ok(())
        }
        Command::Paths => {
            let v = paths();
            if out.json {
                out.value(&v);
            } else if let Some(o) = v.as_object() {
                let width = o.keys().map(String::len).max().unwrap_or(0);
                for (k, val) in o {
                    out.line(format!("{k:<width$}  {}", val.as_str().unwrap_or_default()));
                }
            }
            Ok(())
        }
        Command::Autostart { state } => {
            if let Some(s) = state {
                let enabled = matches!(s, OnOff::On);
                crate::shell::set_autostart(enabled).map_err(|e| {
                    out.error(&format!("autostart: {e}"));
                    EXIT_FAILED
                })?;
                // Settings shows the preference, so keep it in step (the
                // running app re-reads prefs on its next save, if any).
                let mut prefs = crate::prefs::Prefs::load();
                if prefs.launch_at_login != enabled {
                    prefs.launch_at_login = enabled;
                    if let Err(e) = prefs.save() {
                        out.error(&format!("autostart: preference not saved: {e}"));
                    }
                }
            }
            let enabled = crate::shell::autostart_enabled();
            if out.json {
                out.value(&json!({ "enabled": enabled, "path": crate::shell::autostart_path() }));
            } else {
                out.line(if enabled { "autostart: on" } else { "autostart: off" });
            }
            Ok(())
        }
        Command::Status { waybar } => status(out, waybar),
        Command::Start { title } => {
            if !client::is_running() {
                if out.json {
                    out.line(json!({ "ok": true, "launched": true }).to_string());
                } else {
                    out.line("miniti is not running; starting it with a new meeting");
                }
                let mut args = vec!["--start-meeting".to_string()];
                if let Some(t) = title {
                    args.push(format!("--title={t}"));
                }
                return spawn_detached(&mut args).map_err(|e| {
                    out.error(&format!("could not launch miniti: {e}"));
                    EXIT_FAILED
                });
            }
            let mut c = connect(out)?;
            let data = call(&mut c, Request::Start { title }, out)?;
            if out.json {
                out.value(&data);
            } else {
                out.line(format!("recording started ({})", data["meeting_id"].as_str().unwrap_or("?")));
            }
            Ok(())
        }
        Command::Stop => {
            let mut c = connect(out)?;
            let data = call(&mut c, Request::Stop, out)?;
            if out.json {
                out.value(&data);
            } else {
                match data["meeting_id"].as_str() {
                    Some(id) => out.line(format!("recording stopped ({id})")),
                    None => out.line("nothing was recording"),
                }
            }
            Ok(())
        }
        Command::Toggle => {
            if !client::is_running() {
                if out.json {
                    out.line(json!({ "ok": true, "launched": true, "recording": true }).to_string());
                } else {
                    out.line("miniti is not running; starting it with a new meeting");
                }
                return spawn_detached(&mut vec!["--start-meeting".to_string()]).map_err(|e| {
                    out.error(&format!("could not launch miniti: {e}"));
                    EXIT_FAILED
                });
            }
            let mut c = connect(out)?;
            let data = call(&mut c, Request::Toggle, out)?;
            if out.json {
                out.value(&data);
            } else if data["recording"].as_bool().unwrap_or(false) {
                out.line("recording started");
            } else {
                out.line("recording stopped");
            }
            Ok(())
        }
        Command::Show => {
            let mut c = connect(out)?;
            call(&mut c, Request::Show, out)?;
            Ok(())
        }
        Command::Open { id } => {
            let mut c = connect(out)?;
            let data = call(&mut c, Request::OpenMeeting { id }, out)?;
            if out.json {
                out.value(&data);
            }
            Ok(())
        }
        Command::Questions => {
            let mut c = connect(out)?;
            let data = call(&mut c, Request::Questions, out)?;
            if out.json {
                out.value(&data);
                return Ok(());
            }
            let qs = data["questions"].as_array().cloned().unwrap_or_default();
            if !data["recording"].as_bool().unwrap_or(false) {
                out.line("not recording");
            } else if qs.is_empty() {
                out.line("no questions yet");
            } else {
                for (i, q) in qs.iter().enumerate() {
                    out.line(format_question(i + 1, q));
                }
            }
            Ok(())
        }
        Command::Meetings { limit } => {
            let mut c = connect(out)?;
            let data = call(&mut c, Request::Meetings { limit: Some(limit) }, out)?;
            if out.json {
                out.value(&data);
                return Ok(());
            }
            let rows = data.as_array().cloned().unwrap_or_default();
            if rows.is_empty() {
                out.line("no meetings yet");
            }
            for m in rows {
                out.line(format_meeting_row(&m));
            }
            Ok(())
        }
        Command::Export { id, output } => {
            let mut c = connect(out)?;
            let data = call(&mut c, Request::Export { id }, out)?;
            let markdown = data["markdown"].as_str().unwrap_or_default();
            match output {
                Some(path) => {
                    std::fs::write(&path, markdown).map_err(|e| {
                        out.error(&format!("write {}: {e}", path.display()));
                        EXIT_FAILED
                    })?;
                    if out.json {
                        out.value(&json!({ "meeting_id": data["meeting_id"], "path": path }));
                    } else {
                        out.line(format!("wrote {}", path.display()));
                    }
                }
                None if out.json => out.value(&data),
                None => out.line(markdown),
            }
            Ok(())
        }
        Command::Decide { choice } => {
            let mut c = connect(out)?;
            let choice = match choice {
                Choice::Primary => "primary",
                Choice::Secondary => "secondary",
                Choice::Tertiary => "tertiary",
            };
            call(&mut c, Request::Decide { choice: choice.into() }, out)?;
            Ok(())
        }
        Command::Watch => {
            let c = connect(out)?;
            let json = out.json;
            c.subscribe(|line| {
                // A closed pipe (`miniti watch | head -1`) ends the stream
                // instead of panicking in println!.
                let mut stdout = std::io::stdout().lock();
                let written = if json {
                    writeln!(stdout, "{line}")
                } else {
                    let v: Value = serde_json::from_str(line).unwrap_or(Value::Null);
                    writeln!(stdout, "{}", status_line(&v))
                };
                written.and_then(|_| stdout.flush()).is_ok()
            })
            .map_err(|e| {
                out.error(&format!("stream ended: {e}"));
                EXIT_FAILED
            })
        }
        Command::Quit => {
            let mut c = connect(out)?;
            call(&mut c, Request::Quit, out)?;
            Ok(())
        }
    }
}

fn status(out: &Output, waybar: bool) -> Result<(), i32> {
    let snapshot = client::connect()
        .ok()
        .and_then(|mut c| c.call(&Request::Status).ok())
        .filter(|r| r.ok)
        .map(|r| r.data);
    if waybar {
        out.line(waybar_json(snapshot.as_ref()).to_string());
        return Ok(());
    }
    let Some(v) = snapshot else {
        return Err(not_running(out));
    };
    if out.json {
        out.value(&v);
    } else {
        out.line(status_line(&v));
        if v["is_recording"].as_bool().unwrap_or(false) {
            let mut facts = vec![
                v["transcription_status"].as_str().unwrap_or_default().to_lowercase(),
                v["audio_status"].as_str().unwrap_or_default().to_lowercase(),
            ];
            if let Some(l) = v["lifecycle_status"].as_str() {
                facts.push(l.to_lowercase());
            }
            out.line(format!("  {}", facts.join(" · ")));
            let n = v["questions"].as_array().map(Vec::len).unwrap_or(0);
            if n > 0 {
                out.line(format!("  questions: {n} (miniti questions)"));
            }
        }
        if let Some(p) = v["prompt"].as_object() {
            out.line(format!(
                "  prompt: {} — {} / {}  (miniti decide primary|secondary)",
                p.get("message").and_then(Value::as_str).unwrap_or_default(),
                p.get("primary").and_then(Value::as_str).unwrap_or_default(),
                p.get("secondary").and_then(Value::as_str).unwrap_or_default(),
            ));
        }
    }
    Ok(())
}

/// One-line human summary of a snapshot.
pub fn status_line(v: &Value) -> String {
    if !v["is_recording"].as_bool().unwrap_or(false) {
        return "○ not recording".to_string();
    }
    let elapsed = v["elapsed_text"].as_str().unwrap_or("00:00");
    let title = v["meeting_title"].as_str().unwrap_or_default();
    if title.is_empty() {
        format!("● recording {elapsed}")
    } else {
        format!("● recording {elapsed} — {title}")
    }
}

/// Waybar custom module output. `class` drives CSS: recording | idle | stopped.
pub fn waybar_json(snapshot: Option<&Value>) -> Value {
    let Some(v) = snapshot else {
        return json!({ "text": "", "alt": "stopped", "class": "stopped", "tooltip": "miniti is not running" });
    };
    if !v["is_recording"].as_bool().unwrap_or(false) {
        let tip = match v["prompt"].as_object() {
            Some(p) => format!(
                "miniti — {}",
                p.get("message").and_then(Value::as_str).unwrap_or("not recording")
            ),
            None => "miniti — not recording".to_string(),
        };
        return json!({ "text": "", "alt": "idle", "class": "idle", "tooltip": tip });
    }
    let elapsed = v["elapsed_text"].as_str().unwrap_or("00:00");
    let title = v["meeting_title"].as_str().unwrap_or_default();
    let mut tip = vec![if title.is_empty() {
        format!("miniti — recording {elapsed}")
    } else {
        format!("miniti — recording {elapsed}\n{title}")
    }];
    tip.push(v["transcription_status"].as_str().unwrap_or_default().to_string());
    tip.push(v["audio_status"].as_str().unwrap_or_default().to_string());
    if let Some(l) = v["lifecycle_status"].as_str() {
        tip.push(l.to_string());
    }
    let n = v["questions"].as_array().map(Vec::len).unwrap_or(0);
    if n > 0 {
        tip.push(format!("{n} question{}", if n == 1 { "" } else { "s" }));
    }
    let class = match v["stream_state"].as_str() {
        Some("reconnecting") | Some("failed") => "recording degraded",
        _ => "recording",
    };
    json!({
        "text": format!("● {elapsed}"),
        "alt": "recording",
        "class": class,
        "tooltip": tip.join("\n"),
        "percentage": n,
    })
}

fn format_question(n: usize, q: &Value) -> String {
    let text = q["question"].as_str().unwrap_or_default();
    let context = q["context"].as_str().unwrap_or_default();
    if context.is_empty() {
        format!("{n}. {text}")
    } else {
        format!("{n}. {text}\n   {context}")
    }
}

fn format_meeting_row(m: &Value) -> String {
    let when = m["started_at"]
        .as_i64()
        .and_then(|t| chrono::DateTime::from_timestamp(t, 0))
        .map(|t| t.with_timezone(&chrono::Local).format("%Y-%m-%d %H:%M").to_string())
        .unwrap_or_else(|| "                ".to_string());
    let secs = m["duration_seconds"].as_i64().unwrap_or(0).max(0);
    let dur = format!("{:>3}:{:02}", secs / 60, secs % 60);
    let title = m["title"].as_str().unwrap_or("Untitled meeting");
    let id = m["id"].as_str().unwrap_or_default();
    let pin = if m["pinned"].as_bool().unwrap_or(false) { "★" } else { " " };
    format!("{when}  {dur}  {pin} {title}  [{id}]")
}

fn paths() -> Value {
    json!({
        "config": crate::prefs::prefs_path().parent().map(|p| p.display().to_string()).unwrap_or_default(),
        "data": crate::device_id::data_dir().display().to_string(),
        "logs": crate::log_dir().display().to_string(),
        "runtime": crate::ipc::runtime_dir().display().to_string(),
        "socket": crate::ipc::socket_path().display().to_string(),
        "state": crate::ipc::state_path().display().to_string(),
        "dbus": format!("{} {}", crate::ipc::BUS_NAME, crate::ipc::OBJECT_PATH),
        "autostart": crate::shell::autostart_path().display().to_string(),
    })
}

/// Launch the desktop app in its own session so it outlives this shell.
fn spawn_detached(args: &mut Vec<String>) -> std::io::Result<()> {
    use std::os::unix::process::CommandExt;
    let exe = std::env::current_exe()?;
    let mut cmd = std::process::Command::new(exe);
    cmd.args(args.drain(..))
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    // SAFETY: setsid only detaches the child from the controlling terminal.
    unsafe {
        cmd.pre_exec(|| {
            libc::setsid();
            Ok(())
        });
    }
    cmd.spawn().map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_launch_and_urls_are_not_subcommands() {
        let cli = Cli::try_parse_from(["miniti"]).unwrap();
        assert!(cli.command.is_none() && !cli.hidden && cli.urls.is_empty());
        let cli = Cli::try_parse_from(["miniti", "--hidden"]).unwrap();
        assert!(cli.hidden);
        let cli = Cli::try_parse_from(["miniti", "--start-meeting", "--title=Weekly sync"]).unwrap();
        assert!(cli.start_meeting && cli.title.as_deref() == Some("Weekly sync"));
        assert!(Cli::try_parse_from(["miniti", "--title=x"]).is_err(), "title needs --start-meeting");
        let cli =
            Cli::try_parse_from(["miniti", "miniti-google://oauth-callback?status=success"]).unwrap();
        assert_eq!(cli.urls, vec!["miniti-google://oauth-callback?status=success"]);
        assert!(cli.command.is_none());
    }

    #[test]
    fn subcommands_parse_with_global_json() {
        let cli = Cli::try_parse_from(["miniti", "status", "--json"]).unwrap();
        assert!(cli.json);
        assert!(matches!(cli.command, Some(Command::Status { waybar: false })));
        let cli = Cli::try_parse_from(["miniti", "--json", "meetings", "-n", "5"]).unwrap();
        assert!(matches!(cli.command, Some(Command::Meetings { limit: 5 })));
        let cli = Cli::try_parse_from(["miniti", "export"]).unwrap();
        assert!(matches!(cli.command, Some(Command::Export { ref id, output: None }) if id == "last"));
        let cli = Cli::try_parse_from(["miniti", "decide", "primary"]).unwrap();
        assert!(matches!(cli.command, Some(Command::Decide { choice: Choice::Primary })));
        assert!(Cli::try_parse_from(["miniti", "decide", "maybe"]).is_err());
        assert!(Cli::try_parse_from(["miniti", "explode"]).is_err(), "unknown words are not URLs");
    }

    #[test]
    fn waybar_output_has_the_module_fields() {
        let v = waybar_json(None);
        assert_eq!(v["class"], "stopped");
        assert_eq!(v["text"], "");
        let idle = json!({ "is_recording": false });
        assert_eq!(waybar_json(Some(&idle))["class"], "idle");
        let rec = json!({
            "is_recording": true, "elapsed_text": "12:34", "meeting_title": "Weekly sync",
            "transcription_status": "Transcription: live", "audio_status": "Audio: healthy",
            "lifecycle_status": "Zoom call active", "stream_state": "connected",
            "questions": [{"question": "a"}, {"question": "b"}],
        });
        let v = waybar_json(Some(&rec));
        assert_eq!(v["text"], "● 12:34");
        assert_eq!(v["class"], "recording");
        assert_eq!(v["percentage"], 2);
        let tip = v["tooltip"].as_str().unwrap();
        assert!(tip.contains("Weekly sync") && tip.contains("2 questions") && tip.contains("Zoom"));
        let mut degraded = rec.clone();
        degraded["stream_state"] = json!("reconnecting");
        assert_eq!(waybar_json(Some(&degraded))["class"], "recording degraded");
    }

    #[test]
    fn status_line_reads_like_the_tray() {
        assert_eq!(status_line(&json!({ "is_recording": false })), "○ not recording");
        assert_eq!(
            status_line(&json!({ "is_recording": true, "elapsed_text": "1:02:03", "meeting_title": "Sync" })),
            "● recording 1:02:03 — Sync"
        );
    }

    #[test]
    fn meeting_rows_are_aligned() {
        let row = format_meeting_row(&json!({
            "id": "abc", "title": "Sync", "started_at": 0, "duration_seconds": 754, "pinned": true
        }));
        assert!(row.contains(" 12:34  ★ Sync  [abc]"), "{row}");
        assert!(format_question(1, &json!({ "question": "Why?", "context": "budget" })).contains("\n   budget"));
    }
}
