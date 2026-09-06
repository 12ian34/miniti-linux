import { useEffect, useState } from "react";
import { environmentHealth, getUsage, hasBridge } from "../api";
import { useStore } from "../store";
import type { EnvHealth, LaunchGate, Usage } from "../types";

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
        <p className="muted">Connect Google Calendar in Settings to see your next meetings here.</p>
      </section>
    </div>
  );
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
