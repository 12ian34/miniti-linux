import { useCallback, useEffect, useState } from "react";
import "./App.css";
import { hasBridge, launchGate } from "./api";
import type { LaunchGate } from "./types";
import { Home } from "./views/Home";
import { Recording } from "./views/Recording";
import { Coaching } from "./views/Coaching";
import { History } from "./views/History";
import { Settings } from "./views/Settings";
import { ForceUpdate, Onboarding, Terms } from "./views/Gates";

type View = "home" | "record" | "coaching" | "history" | "settings";

const NAV: { id: View; label: string }[] = [
  { id: "home", label: "Home" },
  { id: "record", label: "Record" },
  { id: "coaching", label: "Coaching" },
  { id: "history", label: "History" },
  { id: "settings", label: "Settings" },
];

function renderView(view: View, goRecord: () => void, gate: LaunchGate | null) {
  switch (view) {
    case "home":
      return <Home onStart={goRecord} gate={gate} />;
    case "record":
      return <Recording />;
    case "coaching":
      return <Coaching />;
    case "history":
      return <History />;
    case "settings":
      return <Settings />;
    default: {
      const _exhaustive: never = view;
      return _exhaustive;
    }
  }
}

function App() {
  const [view, setView] = useState<View>("home");
  const [gate, setGate] = useState<LaunchGate | null>(null);
  const [gateChecked, setGateChecked] = useState(!hasBridge);

  const refreshGate = useCallback(() => {
    if (!hasBridge) return;
    launchGate()
      .then(setGate)
      .catch(() => setGate(null))
      .finally(() => setGateChecked(true));
  }, []);

  useEffect(() => {
    refreshGate();
  }, [refreshGate]);

  if (!gateChecked) {
    return (
      <div className="gate">
        <span className="brand-mark">miniti</span>
        <p className="muted">starting…</p>
      </div>
    );
  }

  if (gate?.gate === "force_update") return <ForceUpdate gate={gate} />;
  if (gate?.gate === "terms") return <Terms gate={gate} onDone={refreshGate} />;
  if (gate?.gate === "onboarding") return <Onboarding gate={gate} onDone={refreshGate} />;

  return (
    <div className="app">
      <aside className="sidebar">
        <div className="brand">
          <span className="brand-mark">miniti</span>
          <span className="brand-sub">linux</span>
        </div>
        <nav className="nav">
          {NAV.map((item) => (
            <button
              key={item.id}
              className={`nav-item ${view === item.id ? "active" : ""}`}
              onClick={() => setView(item.id)}
            >
              {item.label}
            </button>
          ))}
        </nav>
        <div className="sidebar-foot">multi-dimensional meetings</div>
      </aside>
      <main className="content">{renderView(view, () => setView("record"), gate)}</main>
    </div>
  );
}

export default App;
