import { useEffect, useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { acceptTerms, completeOnboarding, errorMessage, omarchyInstallWidget, omarchyStatus } from "../api";
import type { LaunchGate, OmarchyStatus } from "../types";

interface GateProps {
  gate: LaunchGate;
  onDone: () => void;
}

/** Non-dismissable: the backend's min_version is above this build. */
export function ForceUpdate({ gate }: { gate: LaunchGate }) {
  return (
    <div className="gate">
      <h1 className="view-title">Update required</h1>
      <p>
        This version ({gate.current_version}) is no longer supported. Please update to{" "}
        {gate.latest_version ?? "the latest version"} to keep recording.
      </p>
      <p className="muted">
        Omarchy and Arch: <code>sudo pacman -Syu miniti-bin</code>. Other distros: download the
        latest release tarball.
      </p>
      {gate.download_url && (
        <button className="btn primary" onClick={() => openUrl(gate.download_url!)}>
          Open download page
        </button>
      )}
    </div>
  );
}

export function Terms({ gate, onDone }: GateProps) {
  const [error, setError] = useState<string | null>(null);
  async function accept() {
    try {
      await acceptTerms();
      onDone();
    } catch (e) {
      setError(errorMessage(e));
    }
  }
  return (
    <div className="gate">
      <h1 className="view-title">Before you start</h1>
      <p>
        miniti records audio from your microphone (and, where enabled, system audio), streams it to
        Deepgram for transcription, and in managed mode sends transcripts to the miniti backend for
        insights. Meetings are stored locally on this device.
      </p>
      <p className="muted">
        You are responsible for obtaining consent from participants where the law requires it. Terms
        version {gate.terms_version}. Full terms and privacy policy:{" "}
        <a href="#" onClick={(e) => (e.preventDefault(), openUrl("https://miniti.app/terms"))}>
          miniti.app/terms
        </a>
      </p>
      {error && <div className="banner error">{error}</div>}
      <button className="btn primary" onClick={accept}>
        I agree
      </button>
    </div>
  );
}

export function Onboarding({ onDone }: GateProps) {
  const [error, setError] = useState<string | null>(null);
  const [omarchy, setOmarchy] = useState<OmarchyStatus | null>(null);
  const [widgetBusy, setWidgetBusy] = useState(false);
  useEffect(() => { omarchyStatus().then(setOmarchy).catch(() => setOmarchy(null)); }, []);
  async function addWidget() {
    setWidgetBusy(true);
    setError(null);
    try { setOmarchy(await omarchyInstallWidget()); } catch (e) { setError(errorMessage(e)); } finally { setWidgetBusy(false); }
  }
  async function finish() {
    try {
      await completeOnboarding();
      onDone();
    } catch (e) {
      setError(errorMessage(e));
    }
  }
  return (
    <div className="gate">
      <h1 className="view-title">Welcome to miniti</h1>
      <ol className="steps">
        <li>
          <strong>Pick a mode</strong>: Managed (miniti backend, free minutes each month; your account
          is an anonymous recovery key you create next) or BYOK (your own Deepgram key).
        </li>
        <li>
          <strong>Check audio</strong> on Home: your microphone must be detected. System audio
          (remote callers) needs PipeWire with the pulse shim so miniti can read the monitor.
        </li>
        <li>
          <strong>Record</strong>, then review the transcript in History and your speaking habits in
          Coaching.
        </li>
      </ol>
      {omarchy?.is_omarchy && (
        <div className="omarchy-offer">
          <strong>You are on Omarchy.</strong> miniti has a bar widget: the recording timer in the bar, a panel
          with the questions worth asking, start and stop. It replaces the tray icon.
          {omarchy.widget_installed
            ? <p className="muted small">The widget is in your bar.</p>
            : <button className="btn" disabled={widgetBusy} onClick={addWidget}>{widgetBusy ? "adding…" : "Add the miniti widget to my bar"}</button>}
        </div>
      )}
      {error && <div className="banner error">{error}</div>}
      <button className="btn primary" onClick={finish}>
        Get started
      </button>
    </div>
  );
}
