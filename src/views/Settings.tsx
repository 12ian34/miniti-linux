import { useEffect, useState, type ReactNode } from "react";
import { getPrefs, hasBridge, setPrefs } from "../api";
import type { AppMode, Prefs } from "../types";

const LANGUAGES = ["en", "es", "fr", "de", "pt", "it", "nl", "sv", "el", "pl", "ru"];

export function Settings() {
  const [prefs, setPrefsState] = useState<Prefs | null>(null);
  const [saved, setSaved] = useState(false);

  useEffect(() => {
    if (!hasBridge) return;
    getPrefs().then(setPrefsState);
  }, []);

  function update<K extends keyof Prefs>(key: K, value: Prefs[K]) {
    setPrefsState((p) => (p ? { ...p, [key]: value } : p));
    setSaved(false);
  }

  async function save() {
    if (!prefs) return;
    await setPrefs(prefs);
    setSaved(true);
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

  return (
    <div className="view">
      <h1 className="view-title">Settings</h1>

      <Field label="Mode">
        <select
          className="input"
          value={prefs.app_mode}
          onChange={(e) => update("app_mode", e.currentTarget.value as AppMode)}
        >
          <option value="managed">Managed (Miniti backend)</option>
          <option value="byok">BYOK (your own keys)</option>
        </select>
      </Field>

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
              placeholder="dg_…"
            />
          </Field>
          <Field label="OpenAI API key">
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

      <Field label="Webhook URL">
        <input
          className="input"
          value={prefs.webhook_url ?? ""}
          onChange={(e) => update("webhook_url", e.currentTarget.value || null)}
          placeholder="https://…"
        />
      </Field>

      <Toggle
        label="Capture system audio"
        checked={prefs.capture_system_audio}
        onChange={(v) => update("capture_system_audio", v)}
      />
      <Toggle
        label="Smart meetings"
        checked={prefs.smart_meetings_enabled}
        onChange={(v) => update("smart_meetings_enabled", v)}
      />
      <Toggle label="Show tray icon" checked={prefs.show_tray} onChange={(v) => update("show_tray", v)} />
      <Toggle
        label="Floating presence"
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
