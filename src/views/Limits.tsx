import { useEffect, useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { errorMessage, subscribeUrl } from "../api";
import type { Prefs, Usage } from "../types";

/** Managed-mode remaining minutes; hidden for BYOK (macOS `UsageBanner`). */
export function UsageBanner({ prefs, usage, error, onUpgrade }: { prefs: Prefs | null; usage: Usage | null; error?: string | null; onUpgrade: () => void }) {
  if (!prefs || prefs.app_mode !== "managed") return null;
  if (!usage && error) return <div className="banner error">plan unavailable: {error}</div>;
  if (!usage) return <span className="pill">checking plan…</span>;
  const pro = usage.tier === "pro";
  const limit = usage.minutes_limit;
  const remaining = limit == null ? Infinity : Math.max(0, limit - usage.minutes_used);
  const low = remaining < 60;
  const critical = remaining < 15;
  const pct = limit == null || limit === 0 ? 0 : Math.min(100, (usage.minutes_used / limit) * 100);
  return (
    <div className={`usage-banner ${critical ? "critical" : low ? "low" : ""}`}>
      <div className="usage-row">
        <span className={`pill ${pro ? "pro" : ""}`}>{pro ? "miniti pro" : "miniti free"}</span>
        <span className="usage-text">
          {Math.round(usage.minutes_used)}m used{limit != null && <> · <strong>{Math.floor(remaining)}m left</strong></>}
        </span>
        {!pro && <button className="ghost upgrade" onClick={onUpgrade}>upgrade</button>}
      </div>
      {limit != null && (
        <div className="usage-bar" role="progressbar" aria-valuenow={Math.round(pct)} aria-valuemin={0} aria-valuemax={100}>
          <div className="usage-fill" style={{ width: `${pct}%` }} />
        </div>
      )}
      {usage.resets_at && !pro && <div className="muted tiny">resets {new Date(usage.resets_at).toLocaleDateString()}</div>}
    </div>
  );
}

function daysUntil(iso: string | null): number | null {
  if (!iso) return null;
  const ms = new Date(iso).getTime() - Date.now();
  return Number.isFinite(ms) ? Math.max(0, Math.ceil(ms / 86_400_000)) : null;
}

/** The month's managed minutes are gone: upgrade or switch to BYOK (macOS `LimitReachedView`). */
export function LimitReached({ usage, resetsAt, onSwitchToByok }: { usage: Usage | null; resetsAt: string | null; onSwitchToByok: () => void }) {
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const days = daysUntil(resetsAt ?? usage?.resets_at ?? null);
  const limit = usage?.minutes_limit ?? 500;
  async function upgrade() {
    setBusy(true);
    try { await openUrl(await subscribeUrl()); } catch (e) { setError(errorMessage(e)); } finally { setBusy(false); }
  }
  return (
    <div className="limit-reached">
      <span className="hex">⬢</span>
      <div className="strong">limit reached</div>
      <p className="muted">You've used all {limit} minutes this month.</p>
      {days != null && (
        <p className="muted small">resets in {days} day{days === 1 ? "" : "s"}{resetsAt || usage?.resets_at ? ` · ${new Date((resetsAt ?? usage?.resets_at)!).toLocaleDateString()}` : ""}</p>
      )}
      <button className="btn primary" onClick={upgrade} disabled={busy}>upgrade to pro — 5,000 min/mo</button>
      <p className="muted tiny">$5/month, billed through Polar</p>
      <p className="muted small">or use your own API keys for unlimited access</p>
      <button className="btn" onClick={onSwitchToByok}>switch to BYOK</button>
      <p className="muted tiny">You'll need Deepgram &amp; OpenAI API keys</p>
      {error && <div className="banner error">{error}</div>}
    </div>
  );
}

/** Parse the structured prefix Rust puts on start errors ("limit_reached:<resets_at>:<message>"). */
export function parseStartError(message: string | null): { kind: "limit_reached" | "device_disabled" | "not_enrolled"; resetsAt: string | null; message: string } | null {
  if (!message) return null;
  const m = message.match(/^(limit_reached|device_disabled|not_enrolled):([^:]*):(.*)$/s);
  if (!m) return null;
  return { kind: m[1] as "limit_reached" | "device_disabled" | "not_enrolled", resetsAt: m[2] || null, message: m[3] };
}

/** Keep a value while the component that owns it is mounted (avoids flashing when usage reloads). */
export function useSticky<T>(value: T | null): T | null {
  const [v, setV] = useState<T | null>(value);
  useEffect(() => { if (value != null) setV(value); }, [value]);
  return v;
}
