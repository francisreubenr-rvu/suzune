import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

interface Settings {
  models_root: string;
  shortcut: string;
  push_to_talk: boolean;
  cleanup_enabled: boolean;
  cleanup_model: string;
  llama_server_path: string;
  cleanup_port: number;
}

export default function SettingsPage() {
  const [settings, setSettings] = useState<Settings | null>(null);

  useEffect(() => {
    invoke<Settings>("get_settings").then(setSettings).catch(console.error);
  }, []);

  return (
    <main className="page">
      <header className="page__masthead">
        <h1>whispr</h1>
        <p className="page__tagline">
          Local voice dictation. Nothing leaves your machine.
        </p>
      </header>

      <section className="page__section">
        <h2>How to dictate</h2>
        <p>
          Hold <kbd>{settings ? prettyShortcut(settings.shortcut) : "…"}</kbd>,
          speak, release. The cleaned text lands wherever your cursor is.
          {settings && !settings.push_to_talk && " (Toggle mode: press once to start, again to stop.)"}
        </p>
      </section>

      <section className="page__section">
        <h2>Current configuration</h2>
        {settings ? (
          <table className="page__table">
            <tbody>
              <tr><th>Shortcut</th><td>{prettyShortcut(settings.shortcut)}</td></tr>
              <tr><th>Mode</th><td>{settings.push_to_talk ? "push to talk" : "toggle"}</td></tr>
              <tr><th>Cleanup pass</th><td>{settings.cleanup_enabled ? `on — ${settings.cleanup_model}` : "off"}</td></tr>
              <tr><th>Models folder</th><td>{settings.models_root}</td></tr>
            </tbody>
          </table>
        ) : (
          <p>Loading…</p>
        )}
        <p className="page__hint">
          Edit <code>settings.json</code> in the app config folder to change
          these, then restart whispr. In-app editing arrives in a later
          version.
        </p>
      </section>
    </main>
  );
}

function prettyShortcut(s: string): string {
  return s
    .split("+")
    .map((part) => ({ alt: "⌥", cmd: "⌘", super: "⌘", ctrl: "⌃", shift: "⇧", space: "Space" }[part.toLowerCase()] ?? part))
    .join(" ");
}
