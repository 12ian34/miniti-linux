import { useState } from "react";
import "./App.css";
import { Home } from "./views/Home";
import { Recording } from "./views/Recording";
import { Coaching } from "./views/Coaching";
import { History } from "./views/History";
import { Settings } from "./views/Settings";

type View = "home" | "record" | "coaching" | "history" | "settings";

const NAV: { id: View; label: string }[] = [
  { id: "home", label: "Home" },
  { id: "record", label: "Record" },
  { id: "coaching", label: "Coaching" },
  { id: "history", label: "History" },
  { id: "settings", label: "Settings" },
];

function renderView(view: View, goRecord: () => void) {
  switch (view) {
    case "home":
      return <Home onStart={goRecord} />;
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
      <main className="content">{renderView(view, () => setView("record"))}</main>
    </div>
  );
}

export default App;
