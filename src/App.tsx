import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import "./App.css";

interface EnvHealth {
  app_name: string;
  app_version: string;
  platform: string;
  pcm_contract: string;
  os: string;
  tauri_bridge: boolean;
}

// Phase 0 skeleton: proves the dev toolchain (Vite + React) and the Tauri
// Rust<->WebView bridge are wired up. Audio/Deepgram/insights land in later
// phases (see PLAN.md §13).
function App() {
  const [health, setHealth] = useState<EnvHealth | null>(null);
  const [error, setError] = useState<string | null>(null);

  const hasBridge = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

  useEffect(() => {
    if (!hasBridge) {
      return;
    }
    invoke<EnvHealth>("environment_health")
      .then(setHealth)
      .catch((e) => setError(String(e)));
  }, [hasBridge]);

  return (
    <main className="shell">
      <header className="brand">
        <span className="brand-mark">miniti</span>
        <span className="brand-sub">multi-dimensional meetings · linux</span>
      </header>

      <section className="panel">
        <h1 className="panel-title">Phase 0 · environment check</h1>
        <p className="panel-note">
          Native Linux client (Tauri 2 + Rust). This screen verifies the dev
          toolchain and the Rust&nbsp;&harr;&nbsp;WebView bridge before audio and
          Deepgram work begins.
        </p>

        <ul className="status-list">
          <li className="status-row">
            <span className="dot dot-ok" />
            <span className="k">frontend</span>
            <span className="v">Vite + React + TypeScript</span>
          </li>
          <li className="status-row">
            <span className={`dot ${hasBridge ? "dot-ok" : "dot-warn"}`} />
            <span className="k">tauri bridge</span>
            <span className="v">
              {hasBridge ? "connected" : "browser preview (no native bridge)"}
            </span>
          </li>
          {error && (
            <li className="status-row">
              <span className="dot dot-err" />
              <span className="k">error</span>
              <span className="v">{error}</span>
            </li>
          )}
        </ul>

        {health && (
          <div className="health">
            <div className="health-row">
              <span className="k">app</span>
              <span className="v">
                {health.app_name} v{health.app_version}
              </span>
            </div>
            <div className="health-row">
              <span className="k">X-Platform</span>
              <span className="v">{health.platform}</span>
            </div>
            <div className="health-row">
              <span className="k">os</span>
              <span className="v">{health.os}</span>
            </div>
            <div className="health-row">
              <span className="k">pcm</span>
              <span className="v">{health.pcm_contract}</span>
            </div>
          </div>
        )}
      </section>

      <footer className="foot">
        dark-only · terminal-adjacent · tokens to be ported from ColorPalette
      </footer>
    </main>
  );
}

export default App;
