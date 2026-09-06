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
} from "../api";
import { useStore } from "../store";
import { useTauriEvent } from "../useEvent";
import { Sheet } from "./MeetingTools";
import type { CalendarEvent, CalendarView, EnvHealth, LaunchGate, Usage } from "../types";

interface HomeProps {
  gate: LaunchGate | null;
}

/** Port of the macOS `ReadyStateView`: mode-aware status, start action, audio sources. */
export function Home({ gate }: HomeProps) {
  const store = useStore();
  const { prefs, recording, start, starting, lastError, clearError, navigate } = store;
  const [health, setHealth] = useState<EnvHealth | null>(null);
  const [usage, setUsage] = useState<Usage | null>(null);
  const [usageError, setUsageError] = useState<string | null>(null);
  const [calendar, setCalendar] = useState<CalendarView | null>(null);
  const [prepEvent, setPrepEvent] = useState<CalendarEvent | null>(null);
  const [prepNotes, setPrepNotesState] = useState("");
  const [calendarNudgeDismissed, setCalendarNudgeDismissed] = useState(() => {
    try { return localStorage.getItem("ui.calendarNudge") === "0"; } catch { return false; }
  });

  const loadCalendar = (refresh = false) => calendarEvents(refresh).then(setCalendar).catch(() => {});
  useEffect(() => {
    if (!hasBridge) return;
    void loadCalendar(false);
  }, []);
  useTauriEvent(onCalendarUpdated, () => void loadCalendar(false));
  useTauriEvent(onDeepLink, (ev) => {
    if (ev.scheme === "miniti-google") void loadCalendar(true);
  });

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
      store.clearError();
      setUsageError(errorMessage(err));
    }
  }

  useEffect(() => {
    if (!hasBridge) return;
    environmentHealth().then(setHealth).catch(() => setHealth(null));
  }, []);

  useEffect(() => {
    if (!hasBridge || !prefs || prefs.app_mode !== "managed" || !health?.backend_key_present) return;
    getUsage()
      .then((u) => {
        setUsage(u);
        setUsageError(null);
      })
      .catch((e) => setUsageError(String(e)));
  }, [prefs, health]);

  const limitReached =
    usage?.minutes_limit != null && usage.minutes_used >= usage.minutes_limit;
  const deviceDisabled = usageError?.includes("disabled") ?? false;
  const byokMissingKey = prefs?.app_mode === "byok" && !prefs.byok_deepgram_key;

  if (recording.recording && recording.meeting_id) {
    return (
      <div className="ready">
        <h1 className="tagline">recording in progress</h1>
        <button className="btn positive" onClick={() => navigate({ kind: "meeting", id: recording.meeting_id! })}>
          Open meeting
        </button>
      </div>
    );
  }

  return (
    <div className="ready">
      <div className="ready-top">
        <StatusPill prefs={prefs} usage={usage} backendKey={health?.backend_key_present ?? null} />
        <button className="ghost" title="Settings (Ctrl+,)" onClick={() => navigate({ kind: "settings" })}>
          ⚙
        </button>
      </div>

      <h1 className="tagline">
        multi-dimensional<span className="cursor">_</span> meetings
      </h1>

      {lastError && (
        <div className="banner error">
          {lastError}
          <button className="ghost" onClick={clearError}>×</button>
        </div>
      )}
      {gate && !gate.backend_reachable && prefs?.app_mode === "managed" && (
        <div className="banner warn">Backend not reachable: {gate.backend_error ?? "unknown error"}.</div>
      )}
      {deviceDisabled ? (
        <div className="banner error">This device has been disabled. Contact support to restore access.</div>
      ) : limitReached ? (
        <div className="banner warn">
          Monthly managed minutes used up. Upgrade to Pro or switch to BYOK in Settings.
          <button className="ghost" onClick={() => navigate({ kind: "settings" })}>open settings</button>
        </div>
      ) : byokMissingKey ? (
        <div className="banner warn">
          BYOK mode needs a Deepgram API key.
          <button className="ghost" onClick={() => navigate({ kind: "settings" })}>add key</button>
        </div>
      ) : null}

      {!deviceDisabled && (
        <button
          className="btn positive large"
          onClick={() => start().catch(() => {})}
          disabled={starting || limitReached || !hasBridge}
        >
          {starting ? "starting…" : "● Start meeting"}
        </button>
      )}
      <p className="muted">Ctrl+R starts and stops. The history sidebar collapses while you record.</p>

      <section className="panel sources">
        <div className="panel-title">audio sources</div>
        <div className="rows">
          <Row
            k="microphone"
            v={health ? (health.microphone_available ? "ready" : "not detected") : "…"}
            ok={health?.microphone_available}
          />
          <Row
            k="system audio"
            v={
              health
                ? health.system_audio_available
                  ? prefs?.capture_system_audio
                    ? "ready · remote speakers on channel 2"
                    : "off"
                  : "not detected (needs pipewire-pulse)"
                : "…"
            }
            ok={health?.system_audio_available && prefs?.capture_system_audio}
          />
          <Row k="language" v={prefs?.language ?? "en"} />
          <Row k="mode" v={prefs?.app_mode === "byok" ? "BYOK" : "Managed"} />
        </div>
      </section>

      <section className="panel">
        <div className="panel-title">upcoming</div>
        {!calendar?.available ? (
          <p className="muted">Calendar needs managed mode (backend key) to connect Google.</p>
        ) : !calendar.connected ? (
          !calendarNudgeDismissed ? (
            <div className="banner warn">
              Connect Google Calendar to see upcoming meetings, prepare notes, and auto-fill titles and attendees.
              <button className="ghost" onClick={() => navigate({ kind: "settings" })}>connect</button>
              <button className="ghost" onClick={() => { setCalendarNudgeDismissed(true); try { localStorage.setItem("ui.calendarNudge", "0"); } catch { /* ignore */ } }}>×</button>
            </div>
          ) : (
            <p className="muted">Google Calendar not connected.</p>
          )
        ) : calendar.upcoming.length === 0 ? (
          <p className="muted">No upcoming events in the next week{calendar.error ? ` (${calendar.error})` : ""}.</p>
        ) : (
          <div className="events">
            {calendar.upcoming.map((e) => (
              <button className="event-row" key={e.id} onClick={() => openPrep(e)}>
                <span className="event-when">{fmtEventWhen(e)}</span>
                <span className="ellipsis">{e.title || "(no title)"}</span>
                {e.attendees.length > 0 && <span className="muted tiny">{e.attendees.length} attendee{e.attendees.length === 1 ? "" : "s"}</span>}
              </button>
            ))}
          </div>
        )}
      </section>

      {prepEvent && (
        <Sheet title="prepare meeting" onClose={() => setPrepEvent(null)}>
          <h2 className="card-title">{prepEvent.title || "(no title)"}</h2>
          <p className="muted small">
            {fmtEventWhen(prepEvent)}
            {prepEvent.attendees.length > 0 && ` · ${prepEvent.attendees.map((a) => a.displayName || a.email).join(", ")}`}
          </p>
          {(prepEvent.meetLink || prepEvent.conferenceUrl) && (
            <p className="muted small">{prepEvent.meetLink || prepEvent.conferenceUrl}</p>
          )}
          <textarea
            className="notes-area short"
            placeholder="Private notes for this meeting — they seed the live notes when you start."
            value={prepNotes}
            onChange={(e) => setPrepNotesState(e.currentTarget.value)}
          />
          <div className="save-row">
            <button className="btn positive" onClick={() => startFromEvent(prepEvent)} disabled={starting}>
              ● Start this meeting
            </button>
            <button className="btn secondary" onClick={async () => { await setPrepNotes(prepEvent.id, prepNotes); setPrepEvent(null); }}>
              Save notes
            </button>
          </div>
        </Sheet>
      )}
    </div>
  );
}

function fmtEventWhen(e: CalendarEvent): string {
  const s = new Date(e.start);
  const today = new Date();
  const sameDay = s.toDateString() === today.toDateString();
  const day = sameDay ? "Today" : s.toLocaleDateString(undefined, { weekday: "short", day: "numeric", month: "short" });
  return `${day} ${s.toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit" })}`;
}

function StatusPill({
  prefs,
  usage,
  backendKey,
}: {
  prefs: import("../types").Prefs | null;
  usage: Usage | null;
  backendKey: boolean | null;
}) {
  if (!prefs) return null;
  if (prefs.app_mode === "byok") {
    return (
      <div className="pills">
        <span className={`pill ${prefs.byok_deepgram_key ? "ok" : "warn"}`}>deepgram</span>
        <span className={`pill ${prefs.byok_openai_key ? "ok" : ""}`}>openai</span>
      </div>
    );
  }
  if (backendKey === false) return <span className="pill warn">managed unavailable in this build</span>;
  if (!usage) return <span className="pill">managed</span>;
  const limit = usage.minutes_limit ?? Infinity;
  const pct = limit === Infinity ? 0 : Math.min(100, (usage.minutes_used / limit) * 100);
  return (
    <div className="pills">
      <span className={`pill ${usage.tier === "pro" ? "pro" : ""}`}>{usage.tier ?? "free"}</span>
      <span className={`pill ${pct >= 90 ? "warn" : ""}`}>
        {Math.round(usage.minutes_used)} / {usage.minutes_limit ?? "∞"} min
      </span>
    </div>
  );
}

function Row({ k, v, ok }: { k: string; v: string; ok?: boolean | null }) {
  return (
    <div className="row-line">
      {ok != null && <span className={`dot ${ok ? "dot-ok" : "dot-warn"}`} />}
      <span className="k">{k}</span>
      <span className="v">{v}</span>
    </div>
  );
}
