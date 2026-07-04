import { useEffect, useState, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";

interface Settings {
  models_root: string;
  input_device: string | null;
  shortcut: string;
  push_to_talk: boolean;
  injection_method: string;
  cleanup_enabled: boolean;
  cleanup_model: string;
  llama_server_path: string;
  cleanup_port: number;
}

// Map a browser KeyboardEvent to tauri-plugin-global-shortcut syntax
// (e.g. "alt+space", "cmd+shift+d"). Returns null for a modifier-only press.
function shortcutFromEvent(e: KeyboardEvent): string | null {
  const mods: string[] = [];
  if (e.metaKey) mods.push("cmd");
  if (e.ctrlKey) mods.push("ctrl");
  if (e.altKey) mods.push("alt");
  if (e.shiftKey) mods.push("shift");
  const key = e.key;
  if (["Meta", "Control", "Alt", "Shift"].includes(key)) return null;
  let main = key;
  if (key === " ") main = "space";
  else if (key.length === 1) main = key.toLowerCase();
  else main = key.toLowerCase();
  return [...mods, main].join("+");
}

function prettyShortcut(s: string): string {
  const map: Record<string, string> = {
    alt: "⌥", cmd: "⌘", super: "⌘", ctrl: "⌃",
    shift: "⇧", space: "Space",
  };
  return s.split("+").map((p) => map[p.toLowerCase()] ?? p.toUpperCase()).join(" ");
}

export default function SettingsPage() {
  const [settings, setSettings] = useState<Settings | null>(null);
  const [devices, setDevices] = useState<string[]>([]);
  const [capturing, setCapturing] = useState(false);
  const [status, setStatus] = useState<{ kind: "ok" | "err"; msg: string } | null>(null);

  useEffect(() => {
    invoke<Settings>("get_settings").then(setSettings).catch(console.error);
    invoke<string[]>("list_input_devices").then(setDevices).catch(console.error);
  }, []);

  const patch = (p: Partial<Settings>) => {
    setSettings((s) => (s ? { ...s, ...p } : s));
    setStatus(null);
  };

  // Capture the next key combination for the hotkey field.
  useEffect(() => {
    if (!capturing) return;
    const onKey = (e: KeyboardEvent) => {
      e.preventDefault();
      const sc = shortcutFromEvent(e);
      if (sc) {
        patch({ shortcut: sc });
        setCapturing(false);
      }
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [capturing]);

  const save = useCallback(async () => {
    if (!settings) return;
    try {
      await invoke("save_settings", { newSettings: settings });
      setStatus({ kind: "ok", msg: "Saved. Changes are live." });
    } catch (e) {
      setStatus({ kind: "err", msg: String(e) });
    }
  }, [settings]);

  if (!settings) return <main className="page"><p>Loading...</p></main>;

  return (
    <main className="page">
      <header className="page__masthead">
        <h1>fude</h1>
        <p className="page__tagline">Local voice dictation. Nothing leaves your machine.</p>
      </header>

      <section className="page__section">
        <h2>Shortcut</h2>
        <div className="field">
          <button
            className={`capture ${capturing ? "capture--active" : ""}`}
            onClick={() => setCapturing((c) => !c)}
          >
            {capturing ? "Press a key combination..." : prettyShortcut(settings.shortcut)}
          </button>
          <span className="field__hint">
            {capturing ? "Listening — press your keys" : "Click to change"}
          </span>
        </div>
      </section>

      <section className="page__section">
        <h2>Mode</h2>
        <div className="segmented">
          <button
            className={settings.push_to_talk ? "seg seg--on" : "seg"}
            onClick={() => patch({ push_to_talk: true })}
          >
            Push to talk
            <small>hold the key, release to type</small>
          </button>
          <button
            className={!settings.push_to_talk ? "seg seg--on" : "seg"}
            onClick={() => patch({ push_to_talk: false })}
          >
            Continuous
            <small>press once to start, again to stop</small>
          </button>
        </div>
      </section>

      <section className="page__section">
        <h2>Text placement</h2>
        <div className="field">
          <select
            value={settings.injection_method}
            onChange={(e) => patch({ injection_method: e.target.value })}
          >
            <option value="clipboard">Paste (works everywhere)</option>
            <option value="ax">Accessibility insert (no clipboard, some apps unsupported)</option>
            <option value="type">Simulated typing (slow)</option>
          </select>
          <span className="field__hint">
            Paste is the reliable default, including terminals and chat apps.
          </span>
        </div>
      </section>

      <section className="page__section">
        <h2>Microphone</h2>
        <div className="field">
          <select
            value={settings.input_device ?? ""}
            onChange={(e) => patch({ input_device: e.target.value || null })}
          >
            <option value="">System default</option>
            {devices.map((d) => (
              <option key={d} value={d}>{d}</option>
            ))}
          </select>
          <span className="field__hint">
            Pin a device so macOS Continuity does not hand the mic to a nearby iPhone.
          </span>
        </div>
      </section>

      <section className="page__section">
        <h2>Cleanup</h2>
        <label className="toggle">
          <input
            type="checkbox"
            checked={settings.cleanup_enabled}
            onChange={(e) => patch({ cleanup_enabled: e.target.checked })}
          />
          <span>Tidy up transcripts with the local model ({settings.cleanup_model.replace(/\.gguf$/, "")})</span>
        </label>
      </section>

      <footer className="page__actions">
        <button className="save" onClick={save}>Save changes</button>
        {status && (
          <span className={`status status--${status.kind}`}>{status.msg}</span>
        )}
      </footer>
    </main>
  );
}
