import { useCallback, useEffect, useState } from "react";
import "./App.css";
import { hasBridge, launchGate } from "./api";
import type { LaunchGate } from "./types";
import { StoreProvider } from "./store";
import { Shell } from "./views/Shell";
import { ForceUpdate, Onboarding, Terms } from "./views/Gates";

function App() {
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
    <StoreProvider>
      <Shell gate={gate} />
    </StoreProvider>
  );
}

export default App;
