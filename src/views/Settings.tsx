import { useEffect, useState, type ReactNode } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import {
  environmentHealth,
  errorMessage,
  getPrefs,
  getUsage,
  hasBridge,
  pickFolder,
  portalUrl,
  probeDocsMcp,
  restoreLicense,
  setPrefs,
  subscribeUrl,
} from "../api";
import type { AppMode, Prefs, Usage } from "../types";
import { useStore } from "../store";

const LANGUAGES = ["en", "es", "fr", "de", "pt", "it", "nl", "sv", "el", "pl", "ru"];

export function Settings() {
  const { back, refreshPrefs } = useStore();
  const [prefs, setPrefsState] = useState<Prefs | null>(null);
  const [saved, setSaved] = useState(false);
  const [backendKey, setBackendKey] = useState<boolean | null>(null);
  const [usage, setUsage] = useState<Usage | null>(null);
  const [usageError, setUsageError] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [licenseKey, setLicenseKey] = useState("");
  const [notice, setNotice] = useState<string | null>(null);

  useEffect(() => {
    if (!hasBridge) return;
    getPrefs().then(setPrefsState);
    environmentHealth().then((h) => setBackendKey(h.backend_key_present)).catch(() => {});
  }, []);

  useEffect(() => {
    if (!hasBridge || !prefs || prefs.app_mode !== "managed" || backendKey === false) return;
    getUsage()
      .then((u) => {
        setUsage(u);
        setUsageError(null);
      })
      .catch((e) => setUsageError(errorMessage(e)));
  }, [prefs?.app_mode, backendKey]);

  function update<K extends keyof Prefs>(key: K, value: Prefs[K]) {
    setPrefsState((p) => (p ? { ...p, [key]: value } : p));
    setSaved(false);
  }

  async function save() {
    if (!prefs) return;
    setError(null);
    try {
      await setPrefs(prefs);
      await refreshPrefs();
      setSaved(true);
    } catch (e) {
      setError(errorMessage(e));
    }
  }

  async function openExternal(fetchUrl: () => Promise<string>) {
    setError(null);
    try {
      await openUrl(await fetchUrl());
    } catch (e) {
      setError(errorMessage(e));
    }
  }

  async function restore() {
    setError(null);
    setNotice(null);
    try {
      await restoreLicense(licenseKey);
      setNotice("Pro restored on this device.");
      setLicenseKey("");
      setUsage(await getUsage());
    } catch (e) {
      setError(errorMessage(e));
    }
  }

  if (!prefs) {
    return (
      <div className="view">
        <h1 className="view-title">Settings</h1>
        <p className="muted">
          {hasBridge ? "Loading…" : "Browser preview — native bridge unavailable."}
        </p>
      </div>
    );
  }

  const managedUnavailable = backendKey === false;

  return (
    <div className="view">
      <div className="detail-head">
        <button className="ghost" onClick={back}>‹ back</button>
        <h1 className="view-title">Settings</h1>
      </div>
      {error && <div className="banner error">{error}</div>}
      {notice && <div className="banner ok">{notice}</div>}

      <Field label="Mode">
        <select
          className="input"
          value={prefs.app_mode}
          onChange={(e) => update("app_mode", e.currentTarget.value as AppMode)}
        >
          <option value="managed" disabled={managedUnavailable}>
            Managed (Miniti backend){managedUnavailable ? " — unavailable in this build" : ""}
          </option>
          <option value="byok">BYOK (your own keys)</option>
        </select>
        {managedUnavailable && (
          <p className="muted">
            This build was compiled without a backend key, so managed mode cannot start sessions.
            Use BYOK, or rebuild with <code>MINITI_API_KEY</code> set.
          </p>
        )}
      </Field>

      {prefs.app_mode === "managed" && !managedUnavailable && (
        <section className="card">
          <h2 className="card-title">Plan</h2>
          {usage ? (
            <div className="rows">
              <Row k="tier" v={usage.tier ?? "free"} />
              <Row
                k="minutes"
                v={`${Math.round(usage.minutes_used)} / ${usage.minutes_limit ?? "∞"}`}
              />
              {usage.resets_at && <Row k="resets" v={new Date(usage.resets_at).toLocaleDateString()} />}
              {usage.docs_lookups_limit != null && (
                <Row k="docs lookups" v={`${usage.docs_lookups_used ?? 0} / ${usage.docs_lookups_limit}`} />
              )}
            </div>
          ) : (
            <p className="muted">{usageError ?? "Loading usage…"}</p>
          )}
          <div className="save-row">
            {usage?.tier === "pro" ? (
              <button className="btn" onClick={() => openExternal(portalUrl)}>
                Manage subscription
              </button>
            ) : (
              <button className="btn primary" onClick={() => openExternal(subscribeUrl)}>
                Upgrade to Pro
              </button>
            )}
          </div>
          <Field label="Restore Pro with a license key">
            <div className="rec-controls">
              <input
                className="input"
                value={licenseKey}
                onChange={(e) => setLicenseKey(e.currentTarget.value)}
                placeholder="XXXX-XXXX-XXXX-XXXX"
              />
              <button className="btn" onClick={restore} disabled={!licenseKey.trim()}>
                Restore
              </button>
            </div>
          </Field>
        </section>
      )}

      <Field label="Language">
        <select
          className="input"
          value={prefs.language}
          onChange={(e) => update("language", e.currentTarget.value)}
        >
          {LANGUAGES.map((l) => (
            <option key={l} value={l}>
              {l}
            </option>
          ))}
        </select>
      </Field>

      {prefs.app_mode === "byok" && (
        <>
          <Field label="Deepgram API key">
            <input
              className="input"
              type="password"
              value={prefs.byok_deepgram_key ?? ""}
              onChange={(e) => update("byok_deepgram_key", e.currentTarget.value || null)}
              placeholder="Deepgram key"
            />
          </Field>
          <Field label="OpenAI API key (live insights, questions, catch-up, investigation)">
            <input
              className="input"
              type="password"
              value={prefs.byok_openai_key ?? ""}
              onChange={(e) => update("byok_openai_key", e.currentTarget.value || null)}
              placeholder="sk-…"
            />
          </Field>
        </>
      )}

      <section className="card">
        <h2 className="card-title">AI & insights</h2>
        <Toggle
          label="Live insights while recording (summary, questions, speaker names)"
          checked={prefs.live_insights_enabled}
          onChange={(v) => update("live_insights_enabled", v)}
        />
        <Toggle
          label="Start new meetings with Sales analysis (MEDDPICC) enabled"
          checked={prefs.sales_insights_default}
          onChange={(v) => update("sales_insights_default", v)}
        />
        <Field label="Docs MCP URL (Playbook) — HTTPS Streamable HTTP server, e.g. https://docs.example.com/mcp">
          <div className="rec-controls">
            <input
              className="input"
              value={prefs.docs_mcp_url ?? ""}
              onChange={(e) => update("docs_mcp_url", e.currentTarget.value || null)}
              placeholder="https://…/mcp"
            />
            <button
              className="btn"
              disabled={!prefs.docs_mcp_url}
              onClick={async () => {
                setError(null);
                setNotice(null);
                try {
                  const r = await probeDocsMcp(prefs.docs_mcp_url ?? "");
                  setNotice(`Docs MCP OK — search tool “${r.search_tool}” (${r.tools.length} tools)`);
                } catch (e) {
                  setError(errorMessage(e));
                }
              }}
            >
              Test
            </button>
          </div>
        </Field>
        <Field label="Codebase folder for investigations">
          <div className="rec-controls">
            <input className="input" value={prefs.codebase_root ?? ""} readOnly placeholder="not set" />
            <button
              className="btn"
              onClick={async () => {
                const p = await pickFolder().catch(() => null);
                if (p) update("codebase_root", p);
              }}
            >
              Choose…
            </button>
            {prefs.codebase_root && (
              <button className="ghost" onClick={() => update("codebase_root", null)}>clear</button>
            )}
          </div>
        </Field>
        <Field label="Personal dictionary (comma-separated terms sent to Deepgram as keyterms)">
          <input
            className="input"
            value={prefs.personal_dictionary.join(", ")}
            onChange={(e) =>
              update(
                "personal_dictionary",
                e.currentTarget.value.split(",").map((s) => s.trim()).filter((s) => s.length > 0),
              )
            }
            placeholder="Lightdash, Ahuja, MEDDPICC"
          />
        </Field>
      </section>

      <Field label="Webhook URL (POST meeting.saved after each recording, meeting.updated after insights)">
        <input
          className="input"
          value={prefs.webhook_url ?? ""}
          onChange={(e) => update("webhook_url", e.currentTarget.value || null)}
          placeholder="https://…"
        />
      </Field>

      <Field label="Coaching filler words (comma-separated; empty = language default)">
        <input
          className="input"
          value={prefs.filler_overrides.join(", ")}
          onChange={(e) =>
            update(
              "filler_overrides",
              e.currentTarget.value
                .split(",")
                .map((s) => s.trim())
                .filter((s) => s.length > 0),
            )
          }
          placeholder="um, uh, like"
        />
      </Field>

      <Toggle
        label="Capture system audio (remote speakers via PipeWire monitor; doubles Deepgram minutes)"
        checked={prefs.capture_system_audio}
        onChange={(v) => update("capture_system_audio", v)}
      />
      <section className="card">
        <h2 className="card-title">Notifications & presence</h2>
        <Toggle label="Show tray icon with recording timer (takes effect after restart)" checked={prefs.show_tray} onChange={(v) => update("show_tray", v)} />
        <Toggle
          label="Floating recording surface while recording (always on top; Wayland may ignore placement)"
          checked={prefs.show_floating_indicator}
          onChange={(v) => update("show_floating_indicator", v)}
        />
        <Toggle
          label="Desktop notifications when Miniti is not in front (Smart-meeting decisions, guidance)"
          checked={prefs.notifications_enabled}
          onChange={(v) => update("notifications_enabled", v)}
        />
        <Toggle
          label="Live guidance nudges (high-priority questions, long monologues, filler bursts)"
          checked={prefs.live_guidance_enabled}
          onChange={(v) => update("live_guidance_enabled", v)}
        />
      </section>

      <section className="card">
        <h2 className="card-title">Calendar & meetings</h2>
        <Toggle
          label="Smart meetings — notice when a meeting may have ended or another is approaching"
          checked={prefs.smart_meetings_enabled}
          onChange={(v) => update("smart_meetings_enabled", v)}
        />
        <Field label="Silence auto-stop">
          <select className="input" value={prefs.auto_stop_minutes} onChange={(e) => update("auto_stop_minutes", Number(e.currentTarget.value))}>
            <option value={0}>Off</option>
            <option value={3}>After 3 minutes</option>
            <option value={5}>After 5 minutes</option>
            <option value={10}>After 10 minutes</option>
            <option value={15}>After 15 minutes</option>
          </select>
        </Field>
        <Toggle label="Auto-start recording for upcoming calendar events" checked={prefs.calendar_auto_start} onChange={(v) => update("calendar_auto_start", v)} />
        <Toggle label="Auto-stop when a calendar event ends" checked={prefs.calendar_auto_stop} onChange={(v) => update("calendar_auto_stop", v)} />
      </section>

      <div className="save-row">
        <button className="btn primary" onClick={save}>
          Save settings
        </button>
        {saved && <span className="muted">saved ✓</span>}
      </div>
    </div>
  );
}

function Field({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="field">
      <label className="field-label">{label}</label>
      {children}
    </div>
  );
}

function Row({ k, v }: { k: string; v: string }) {
  return (
    <div className="row-line">
      <span className="k">{k}</span>
      <span className="v">{v}</span>
    </div>
  );
}

function Toggle({
  label,
  checked,
  onChange,
}: {
  label: string;
  checked: boolean;
  onChange: (v: boolean) => void;
}) {
  return (
    <label className="toggle">
      <input type="checkbox" checked={checked} onChange={(e) => onChange(e.currentTarget.checked)} />
      <span>{label}</span>
    </label>
  );
}
