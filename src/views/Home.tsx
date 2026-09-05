import { useEffect, useState } from "react";
import { environmentHealth, getDeviceId, hasBridge } from "../api";
import type { EnvHealth } from "../types";

interface HomeProps {
  onStart: () => void;
}

export function Home({ onStart }: HomeProps) {
  const [health, setHealth] = useState<EnvHealth | null>(null);
  const [device, setDevice] = useState<string>("");

  useEffect(() => {
    if (!hasBridge) return;
    environmentHealth().then(setHealth).catch(() => setHealth(null));
    getDeviceId().then(setDevice).catch(() => setDevice(""));
  }, []);

  return (
    <div className="view">
      <h1 className="view-title">Home</h1>
      <p className="muted">
        Native Linux meeting assistant. Start a meeting to capture mic (and system
        audio where available) and stream live transcription.
      </p>

      <button className="btn primary" onClick={onStart}>
        ● Start meeting
      </button>

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
            <Row k="device id" v={device || "—"} />
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
