import { useEffect, useState } from "react";
import {
  calendarEvents,
  environmentHealth,
  errorMessage,
  getPrepNotes,
  getUsage,
  hasBridge,
  onCalendarUpdated,
  onDeepLink,
  setPrepNotes,
  startMeetingFromEvent,
  subscribeUrl,
  setPrefs,
} from "../api";
import { useStore } from "../store";
import { openUrl } from "@tauri-apps/plugin-opener";
import { useTauriEvent } from "../useEvent";
import { LimitReached, UsageBanner, parseStartError } from "./Limits";
import { isNewerVersion } from "../format";
import { Sheet } from "./MeetingTools";
import type { CalendarEvent, CalendarView, EnvHealth, LaunchGate, Prefs, Usage } from "../types";

/** Port of the macOS `ReadyStateView`: centred lockup, tagline, start action, calendar. */
export function Home({ gate }: { gate: LaunchGate | null }) {
  const store = useStore();
  const { prefs, recording, start, starting, lastError, clearError, navigate } = store;
  const [health, setHealth] = useState<EnvHealth | null>(null);
  const [usage, setUsage] = useState<Usage | null>(null);
  const [usageError, setUsageError] = useState<string | null>(null);
  const [calendar, setCalendar] = useState<CalendarView | null>(null);
  const [prepEvent, setPrepEvent] = useState<CalendarEvent | null>(null);
  const [prepNotes, setPrepNotesState] = useState("");
  const [shortcuts, setShortcuts] = useState(false);
  const [nudgeDismissed, setNudgeDismissed] = useState(() => {
    try { return localStorage.getItem("ui.calendarNudge") === "0"; } catch { return false; }
  });

  useEffect(() => {
    if (!hasBridge) return;
    environmentHealth().then(setHealth).catch(() => setHealth(null));
  }, []);
  useEffect(() => {
    if (!hasBridge || !prefs || prefs.app_mode !== "managed" || !health?.enrolled) return;
    getUsage().then((u) => { setUsage(u); setUsageError(null); }).catch((e) => setUsageError(String(e)));
  }, [prefs, health]);

  const loadCalendar = (refresh = false) => calendarEvents(refresh).then(setCalendar).catch(() => {});
  useEffect(() => { if (hasBridge) void loadCalendar(false); }, []);
  useTauriEvent(onCalendarUpdated, () => void loadCalendar(false));
  useTauriEvent(onDeepLink, (ev) => { if (ev.scheme === "miniti-google") void loadCalendar(true); });

  async function openPrep(e: CalendarEvent) {
    setPrepEvent(e);
    setPrepNotesState(await getPrepNotes(e.id).catch(() => ""));
  }
  async function startFromEvent(e: CalendarEvent) {
    try {
      await setPrepNotes(e.id, prepNotes);
      const id = await startMeetingFromEvent(e.id);
      setPrepEvent(null);
      navigate({ kind: "meeting", id });
    } catch (err) {
      setUsageError(errorMessage(err));
    }
  }

  const startError = parseStartError(lastError);
  const limitReached = startError?.kind === "limit_reached" || (usage?.minutes_limit != null && usage.minutes_used >= usage.minutes_limit);
  const deviceDisabled = startError?.kind === "device_disabled" || (usageError?.toLowerCase().includes("disabled") ?? false);
  const byokMissingKey = prefs?.app_mode === "byok" && !prefs.byok_deepgram_key;
  const showNudge = calendar?.available && !calendar.connected && !nudgeDismissed;

  return (
    <div className="ready">
      <div className="ready-actions">
        <button className="home-action" title="Keyboard shortcuts" onClick={() => setShortcuts(true)}>⌘ shortcuts</button>
        <button className="home-action" title="Settings (Ctrl+,)" onClick={() => navigate({ kind: "settings" })}>⚙ settings</button>
      </div>

      <div className="ready-center">
        <div className="lockup">
          <span className="hex big">⬢</span>
          <span className="wordmark">miniti</span>
          <span className="tagline">multi-dimensional<span className="cursor">_</span> meetings</span>
        </div>

        {prefs?.app_mode === "managed" && health?.enrolled ? (
          <UsageBanner prefs={prefs} usage={usage} onUpgrade={() => subscribeUrl().then(openUrl).catch(() => navigate({ kind: "settings" }))} />
        ) : (
          <StatusPills prefs={prefs} usage={usage} backendKey={health?.enrolled ?? null} />
        )}

        {lastError && !startError && <div className="banner error narrow">{lastError}<button className="ghost" onClick={clearError}>×</button></div>}
        {gate && !gate.backend_reachable && prefs?.app_mode === "managed" && (
          <div className="banner warn narrow">backend not reachable: {gate.backend_error ?? "unknown error"}</div>
        )}
        {gate && <UpdateNotice gate={gate} />}

        {deviceDisabled ? (
          <div className="stack-center">
            <div className="strong">account disabled</div>
            <div className="muted small">contact support for help</div>
          </div>
        ) : limitReached ? (
          <LimitReached
            usage={usage}
            resetsAt={startError?.resetsAt ?? null}
            onSwitchToByok={async () => { if (prefs) { await setPrefs({ ...prefs, app_mode: "byok" }); clearError(); } navigate({ kind: "settings" }); }}
          />
        ) : byokMissingKey ? (
          <div className="stack-center">
            <div className="strong">add your Deepgram key</div>
            <button className="control" onClick={() => navigate({ kind: "settings" })}>⚿ open settings</button>
          </div>
        ) : recording.recording && recording.meeting_id ? (
          <button className="start-btn" onClick={() => navigate({ kind: "meeting", id: recording.meeting_id! })}>
            <span className="dot dot-rec pulse" /> recording in progress
          </button>
        ) : (
          <button className="start-btn" onClick={() => start().catch(() => {})} disabled={starting || !hasBridge}>
            {starting ? "starting…" : <><span className="rec-glyph">◉</span> start meeting <span className="kbd light">Ctrl+R</span></>}
          </button>
        )}

        <div className="source-summary muted small">
          {health ? (
            <>
              <span className={`dot ${health.microphone_available ? "dot-ok" : "dot-warn"}`} /> microphone
              {prefs?.capture_system_audio && <><span className="sep">•</span><span className={`dot ${health.system_audio_available ? "dot-ok" : "dot-warn"}`} /> system audio</>}
              <span className="sep">•</span>{prefs?.language ?? "en"}
            </>
          ) : "…"}
        </div>

        {showNudge && (
          <div className="calendar-nudge">
            <div className="strong small">connect google calendar</div>
            <div className="muted small">see upcoming meetings, prep notes, and auto-fill titles and attendees.</div>
            <div className="nudge-actions">
              <button className="control primary" onClick={() => navigate({ kind: "settings" })}>connect</button>
              <button className="ghost" onClick={() => { setNudgeDismissed(true); try { localStorage.setItem("ui.calendarNudge", "0"); } catch { /* ignore */ } }}>not now</button>
            </div>
          </div>
        )}

        {calendar?.connected && calendar.upcoming.length > 0 && (
          <div className="upcoming">
            <div className="upcoming-label muted tiny">upcoming</div>
            {calendar.upcoming.map((e) => (
              <button className="event-row" key={e.id} onClick={() => openPrep(e)}>
                <span className="dot dot-info" />
                <span className="event-when">{fmtEventWhen(e)}</span>
                <span className="ellipsis">{e.title || "(no title)"}</span>
                {e.attendees.length > 0 && <span className="muted tiny">👥 {e.attendees.filter((a) => !a.self).length}</span>}
              </button>
            ))}
          </div>
        )}
      </div>

      {prepEvent && (
        <Sheet title="prepare meeting" onClose={() => setPrepEvent(null)}>
          <h2 className="card-title">{prepEvent.title || "(no title)"}</h2>
          <p className="muted small">
            {fmtEventWhen(prepEvent)}
            {prepEvent.attendees.length > 0 && ` · ${prepEvent.attendees.map((a) => a.displayName || a.email).join(", ")}`}
          </p>
          {(prepEvent.meetLink || prepEvent.conferenceUrl) && <p className="muted small">{prepEvent.meetLink || prepEvent.conferenceUrl}</p>}
          <textarea className="notes-area short" placeholder="Private notes for this meeting — they seed the live notes when you start." value={prepNotes} onChange={(e) => setPrepNotesState(e.currentTarget.value)} />
          <div className="save-row">
            <button className="control positive" onClick={() => startFromEvent(prepEvent)} disabled={starting}>◉ start this meeting</button>
            <button className="control" onClick={async () => { await setPrepNotes(prepEvent.id, prepNotes); setPrepEvent(null); }}>save notes</button>
          </div>
        </Sheet>
      )}

      {shortcuts && (
        <Sheet title="keyboard shortcuts" onClose={() => setShortcuts(false)}>
          <table className="table kbd-table"><tbody>
            {[
              ["Ctrl+R", "start / stop recording"], ["Ctrl+N", "home"], ["Ctrl+,", "settings"],
              ["Ctrl+[", "toggle history sidebar"], ["Ctrl+]", "toggle insights"],
              ["/", "search meetings"], ["↑ K / ↓ J", "move through history"], ["Enter", "open meeting"],
              ["Ctrl+F", "search settings (in Settings)"],
            ].map(([k, v]) => <tr key={k}><td><span className="kbd">{k}</span></td><td>{v}</td></tr>)}
          </tbody></table>
        </Sheet>
      )}
    </div>
  );
}

function StatusPills({ prefs, usage, backendKey }: { prefs: Prefs | null; usage: Usage | null; backendKey: boolean | null }) {
  if (!prefs) return null;
  if (prefs.app_mode === "byok") {
    return (
      <div className="pills">
        <span className={`pill ${prefs.byok_deepgram_key ? "ok" : "warn"}`}>deepgram {prefs.byok_deepgram_key ? "✓" : "missing"}</span>
        <span className={`pill ${prefs.byok_openai_key ? "ok" : ""}`}>openai {prefs.byok_openai_key ? "✓" : "—"}</span>
      </div>
    );
  }
  if (backendKey === false) return <span className="pill warn">managed · not enrolled (Settings → Account)</span>;
  if (!usage) return <span className="pill">managed · free</span>;
  const limit = usage.minutes_limit ?? Infinity;
  const pct = limit === Infinity ? 0 : Math.min(100, (usage.minutes_used / limit) * 100);
  return (
    <div className="pills">
      <span className={`pill ${usage.tier === "pro" ? "pro" : ""}`}>{usage.tier === "pro" ? "pro" : "free"}</span>
      <span className={`pill ${pct >= 90 ? "warn" : ""}`}>{Math.round(usage.minutes_used)} / {usage.minutes_limit ?? "∞"} min</span>
    </div>
  );
}

function fmtEventWhen(e: CalendarEvent): string {
  const s = new Date(e.start);
  const today = new Date();
  const sameDay = s.toDateString() === today.toDateString();
  const day = sameDay ? "today" : s.toLocaleDateString(undefined, { weekday: "short", day: "numeric", month: "short" }).toLowerCase();
  return `${day} ${s.toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit" })}`;
}

const UPDATE_DISMISS_KEY = "update.dismissedVersion";

/**
 * Quiet once-per-release notice that a newer Linux build exists. Linux has no
 * Sparkle or App Store, so the backend reports the real latest version and the
 * app points at the README's install section; dismissing remembers the version.
 */
function UpdateNotice({ gate }: { gate: LaunchGate }) {
  const [dismissed, setDismissed] = useState<string | null>(() => {
    try { return localStorage.getItem(UPDATE_DISMISS_KEY); } catch { return null; }
  });
  const latest = gate.latest_version;
  if (!latest || !isNewerVersion(latest, gate.current_version) || dismissed === latest) return null;
  const dismiss = () => {
    try { localStorage.setItem(UPDATE_DISMISS_KEY, latest); } catch { /* ignore */ }
    setDismissed(latest);
  };
  return (
    <div className="banner narrow update-notice">
      <span>miniti {latest} is available (you have {gate.current_version}).</span>
      <button className="ghost" onClick={() => openUrl("https://github.com/12ian34/miniti-linux#install")}>how to update</button>
      <button className="ghost" onClick={dismiss} title="Hide until the next release">dismiss</button>
    </div>
  );
}
