import { useEffect, useState } from "react";
import { environmentHealth, hasBridge } from "../api";
import type { EnvHealth, LaunchGate } from "../types";

interface HomeProps {
  onStart: () => void;
  gate: LaunchGate | null;
}

export function Home({ onStart, gate }: HomeProps) {
  const [health, setHealth] = useState<EnvHealth | null>(null);

  useEffect(() => {
    if (!hasBridge) return;
    environmentHealth().then(setHealth).catch(() => setHealth(null));
  }, []);

  return (
    <div className="view">
      <h1 className="view-title">Home</h1>
      <p className="muted">
        Native Linux meeting assistant. Start a meeting to capture your microphone
        and stream live transcription.
      </p>

      <button className="btn primary" onClick={onStart}>
        ● Start meeting
      </button>

      {gate && !gate.backend_reachable && (
        <div className="banner warn">
          Backend not reachable: {gate.backend_error ?? "unknown error"}. BYOK still works.
        </div>
      )}

      <section className="card">
        <h2 className="card-title">Environment</h2>
        {health ? (
          <div className="rows">
            <Row k="app" v={`${health.app_name} v${health.app_version}`} />
            <Row k="X-Platform" v={health.platform} />
            <Row k="os" v={health.os} />
            <Row k="pcm" v={health.pcm_contract} />
            <Row
              k="microphone"
              v={health.microphone_available ? "available" : "not detected"}
              ok={health.microphone_available}
            />
            <Row
              k="system audio"
              v={health.system_audio_available ? "available (PipeWire/Pulse)" : "not detected"}
              ok={health.system_audio_available}
            />
            <Row
              k="managed mode"
              v={health.backend_key_present ? "backend key present" : "unavailable in this build (BYOK only)"}
              ok={health.backend_key_present}
            />
            <Row k="device id" v={health.device_id || "—"} />
          </div>
        ) : (
          <p className="muted">
            {hasBridge ? "Loading…" : "Browser preview — native bridge unavailable."}
          </p>
        )}
      </section>
    </div>
  );
}

function Row({ k, v, ok }: { k: string; v: string; ok?: boolean }) {
  return (
    <div className="row-line">
      {ok !== undefined && <span className={`dot ${ok ? "dot-ok" : "dot-warn"}`} />}
      <span className="k">{k}</span>
      <span className="v">{v}</span>
    </div>
  );
}
