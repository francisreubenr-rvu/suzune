import { useEffect, useState, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

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
  grammar_level: string;
  tone: string;
  personalization_enabled: boolean;
}

interface HistoryEntry {
  id: number;
  raw: string;
  cleaned: string;
  ts: number;
  tone?: string;
}

interface CorrectionRecord {
  id: number;
  ts: number;
  raw: string;
  cleaned: string;
  corrected: string;
  tone?: string;
}

const GRAMMAR_LEVELS: { value: string; label: string; hint: string }[] = [
  { value: "butler", label: "Butler", hint: "barely touches your words" },
  { value: "casual", label: "Casual", hint: "light cleanup, keeps your voice" },
  { value: "standard", label: "Standard", hint: "fixes grammar, keeps contractions" },
  { value: "formal", label: "Formal", hint: "expands contractions, drops casual openers" },
  { value: "oxford", label: "Oxford", hint: "maximally correct written English" },
];

const TONES: { value: string; label: string; hint: string }[] = [
  { value: "neutral", label: "Neutral", hint: "no restyle, fastest" },
  { value: "playful", label: "Playful", hint: "a little warmth and humor" },
  { value: "enthusiastic", label: "Enthusiastic", hint: "upbeat energy" },
  { value: "direct", label: "Direct", hint: "terse, strips hedging" },
  { value: "dramatic", label: "Dramatic", hint: "maximum flair" },
];

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
  // Mirrors the last-persisted settings (as of load / successful save), so the
  // UI can tell "toggled locally" apart from "actually saved to disk".
  const [savedSettings, setSavedSettings] = useState<Settings | null>(null);
  const [devices, setDevices] = useState<string[]>([]);
  const [capturing, setCapturing] = useState(false);
  const [captureStuck, setCaptureStuck] = useState(false);
  const [status, setStatus] = useState<{ kind: "ok" | "err"; msg: string } | null>(null);

  const [history, setHistory] = useState<HistoryEntry[]>([]);
  const [corrections, setCorrections] = useState<CorrectionRecord[]>([]);
  const [fixingId, setFixingId] = useState<number | null>(null);
  const [fixText, setFixText] = useState("");

  useEffect(() => {
    invoke<Settings>("get_settings").then((s) => {
      setSettings(s);
      setSavedSettings(s);
    }).catch(console.error);
    invoke<string[]>("list_input_devices").then(setDevices).catch(console.error);
  }, []);

  const patch = (p: Partial<Settings>) => {
    setSettings((s) => (s ? { ...s, ...p } : s));
    setStatus(null);
  };

  // Capture the next key combination for the hotkey field. Escape cancels
  // without changing anything — and since some combinations never reach the
  // webview at all (many Cmd+Shift+<letter> combos are already claimed by
  // macOS itself, or by another app's own global shortcut, before the
  // keystroke is ever delivered here), a stuck "Listening..." with no way
  // out used to be the only outcome. A timeout now surfaces that possibility
  // instead of leaving the user stuck.
  useEffect(() => {
    if (!capturing) {
      setCaptureStuck(false);
      return;
    }
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        setCapturing(false);
        return;
      }
      e.preventDefault();
      const sc = shortcutFromEvent(e);
      if (sc) {
        patch({ shortcut: sc });
        setCapturing(false);
      }
    };
    // Losing window focus mid-capture (e.g. Cmd+Tab, clicking another app)
    // means the eventual keydown, if any, won't be the user's intended
    // combination for this app — cancel rather than staying stuck listening.
    const onBlur = () => setCapturing(false);
    window.addEventListener("keydown", onKey, true);
    window.addEventListener("blur", onBlur);
    const stuckTimer = window.setTimeout(() => setCaptureStuck(true), 4000);
    return () => {
      window.removeEventListener("keydown", onKey, true);
      window.removeEventListener("blur", onBlur);
      window.clearTimeout(stuckTimer);
    };
  }, [capturing]);

  const save = useCallback(async () => {
    if (!settings) return;
    try {
      await invoke("save_settings", { newSettings: settings });
      setSavedSettings(settings);
      setStatus({ kind: "ok", msg: "Saved. Changes are live." });
    } catch (e) {
      setStatus({ kind: "err", msg: String(e) });
    }
  }, [settings]);

  const refreshHistory = useCallback(() => {
    invoke<HistoryEntry[]>("get_recent_history").then(setHistory).catch(console.error);
  }, []);

  const refreshCorrections = useCallback(() => {
    invoke<CorrectionRecord[]>("list_corrections").then(setCorrections).catch(console.error);
  }, []);

  // Personalization is opt-in: only fetch/subscribe once the user has
  // turned it on, and nothing here writes anything to disk by itself.
  useEffect(() => {
    if (!settings?.personalization_enabled) return;
    refreshHistory();
    refreshCorrections();
    const unlisten = listen("history-updated", () => refreshHistory());
    return () => {
      unlisten.then((fn) => fn());
    };
  }, [settings?.personalization_enabled, refreshHistory, refreshCorrections]);

  const startFix = (h: HistoryEntry) => {
    if (fixingId !== null && fixingId !== h.id) {
      const current = history.find((e) => e.id === fixingId);
      const draftDirty = !!current && fixText.trim() !== current.cleaned.trim();
      if (draftDirty && !window.confirm("Discard your unsaved correction?")) {
        return;
      }
    }
    setFixingId(h.id);
    setFixText(h.cleaned);
  };

  const cancelFix = () => {
    setFixingId(null);
    setFixText("");
  };

  const submitFix = async (id: number) => {
    const entry = history.find((h) => h.id === id);
    const trimmed = fixText.trim();
    if (!entry || trimmed === "" || trimmed === entry.cleaned.trim()) {
      cancelFix();
      return;
    }
    try {
      await invoke("submit_correction", { historyId: id, correctedText: trimmed });
      setFixingId(null);
      setFixText("");
      refreshCorrections();
    } catch (e) {
      setStatus({ kind: "err", msg: String(e) });
    }
  };

  const clearAllCorrections = async () => {
    if (!window.confirm("Delete all saved corrections? This cannot be undone.")) return;
    try {
      await invoke("clear_corrections");
      refreshCorrections();
    } catch (e) {
      setStatus({ kind: "err", msg: String(e) });
    }
  };

  if (!settings) return <main className="page"><p>Loading...</p></main>;

  // True while the toggle reads on locally but the persisted setting hasn't
  // caught up — the backend won't actually record anything until Save runs.
  const personalizationUnsaved =
    settings.personalization_enabled && !savedSettings?.personalization_enabled;

  return (
    <main className="page">
      <header className="page__masthead">
        <h1>suzune</h1>
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
            {capturing
              ? "Listening — press your keys, or Esc to cancel"
              : "Click to change"}
          </span>
          {capturing && captureStuck && (
            <span className="field__hint field__hint--warn">
              Still nothing? Some key combinations are already claimed by
              macOS itself or another app, so this app never sees them —
              try a different combination, or press Esc to cancel.
            </span>
          )}
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

      <section className="page__section">
        <h2>Grammar strictness</h2>
        <div className="segmented">
          {GRAMMAR_LEVELS.map(({ value, label, hint }) => (
            <button
              key={value}
              className={settings.grammar_level === value ? "seg seg--on" : "seg"}
              onClick={() => patch({ grammar_level: value })}
            >
              {label}
              <small>{hint}</small>
            </button>
          ))}
        </div>
      </section>

      <section className="page__section">
        <h2>Tone</h2>
        <div className="segmented">
          {TONES.map(({ value, label, hint }) => (
            <button
              key={value}
              className={settings.tone === value ? "seg seg--on" : "seg"}
              onClick={() => patch({ tone: value })}
            >
              {label}
              <small>{hint}</small>
            </button>
          ))}
        </div>
      </section>

      <section className="page__section">
        <h2>History &amp; personalization</h2>
        <label className="toggle">
          <input
            type="checkbox"
            checked={settings.personalization_enabled}
            onChange={(e) => patch({ personalization_enabled: e.target.checked })}
          />
          <span>
            Remember recent dictations so you can fix mistakes, and let suzune learn
            your corrections over time. Off by default — nothing is stored unless you
            actually correct something, it never leaves your machine, and you can
            clear it anytime below.
          </span>
        </label>

        {settings.personalization_enabled && (
          <div className="history">
            {personalizationUnsaved && (
              <p className="field__hint field__hint--warn">
                Click "Save changes" below to activate — nothing is recorded until saved.
              </p>
            )}
            <h3 className="page__subhead">Recent dictations</h3>
            {history.length === 0 ? (
              !personalizationUnsaved && (
                <p className="history-empty">Nothing yet — dictate something and it will show up here.</p>
              )
            ) : (
              <div className="history-list">
                {history.slice().reverse().map((h) => (
                  <div className="history-item" key={h.id}>
                    <div className="history-item__text">{h.cleaned}</div>
                    {fixingId === h.id ? (
                      <div className="history-item__fix">
                        <textarea
                          value={fixText}
                          onChange={(e) => setFixText(e.target.value)}
                          rows={2}
                        />
                        <div className="history-item__actions">
                          <button
                            className="link-btn"
                            onClick={() => submitFix(h.id)}
                            disabled={fixText.trim() === "" || fixText.trim() === h.cleaned.trim()}
                          >
                            Save correction
                          </button>
                          <button className="link-btn" onClick={cancelFix}>Cancel</button>
                        </div>
                      </div>
                    ) : (
                      <div className="history-item__actions">
                        <button className="link-btn" onClick={() => startFix(h)}>Fix this</button>
                      </div>
                    )}
                  </div>
                ))}
              </div>
            )}

            <h3 className="page__subhead">Your corrections ({corrections.length})</h3>
            {corrections.length === 0 ? (
              <p className="history-empty">No corrections saved yet.</p>
            ) : (
              <div className="corrections-list">
                {corrections.slice().reverse().map((c) => (
                  <div className="corrections-list__item" key={`${c.ts}-${c.id}`}>
                    "{c.cleaned}" → "{c.corrected}"
                  </div>
                ))}
              </div>
            )}
            {corrections.length > 0 && (
              <button className="danger" onClick={clearAllCorrections}>Clear all corrections</button>
            )}
          </div>
        )}
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
