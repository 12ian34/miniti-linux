import { useEffect, useState, type ReactNode } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import {
  environmentHealth,
  errorMessage,
  getPrefs,
  getUsage,
  hasBridge,
  portalUrl,
  restoreLicense,
  setPrefs,
  subscribeUrl,
} from "../api";
import type { AppMode, Prefs, Usage } from "../types";

const LANGUAGES = ["en", "es", "fr", "de", "pt", "it", "nl", "sv", "el", "pl", "ru"];

export function Settings() {
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
      <h1 className="view-title">Settings</h1>
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
          <Field label="OpenAI API key (insights, not wired yet)">
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

      <Field label="Webhook URL (POST meeting.saved after each recording)">
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
        label="Capture system audio (metering only until the multichannel engine lands)"
        checked={prefs.capture_system_audio}
        onChange={(v) => update("capture_system_audio", v)}
      />
      <Toggle
        label="Smart meetings (not wired yet)"
        checked={prefs.smart_meetings_enabled}
        onChange={(v) => update("smart_meetings_enabled", v)}
      />
      <Toggle label="Show tray icon (not wired yet)" checked={prefs.show_tray} onChange={(v) => update("show_tray", v)} />
      <Toggle
        label="Floating presence (not wired yet)"
        checked={prefs.show_floating_indicator}
        onChange={(v) => update("show_floating_indicator", v)}
      />

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
