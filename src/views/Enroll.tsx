import { useState } from "react";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import { authCreateAccount, authRestoreAccount, errorMessage, getPrefs, setPrefs } from "../api";

type Step = "choose" | "created" | "restore";

/**
 * First managed launch: create or restore an anonymous recovery key
 * (roadmap P0.1). No email, no password; the key is the account.
 */
export function Enroll({ onDone, inline = false, keyringPending = false }: { onDone: () => void; inline?: boolean; keyringPending?: boolean }) {
  const wrap = inline ? "enroll-inline" : "gate";
  const [step, setStep] = useState<Step>("choose");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [recoveryKey, setRecoveryKey] = useState<string | null>(null);
  const [saved, setSaved] = useState(false);
  const [copied, setCopied] = useState(false);
  const [input, setInput] = useState("");

  async function create() {
    setBusy(true);
    setError(null);
    try {
      setRecoveryKey(await authCreateAccount());
      setStep("created");
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  }

  async function restore() {
    setBusy(true);
    setError(null);
    try {
      await authRestoreAccount(input);
      onDone();
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  }

  async function useByok() {
    try {
      const prefs = await getPrefs();
      await setPrefs({ ...prefs, app_mode: "byok" });
      onDone();
    } catch (e) {
      setError(errorMessage(e));
    }
  }

  async function copy() {
    if (!recoveryKey) return;
    try {
      await writeText(recoveryKey);
      setCopied(true);
    } catch {
      setCopied(false);
    }
  }

  if (step === "created" && recoveryKey) {
    return (
      <div className={wrap}>
        {!inline && <h1 className="view-title">Save your recovery key</h1>}
        <p>
          This key is your miniti account. It is the only way to restore your plan and devices on
          another computer, and it cannot be recovered if lost. Store it in your password manager.
        </p>
        <div className="recovery-key">{recoveryKey}</div>
        <div className="save-row">
          <button className="btn" onClick={copy}>{copied ? "copied" : "copy"}</button>
        </div>
        <label className="check">
          <input type="checkbox" checked={saved} onChange={(e) => setSaved(e.currentTarget.checked)} />
          <span>I have saved this recovery key somewhere safe.</span>
        </label>
        <p className="muted tiny">You can reveal or rotate it later in Settings → Account &amp; Plan.</p>
        <button className="btn primary" disabled={!saved} onClick={onDone}>Continue</button>
      </div>
    );
  }

  if (step === "restore") {
    return (
      <div className={wrap}>
        {!inline && <h1 className="view-title">Restore with recovery key</h1>}
        <p className="muted">Enter the key from your other device. It starts with M1 and has eight groups of four characters.</p>
        <input
          className="input"
          autoFocus
          value={input}
          onChange={(e) => setInput(e.currentTarget.value)}
          placeholder="M1-XXXX-XXXX-XXXX-XXXX-XXXX-XXXX-XXXX-XXXX"
          onKeyDown={(e) => { if (e.key === "Enter" && input.trim()) void restore(); }}
        />
        {error && <div className="banner error">{error}</div>}
        <div className="save-row">
          <button className="btn primary" disabled={busy || !input.trim()} onClick={restore}>{busy ? "restoring…" : "Restore"}</button>
          <button className="btn" disabled={busy} onClick={() => { setStep("choose"); setError(null); }}>Back</button>
        </div>
      </div>
    );
  }

  return (
    <div className={wrap}>
      {!inline && <h1 className="view-title">Set up managed mode</h1>}
      <p className="muted">
        Managed mode uses the miniti backend for transcription and insights. Instead of an email or
        password, your account is an anonymous recovery key that only you hold.
      </p>
      {keyringPending && (
        <div className="banner">
          Your keyring has not answered yet. If this computer was set up before, its credentials are
          probably still there: unlock the keyring (or wait a moment) and this screen goes away on its
          own. Creating a new key now would fail because the backend already knows this computer.
        </div>
      )}
      {error && <div className="banner error">{error}</div>}
      <div className="choice-list">
        <button className="choice" disabled={busy || keyringPending} onClick={create}>
          <strong>{busy ? "Creating…" : "Create recovery key"}</strong>
          <span>New account. Free minutes every month; upgrade to Pro any time.</span>
        </button>
        <button className="choice" disabled={busy} onClick={() => { setStep("restore"); setError(null); }}>
          <strong>Restore with recovery key</strong>
          <span>Add this computer to an account you already have.</span>
        </button>
        <button className="choice" disabled={busy} onClick={useByok}>
          <strong>Use my own keys instead</strong>
          <span>BYOK: your Deepgram and OpenAI keys, nothing goes through the miniti backend.</span>
        </button>
      </div>
    </div>
  );
}
