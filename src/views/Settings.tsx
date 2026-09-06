import { useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { confirm, save } from "@tauri-apps/plugin-dialog";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import {
  authDeleteAccount,
  authDevices,
  authRecoveryKey,
  authRemoveDevice,
  authRotateRecoveryKey,
  authSignOut,
  authStatus,
  crmConnect,
  debugLogClear,
  debugLogExport,
  debugLogPath,
  debugLogReveal,
  debugLogTail,
  crmStatus,
  environmentHealth,
  errorMessage,
  getPrefs,
  getUsage,
  googleConnect,
  googleDisconnect,
  googleStatus,
  hasBridge,
  importGranolaCsv,
  onDeepLink,
  pickFolder,
  portalUrl,
  probeDocsMcp,
  restoreLicense,
  setPrefs,
  subscribeUrl,
} from "../api";
import type { AppMode, AuthDevices, AuthStatus, CrmProvider, EnvHealth, Prefs, Usage } from "../types";
import { Enroll } from "./Enroll";
import { Sheet } from "./MeetingTools";
import { useStore } from "../store";
import { useTauriEvent } from "../useEvent";

const LANGUAGES: [string, string][] = [
  ["en", "English"], ["es", "Español"], ["fr", "Français"], ["de", "Deutsch"], ["pt", "Português"],
  ["it", "Italiano"], ["nl", "Nederlands"], ["sv", "Svenska"], ["el", "Ελληνικά"], ["pl", "Polski"], ["ru", "Русский"],
];

/** macOS settings destinations (iOS-only items omitted; CRM is desktop-only so it stays). */
const DESTINATIONS = [
  { id: "general", title: "General" },
  { id: "account", title: "Account & Plan" },
  { id: "recording", title: "Recording & Audio" },
  { id: "language", title: "Language" },
  { id: "ai", title: "AI & Models" },
  { id: "notifications", title: "Notifications" },
  { id: "calendar", title: "Calendar & Meetings" },
  { id: "crm", title: "CRM" },
  { id: "webhooks", title: "Webhooks" },
  { id: "docs", title: "Docs MCP" },
  { id: "data", title: "Data & Export" },
  { id: "privacy", title: "Privacy & Support" },
] as const;
type DestId = (typeof DESTINATIONS)[number]["id"];

/** Search index: every individual control with its destination + keywords. */
interface Control {
  id: string;
  dest: DestId;
  label: string;
  keywords: string;
}

function readDest(): DestId {
  try {
    const v = localStorage.getItem("settings.dest") as DestId | null;
    if (v && DESTINATIONS.some((d) => d.id === v)) return v;
  } catch { /* ignore */ }
  return "general";
}

export function Settings() {
  const { back, refreshPrefs } = useStore();
  const [dest, setDestState] = useState<DestId>(readDest);
  const [query, setQuery] = useState("");
  const [prefs, setPrefsState] = useState<Prefs | null>(null);
  const [saved, setSaved] = useState(false);
  const [health, setHealth] = useState<EnvHealth | null>(null);
  const [auth, setAuth] = useState<AuthStatus | null>(null);
  const [usage, setUsage] = useState<Usage | null>(null);
  const [usageError, setUsageError] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [licenseKey, setLicenseKey] = useState("");
  const [notice, setNotice] = useState<string | null>(null);
  const [google, setGoogle] = useState<{ connected: boolean; email: string | null } | null>(null);
  const [crm, setCrm] = useState<Record<CrmProvider, { connected: boolean; account_label: string | null } | null>>({ attio: null, twenty: null });
  const [highlight, setHighlight] = useState<string | null>(null);
  const searchRef = useRef<HTMLInputElement>(null);
  const backendKey = auth?.enrolled ?? null;
  const notEnrolled = backendKey === false;
  const refreshAuth = () => { if (hasBridge) authStatus().then(setAuth).catch(() => setAuth(null)); };

  function setDest(d: DestId) {
    setDestState(d);
    try { localStorage.setItem("settings.dest", d); } catch { /* ignore */ }
  }

  const refreshIntegrations = () => {
    if (!hasBridge || backendKey !== true) return;
    googleStatus().then(setGoogle).catch(() => setGoogle(null));
    (["attio", "twenty"] as CrmProvider[]).forEach((p) =>
      crmStatus(p).then((s) => setCrm((c) => ({ ...c, [p]: s }))).catch(() => setCrm((c) => ({ ...c, [p]: null }))),
    );
  };
  useEffect(refreshIntegrations, [backendKey]);
  useTauriEvent(onDeepLink, (ev) => {
    if (ev.query.status === "success") setNotice(`${ev.scheme.replace("miniti-", "")} connected.`);
    else if (ev.query.status === "error") setError(ev.query.message ?? "Connection failed.");
    refreshIntegrations();
  });

  useEffect(() => {
    if (!hasBridge) return;
    getPrefs().then(setPrefsState);
    environmentHealth().then(setHealth).catch(() => {});
    refreshAuth();
  }, []);

  useEffect(() => {
    if (!hasBridge || !prefs || prefs.app_mode !== "managed" || backendKey !== true) return;
    getUsage()
      .then((u) => { setUsage(u); setUsageError(null); })
      .catch((e) => setUsageError(errorMessage(e)));
  }, [prefs?.app_mode, backendKey]);

  // Ctrl+F focuses settings search (macOS ⌘F).
  useEffect(() => {
    function onKey(e: KeyboardEvent) {
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "f") {
        e.preventDefault();
        searchRef.current?.focus();
      }
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

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
    try { await openUrl(await fetchUrl()); } catch (e) { setError(errorMessage(e)); }
  }
  async function connect(kind: "google" | CrmProvider) {
    setError(null);
    try {
      const url = kind === "google" ? await googleConnect() : await crmConnect(kind);
      await openUrl(url);
      setNotice("Finish signing in in your browser; miniti picks up the return automatically.");
    } catch (e) {
      setError(errorMessage(e));
    }
  }
  async function restore() {
    setError(null); setNotice(null);
    try {
      await restoreLicense(licenseKey);
      setNotice("Pro restored on this device.");
      setLicenseKey("");
      setUsage(await getUsage());
    } catch (e) { setError(errorMessage(e)); }
  }
  async function importGranola() {
    setError(null); setNotice(null);
    try {
      const r = await importGranolaCsv();
      if (!r) return;
      const parts = [r.imported > 0 ? `Imported ${r.imported} meeting${r.imported === 1 ? "" : "s"}` : "No new meetings imported"];
      if (r.duplicates > 0) parts.push(`${r.duplicates} already imported`);
      if (r.skipped_rows > 0) parts.push(`${r.skipped_rows} row${r.skipped_rows === 1 ? "" : "s"} skipped`);
      setNotice(parts.join(" · "));
    } catch (e) { setError(errorMessage(e)); }
  }

  const controls: Control[] = useMemo(() => [
    { id: "tray", dest: "general", label: "Tray icon", keywords: "tray menu bar timer icon" },
    { id: "presence", dest: "general", label: "Floating recording surface", keywords: "floating window indicator presence always on top" },
    { id: "mode", dest: "account", label: "Mode", keywords: "managed byok mode api backend" },
    { id: "plan", dest: "account", label: "Plan & usage", keywords: "pro upgrade subscription minutes usage polar portal restore license" },
    { id: "recovery", dest: "account", label: "Recovery key & devices", keywords: "recovery key account devices sign out delete rotate reveal" },
    { id: "keys", dest: "account", label: "API keys", keywords: "deepgram openai key byok" },
    { id: "device", dest: "account", label: "Device ID", keywords: "device id uuid" },
    { id: "sources", dest: "recording", label: "Audio sources", keywords: "microphone system audio pipewire capture" },
    { id: "autostop", dest: "recording", label: "Silence auto-stop", keywords: "auto stop silence minutes quiet" },
    { id: "lang", dest: "language", label: "Transcription language", keywords: "language english spanish" },
    { id: "dict", dest: "language", label: "Personal dictionary", keywords: "dictionary keyterms names terms deepgram" },
    { id: "fillers", dest: "language", label: "Filler words", keywords: "filler coaching um uh" },
    { id: "live", dest: "ai", label: "Live insights", keywords: "insights summary questions live" },
    { id: "sales", dest: "ai", label: "Sales analysis default", keywords: "sales meddpicc default" },
    { id: "models", dest: "ai", label: "Models", keywords: "model gpt nova deepgram openai" },
    { id: "codebase", dest: "ai", label: "Codebase folder", keywords: "codebase investigate folder repository" },
    { id: "notify", dest: "notifications", label: "Desktop notifications", keywords: "notifications desktop notify" },
    { id: "guidance", dest: "notifications", label: "Live guidance", keywords: "guidance nudge question monologue filler" },
    { id: "smart", dest: "calendar", label: "Smart meetings", keywords: "smart meetings quiet ended prompt call" },
    { id: "gcal", dest: "calendar", label: "Google Calendar", keywords: "google calendar connect events upcoming" },
    { id: "calauto", dest: "calendar", label: "Calendar automation", keywords: "auto start auto stop calendar" },
    { id: "attio", dest: "crm", label: "Attio", keywords: "attio crm" },
    { id: "twenty", dest: "crm", label: "Twenty", keywords: "twenty crm" },
    { id: "webhook", dest: "webhooks", label: "Webhook URL", keywords: "webhook url post meeting saved" },
    { id: "mcp", dest: "docs", label: "Docs MCP URL", keywords: "docs mcp playbook documentation" },
    { id: "granola", dest: "data", label: "Granola import", keywords: "granola import csv" },
    { id: "export", dest: "data", label: "Markdown export folder", keywords: "export markdown folder" },
    { id: "about", dest: "privacy", label: "Version & diagnostics", keywords: "version about diagnostics privacy terms support" },
    { id: "log", dest: "privacy", label: "Debug log", keywords: "debug log logs diagnostics report issue save copy" },
  ], []);

  const q = query.trim().toLowerCase();
  const matches = q ? controls.filter((c) => `${c.label} ${c.keywords} ${DESTINATIONS.find((d) => d.id === c.dest)?.title}`.toLowerCase().includes(q)) : [];

  function jump(c: Control) {
    setDest(c.dest);
    setQuery("");
    setHighlight(c.id);
    window.setTimeout(() => {
      document.getElementById(`s-${c.id}`)?.scrollIntoView({ block: "center" });
      window.setTimeout(() => setHighlight(null), 1600);
    }, 30);
  }

  if (!prefs) {
    return (
      <div className="view">
        <h1 className="view-title">Settings</h1>
        <p className="muted">{hasBridge ? "Loading…" : "Browser preview — native bridge unavailable."}</p>
      </div>
    );
  }

  const C = ({ id, children }: { id: string; children: ReactNode }) => (
    <div id={`s-${id}`} className={`setting ${highlight === id ? "flash" : ""}`}>{children}</div>
  );

  return (
    <div className="settings">
      <aside className="settings-side">
        <div className="detail-head">
          <button className="ghost" onClick={back}>‹</button>
          <span className="view-title small-title">Settings</span>
        </div>
        <input ref={searchRef} className="input search" placeholder="search settings (Ctrl+F)" value={query} onChange={(e) => setQuery(e.currentTarget.value)} />
        {q ? (
          <div className="settings-results">
            {matches.length === 0 && <p className="muted pad">No matching settings.</p>}
            {matches.map((c) => (
              <button key={c.id} className="settings-result" onClick={() => jump(c)}>
                <span>{c.label}</span>
                <span className="muted tiny">{DESTINATIONS.find((d) => d.id === c.dest)?.title}</span>
              </button>
            ))}
          </div>
        ) : (
          <nav className="settings-nav">
            {DESTINATIONS.map((d) => (
              <button key={d.id} className={`side-item ${dest === d.id ? "active" : ""}`} onClick={() => setDest(d.id)}>{d.title}</button>
            ))}
          </nav>
        )}
      </aside>

      <div className="settings-detail">
        <h1 className="view-title">{DESTINATIONS.find((d) => d.id === dest)?.title}</h1>
        {error && <div className="banner error">{error}</div>}
        {notice && <div className="banner ok">{notice}<button className="ghost" onClick={() => setNotice(null)}>×</button></div>}

        {dest === "general" && (
          <>
            <C id="tray"><Toggle label="Show tray icon with recording timer (takes effect after restart)" checked={prefs.show_tray} onChange={(v) => update("show_tray", v)} /></C>
            <C id="presence"><Toggle label="Floating recording surface while recording (always on top; Wayland may ignore placement)" checked={prefs.show_floating_indicator} onChange={(v) => update("show_floating_indicator", v)} /></C>
            <p className="muted small">Shortcuts: Ctrl+R record/stop · Ctrl+[ history · Ctrl+] insights · Ctrl+, settings · Ctrl+F search settings.</p>
          </>
        )}

        {dest === "account" && (
          <>
            <C id="mode">
              <Field label="Mode">
                <select className="input" value={prefs.app_mode} onChange={(e) => update("app_mode", e.currentTarget.value as AppMode)}>
                  <option value="managed">Managed (miniti backend)</option>
                  <option value="byok">BYOK (your own keys)</option>
                </select>
              </Field>
            </C>
            {prefs.app_mode === "managed" && notEnrolled && (
              <C id="recovery">
                <section className="card">
                  <h2 className="card-title">Account</h2>
                  <Enroll inline onDone={() => { refreshAuth(); refreshPrefs(); getPrefs().then(setPrefsState); }} />
                </section>
              </C>
            )}
            {prefs.app_mode === "managed" && backendKey === true && (
              <C id="plan">
                <section className="card">
                  <h2 className="card-title">Plan</h2>
                  {usage ? (
                    <div className="rows">
                      <Row k="tier" v={usage.tier ?? "free"} />
                      <Row k="minutes" v={`${Math.round(usage.minutes_used)} / ${usage.minutes_limit ?? "∞"}`} />
                      {usage.resets_at && <Row k="resets" v={new Date(usage.resets_at).toLocaleDateString()} />}
                      {usage.docs_lookups_limit != null && <Row k="docs lookups" v={`${usage.docs_lookups_used ?? 0} / ${usage.docs_lookups_limit}`} />}
                    </div>
                  ) : <p className="muted">{usageError ?? "Loading usage…"}</p>}
                  <div className="save-row">
                    {usage?.tier === "pro"
                      ? <button className="btn" onClick={() => openExternal(portalUrl)}>Manage subscription</button>
                      : <button className="btn primary" onClick={() => openExternal(subscribeUrl)}>Upgrade to Pro</button>}
                  </div>
                  <Field label="Restore Pro with a license key">
                    <div className="rec-controls">
                      <input className="input" value={licenseKey} onChange={(e) => setLicenseKey(e.currentTarget.value)} placeholder="XXXX-XXXX-XXXX-XXXX" />
                      <button className="btn" onClick={restore} disabled={!licenseKey.trim()}>Restore</button>
                    </div>
                  </Field>
                </section>
              </C>
            )}
            {prefs.app_mode === "managed" && backendKey === true && auth && (
              <C id="recovery">
                <AccountCard auth={auth} onChanged={() => { refreshAuth(); setUsage(null); }} onError={setError} onNotice={setNotice} />
              </C>
            )}
            {prefs.app_mode === "byok" && (
              <C id="keys">
                <Field label="Deepgram API key">
                  <input className="input" type="password" value={prefs.byok_deepgram_key ?? ""} onChange={(e) => update("byok_deepgram_key", e.currentTarget.value || null)} placeholder="Deepgram key" />
                </Field>
                <Field label="OpenAI API key (live insights, questions, catch-up, investigation)">
                  <input className="input" type="password" value={prefs.byok_openai_key ?? ""} onChange={(e) => update("byok_openai_key", e.currentTarget.value || null)} placeholder="sk-…" />
                </Field>
              </C>
            )}
            <C id="device"><Field label="Device ID"><input className="input" readOnly value={health?.device_id ?? "…"} /></Field></C>
          </>
        )}

        {dest === "recording" && (
          <>
            <C id="sources">
              <div className="rows">
                <Row k="microphone" v={health ? (health.microphone_available ? "detected" : "not detected") : "…"} />
                <Row k="system audio" v={health ? (health.system_audio_available ? "PipeWire monitor available" : "not detected (install pipewire-pulse)") : "…"} />
              </div>
              <Toggle label="Capture system audio (remote speakers via PipeWire monitor; doubles Deepgram minutes)" checked={prefs.capture_system_audio} onChange={(v) => update("capture_system_audio", v)} />
            </C>
            <C id="autostop">
              <Field label="Silence auto-stop">
                <select className="input" value={prefs.auto_stop_minutes} onChange={(e) => update("auto_stop_minutes", Number(e.currentTarget.value))}>
                  <option value={0}>Off</option>
                  <option value={3}>After 3 minutes</option>
                  <option value={5}>After 5 minutes</option>
                  <option value={10}>After 10 minutes</option>
                  <option value={15}>After 15 minutes</option>
                </select>
              </Field>
            </C>
          </>
        )}

        {dest === "language" && (
          <>
            <C id="lang">
              <Field label="Transcription language">
                <select className="input" value={prefs.language} onChange={(e) => update("language", e.currentTarget.value)}>
                  {LANGUAGES.map(([code, name]) => <option key={code} value={code}>{name}</option>)}
                </select>
              </Field>
            </C>
            <C id="dict">
              <Field label="Personal dictionary (comma-separated terms sent to Deepgram as keyterms)">
                <input className="input" value={prefs.personal_dictionary.join(", ")} onChange={(e) => update("personal_dictionary", e.currentTarget.value.split(",").map((s) => s.trim()).filter(Boolean))} placeholder="Lightdash, Ahuja, MEDDPICC" />
              </Field>
            </C>
            <C id="fillers">
              <Field label="Coaching filler words (comma-separated; empty = language default)">
                <input className="input" value={prefs.filler_overrides.join(", ")} onChange={(e) => update("filler_overrides", e.currentTarget.value.split(",").map((s) => s.trim()).filter(Boolean))} placeholder="um, uh, like" />
              </Field>
            </C>
          </>
        )}

        {dest === "ai" && (
          <>
            <C id="live"><Toggle label="Live insights while recording (summary, questions, speaker names)" checked={prefs.live_insights_enabled} onChange={(v) => update("live_insights_enabled", v)} /></C>
            <C id="sales"><Toggle label="Start new meetings with Sales analysis (MEDDPICC) enabled" checked={prefs.sales_insights_default} onChange={(v) => update("sales_insights_default", v)} /></C>
            <C id="models">
              <section className="card">
                <h2 className="card-title">Models</h2>
                <div className="rows">
                  <Row k="transcription" v="Deepgram Nova-3 (streaming, diarized)" />
                  <Row k="insights" v={prefs.app_mode === "byok" ? "gpt-5-mini (investigations: gpt-5.4-mini) via your OpenAI key" : "gpt-5.4-mini full passes · gpt-5-mini incremental, via the miniti backend"} />
                  <Row k="coaching" v="computed locally, never sent anywhere" />
                </div>
              </section>
            </C>
            <C id="codebase">
              <Field label="Codebase folder for investigations">
                <div className="rec-controls">
                  <input className="input" value={prefs.codebase_root ?? ""} readOnly placeholder="not set" />
                  <button className="btn" onClick={async () => { const p = await pickFolder().catch(() => null); if (p) update("codebase_root", p); }}>Choose…</button>
                  {prefs.codebase_root && <button className="ghost" onClick={() => update("codebase_root", null)}>clear</button>}
                </div>
              </Field>
            </C>
          </>
        )}

        {dest === "notifications" && (
          <>
            <p className="muted small">Decisions and guidance show on the floating surface while miniti is in front, and as desktop notifications otherwise. The tray menu always carries the same decisions.</p>
            <C id="notify"><Toggle label="Desktop notifications when miniti is not in front (Smart-meeting decisions, guidance)" checked={prefs.notifications_enabled} onChange={(v) => update("notifications_enabled", v)} /></C>
            <C id="guidance">
              <Toggle label="Live guidance nudges (high-priority questions, long monologues, filler bursts)" checked={prefs.live_guidance_enabled} onChange={(v) => update("live_guidance_enabled", v)} />
              {prefs.live_guidance_enabled && (
                <div className="rows">
                  {([["question", "Worth asking — a high-priority question surfaces"], ["monologue", "Long stretch — you have been speaking for a while"], ["filler", "Filler burst — many fillers in the last minute"], ["sales", "Sounds like a sales call — offer Sales analysis"]] as const).map(([kind, label]) => (
                    <Toggle
                      key={kind}
                      label={label}
                      checked={!(prefs.disabled_nudge_kinds ?? []).includes(kind)}
                      onChange={(on) => update("disabled_nudge_kinds", on ? (prefs.disabled_nudge_kinds ?? []).filter((k) => k !== kind) : [...(prefs.disabled_nudge_kinds ?? []), kind])}
                    />
                  ))}
                  <p className="muted small">"Don't remind me" on the floating surface switches a kind off here.</p>
                </div>
              )}
            </C>
          </>
        )}

        {dest === "calendar" && (
          <>
            <C id="smart"><Toggle label="Smart meetings — notice when a meeting may have ended or another is approaching, then help finish, save, and start the right recording" checked={prefs.smart_meetings_enabled} onChange={(v) => update("smart_meetings_enabled", v)} /></C>
            <C id="gcal">
              <div className="row-line">
                <span className="k">Google Calendar</span>
                {notEnrolled ? <span className="muted">needs managed mode with an enrolled account (Account &amp; Plan)</span> : (
                  <>
                    <span className="v">{google === null ? "…" : google.connected ? `connected${google.email ? ` (${google.email})` : ""}` : "not connected"}</span>
                    {google?.connected
                      ? <button className="ghost" onClick={() => googleDisconnect().then(refreshIntegrations).catch((e) => setError(errorMessage(e)))}>disconnect</button>
                      : <button className="btn small" onClick={() => connect("google")}>Connect</button>}
                  </>
                )}
              </div>
            </C>
            <C id="calauto">
              <Toggle label="Auto-start recording for upcoming calendar events (16 s countdown)" checked={prefs.calendar_auto_start} onChange={(v) => update("calendar_auto_start", v)} />
              <Toggle label="Auto-stop when a calendar event ends (enables automatic handoff with Smart meetings)" checked={prefs.calendar_auto_stop} onChange={(v) => update("calendar_auto_stop", v)} />
            </C>
          </>
        )}

        {dest === "crm" && (
          notEnrolled ? <p className="muted">Attio and Twenty connect through the miniti backend; set up managed mode under Account &amp; Plan first.</p> : (
            <>
              {(["attio", "twenty"] as CrmProvider[]).map((p) => (
                <C id={p} key={p}>
                  <div className="row-line">
                    <span className="k">{p === "attio" ? "Attio" : "Twenty"}</span>
                    <span className="v">{crm[p] === null ? "…" : crm[p]!.connected ? `connected${crm[p]!.account_label ? ` (${crm[p]!.account_label})` : ""}` : "not connected"}</span>
                    {!crm[p]?.connected && <button className="btn small" onClick={() => connect(p)}>Connect</button>}
                  </div>
                </C>
              ))}
              <p className="muted small">Send a saved meeting from its header (crm). Notes include summary, discussion, actions, decisions, topics, MEDDPICC and your notes; the transcript is never sent.</p>
            </>
          )
        )}

        {dest === "webhooks" && (
          <C id="webhook">
            <Field label="Webhook URL (POST meeting.saved after each recording, meeting.updated after insights)">
              <input className="input" value={prefs.webhook_url ?? ""} onChange={(e) => update("webhook_url", e.currentTarget.value || null)} placeholder="https://…" />
            </Field>
            <p className="muted small">Same JSON shape as the macOS app: a <code>meeting</code> envelope with insights, a <code>training</code> coaching blob and the transcript with resolved speaker labels. 10 s timeout, fire-and-forget.</p>
          </C>
        )}

        {dest === "docs" && (
          <C id="mcp">
            <Field label="Docs MCP URL (Playbook) — HTTPS Streamable HTTP server, e.g. https://docs.example.com/mcp">
              <div className="rec-controls">
                <input className="input" value={prefs.docs_mcp_url ?? ""} onChange={(e) => update("docs_mcp_url", e.currentTarget.value || null)} placeholder="https://…/mcp" />
                <button className="btn" disabled={!prefs.docs_mcp_url} onClick={async () => {
                  setError(null); setNotice(null);
                  try { const r = await probeDocsMcp(prefs.docs_mcp_url ?? ""); setNotice(`Docs MCP OK — search tool “${r.search_tool}” (${r.tools.length} tools)`); }
                  catch (e) { setError(errorMessage(e)); }
                }}>Test</button>
              </div>
            </Field>
            <p className="muted small">Topics the conversation raises are looked up automatically for BYOK and Pro; managed-free has a monthly allowance and looks up on demand.</p>
          </C>
        )}

        {dest === "data" && (
          <>
            <C id="granola">
              <Field label="Granola import">
                <div className="rec-controls">
                  <button className="btn" onClick={importGranola}>Import Granola CSV…</button>
                  <button className="ghost" onClick={() => openUrl("https://notes.granola.ai/settings/profile")}>get an export</button>
                </div>
                <p className="muted small">Duplicate protection: rows already imported are skipped. Imported meetings show their source in the sidebar.</p>
              </Field>
            </C>
            <C id="export">
              <Field label="Markdown export folder (remembered from the last export)">
                <div className="rec-controls">
                  <input className="input" readOnly value={prefs.export_folder ?? ""} placeholder="not set — the save dialog asks" />
                  <button className="btn" onClick={async () => { const p = await pickFolder().catch(() => null); if (p) update("export_folder", p); }}>Choose…</button>
                </div>
              </Field>
            </C>
          </>
        )}

        {dest === "privacy" && (
          <>
          <C id="about">
            <section className="card">
              <h2 className="card-title">About</h2>
              <div className="rows">
                <Row k="version" v={health ? `miniti Linux ${health.app_version}` : "…"} />
                <Row k="platform" v={health ? `${health.platform} · ${health.os}` : "…"} />
                <Row k="managed mode" v={auth ? (auth.enrolled ? `enrolled (${auth.storage === "file" ? "credentials in ~/.local/share/miniti/auth.json" : "credentials in the secret service"})` : "not enrolled") : "…"} />
                <Row k="data" v="~/.local/share/miniti · prefs in ~/.config/miniti" />
              </div>
              <div className="save-row">
                <button className="ghost" onClick={() => openUrl("https://miniti.app/docs")}>docs</button>
                <button className="ghost" onClick={() => openUrl("https://miniti.app/changelog")}>changelog</button>
                <button className="ghost" onClick={() => openUrl("https://miniti.app/terms")}>terms & privacy</button>
                <button className="ghost" onClick={() => openUrl("https://github.com/12ian34/miniti-linux/issues")}>report an issue</button>
              </div>
              <p className="muted small">Transcripts and audio never leave the device except to Deepgram and, in managed mode, the miniti backend.</p>
            </section>
          </C>
          <C id="log">
            <DebugLogCard onError={setError} onNotice={setNotice} />
          </C>
          </>
        )}

        <div className="save-row sticky-save">
          <button className="btn primary" onClick={save}>Save settings</button>
          {saved && <span className="muted">saved ✓</span>}
        </div>
      </div>
    </div>
  );
}

/** Daily rolling log the user can read, copy, save, or clear (macOS DebugLogView). */
function DebugLogCard({ onError, onNotice }: { onError: (m: string | null) => void; onNotice: (m: string | null) => void }) {
  const [open, setOpen] = useState(false);
  const [text, setText] = useState("");
  const [path, setPath] = useState("");
  const load = () => Promise.all([debugLogTail(600), debugLogPath()]).then(([t, p]) => { setText(t); setPath(p); }).catch((e) => onError(errorMessage(e)));
  useEffect(() => { if (open) void load(); }, [open]);
  async function copyLog() {
    try { await writeText(text); onNotice("Log copied."); } catch { onError("Could not copy to the clipboard."); }
  }
  async function saveLog() {
    try {
      const dest = await save({ defaultPath: `miniti-log-${new Date().toISOString().slice(0, 10)}.txt`, filters: [{ name: "Text", extensions: ["txt"] }] });
      if (dest) { await debugLogExport(dest); onNotice(`Log saved to ${dest}`); }
    } catch (e) { onError(errorMessage(e)); }
  }
  async function clearLog() {
    if (!(await confirm("Clear the current log file?", { title: "Clear log", kind: "warning" }))) return;
    try { await debugLogClear(); await load(); } catch (e) { onError(errorMessage(e)); }
  }
  return (
    <section className="card">
      <h2 className="card-title">Debug log</h2>
      <p className="muted small">miniti keeps seven days of logs in <code>~/.local/share/miniti/logs</code>. No transcript text, keys or tokens are written. Attach the log when you report an issue.</p>
      <div className="save-row">
        <button className="btn small" onClick={() => setOpen(true)}>view log</button>
        <button className="btn small" onClick={() => debugLogReveal().catch((e) => onError(errorMessage(e)))}>show in files</button>
      </div>
      {open && (
        <Sheet title="debug log" onClose={() => setOpen(false)}>
          <p className="muted tiny">{path}</p>
          <pre className="log-view">{text || "(empty)"}</pre>
          <div className="save-row">
            <button className="btn small" onClick={load}>refresh</button>
            <button className="btn small" onClick={copyLog}>copy</button>
            <button className="btn small" onClick={saveLog}>save…</button>
            <button className="btn small" onClick={clearLog}>clear</button>
          </div>
        </Sheet>
      )}
    </section>
  );
}

/** Recovery key, devices, sign-out and deletion for the anonymous account (macOS Settings › Account). */
function AccountCard({ auth, onChanged, onError, onNotice }: { auth: AuthStatus; onChanged: () => void; onError: (m: string | null) => void; onNotice: (m: string | null) => void }) {
  const [revealed, setRevealed] = useState<string | null>(null);
  const [devices, setDevices] = useState<AuthDevices | null>(null);
  const [devicesError, setDevicesError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const loadDevices = () => authDevices().then((d) => { setDevices(d); setDevicesError(null); }).catch((e) => setDevicesError(errorMessage(e)));
  useEffect(() => { void loadDevices(); }, []);

  async function reveal() {
    if (!(await confirm("Show your recovery key on screen? Anyone who sees it can add devices to this account.", { title: "Reveal recovery key", kind: "warning" }))) return;
    try { setRevealed(await authRecoveryKey()); } catch (e) { onError(errorMessage(e)); }
  }
  async function copyKey() {
    if (!revealed) return;
    try { await writeText(revealed); onNotice("Recovery key copied."); } catch { onError("Could not copy to the clipboard."); }
  }
  async function rotate() {
    if (!(await confirm("Generate a new recovery key? The current key stops working immediately; your devices stay signed in.", { title: "Rotate recovery key", kind: "warning" }))) return;
    setBusy(true);
    try { setRevealed(await authRotateRecoveryKey()); onNotice("Recovery key rotated. Save the new key."); } catch (e) { onError(errorMessage(e)); } finally { setBusy(false); }
  }
  async function remove(id: string, current: boolean) {
    const msg = current ? "Sign this computer out of the account?" : "Remove this device from the account? It will need the recovery key to sign back in.";
    if (!(await confirm(msg, { title: "Remove device", kind: "warning" }))) return;
    setBusy(true);
    try { await authRemoveDevice(id); if (current) onChanged(); else await loadDevices(); } catch (e) { onError(errorMessage(e)); } finally { setBusy(false); }
  }
  async function signOut() {
    if (!(await confirm("Sign this computer out? You will need the recovery key to use managed mode again.", { title: "Sign out", kind: "warning" }))) return;
    setBusy(true);
    try { await authSignOut(); onChanged(); } catch (e) { onError(errorMessage(e)); } finally { setBusy(false); }
  }
  async function deleteAccount() {
    if (!(await confirm("Delete this account? Every device loses access and the recovery key stops working. This does not cancel a Polar subscription; manage that from the customer portal first.", { title: "Delete account", kind: "warning", okLabel: "Delete account" }))) return;
    setBusy(true);
    try { await authDeleteAccount(); onChanged(); } catch (e) { onError(errorMessage(e)); } finally { setBusy(false); }
  }

  return (
    <section className="card">
      <h2 className="card-title">Account</h2>
      <div className="rows">
        <Row k="account" v={auth.account_id ? `${auth.account_id.slice(0, 12)}…` : "—"} />
        <Row k="devices" v={devices ? `${devices.devices.length} / ${devices.device_cap ?? auth.device_cap ?? "—"}` : devicesError ?? "…"} />
        {auth.storage === "file" && <Row k="storage" v="no secret service found; credentials kept in ~/.local/share/miniti/auth.json (0600)" />}
      </div>
      <Field label="Recovery key">
        {revealed ? (
          <>
            <div className="recovery-key">{revealed}</div>
            <div className="save-row">
              <button className="btn small" onClick={copyKey}>copy</button>
              <button className="btn small" onClick={() => setRevealed(null)}>hide</button>
            </div>
          </>
        ) : (
          <div className="save-row">
            <button className="btn small" onClick={reveal} disabled={busy}>reveal</button>
            <button className="btn small" onClick={rotate} disabled={busy}>rotate</button>
          </div>
        )}
      </Field>
      <Field label="Devices on this account">
        {devices ? (
          devices.devices.map((d) => (
            <div key={d.installation_id} className="device-row">
              <div className="grow">
                <div>{d.label ?? d.installation_id.slice(0, 8)}{d.current ? " · this computer" : ""}</div>
                <div className="sub">{[d.platform, d.app_version, d.last_auth_at ? `seen ${new Date(d.last_auth_at).toLocaleDateString()}` : null].filter(Boolean).join(" · ")}</div>
              </div>
              <button className="control destructive" disabled={busy} onClick={() => remove(d.installation_id, d.current)}>{d.current ? "sign out" : "remove"}</button>
            </div>
          ))
        ) : <p className="muted">{devicesError ?? "Loading devices…"}</p>}
      </Field>
      <div className="save-row">
        <button className="btn" onClick={signOut} disabled={busy}>Sign out</button>
        <button className="btn danger" onClick={deleteAccount} disabled={busy}>Delete account</button>
      </div>
    </section>
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

function Toggle({ label, checked, onChange }: { label: string; checked: boolean; onChange: (v: boolean) => void }) {
  return (
    <label className="toggle">
      <input type="checkbox" checked={checked} onChange={(e) => onChange(e.currentTarget.checked)} />
      <span>{label}</span>
    </label>
  );
}
