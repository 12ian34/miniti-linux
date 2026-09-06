import { useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { acceptTerms, completeOnboarding, errorMessage } from "../api";
import type { LaunchGate } from "../types";

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
        Arch: <code>pacman -Syu miniti-bin</code> (or your AUR helper). Other distros: download the
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
        Miniti records audio from your microphone (and, where enabled, system audio), streams it to
        Deepgram for transcription, and in managed mode sends transcripts to the Miniti backend for
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
      <h1 className="view-title">Welcome to Miniti</h1>
      <ol className="steps">
        <li>
          <strong>Pick a mode</strong>: Managed (Miniti backend, free minutes each month; your account
          is an anonymous recovery key you create next) or BYOK (your own Deepgram key).
        </li>
        <li>
          <strong>Check audio</strong> on Home: your microphone must be detected. System audio
          (remote callers) needs PipeWire with the pulse shim so Miniti can read the monitor.
        </li>
        <li>
          <strong>Record</strong>, then review the transcript in History and your speaking habits in
          Coaching.
        </li>
      </ol>
      {error && <div className="banner error">{error}</div>}
      <button className="btn primary" onClick={finish}>
        Get started
      </button>
    </div>
  );
}
