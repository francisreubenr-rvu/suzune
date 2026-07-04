import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";

type SetupEvent =
  | { stage: "downloading"; name: string; received: number; total: number }
  | { stage: "extracting"; name: string }
  | { stage: "done" }
  | { stage: "error"; message: string };

function mb(bytes: number): string {
  return (bytes / (1024 * 1024)).toFixed(0);
}

// First-run screen: the app fetches on-device models once, with progress.
// Calls onDone when finished so the shell can swap to the settings page.
export default function SetupPage({ onDone }: { onDone: () => void }) {
  const [ev, setEv] = useState<SetupEvent | null>(null);

  useEffect(() => {
    const unlisten = listen<SetupEvent>("model-setup", (e) => {
      setEv(e.payload);
      if (e.payload.stage === "done") setTimeout(onDone, 800);
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, [onDone]);

  const pct =
    ev && ev.stage === "downloading" && ev.total > 0
      ? Math.min(100, Math.round((ev.received / ev.total) * 100))
      : null;

  return (
    <main className="page setup">
      <header className="page__masthead">
        <h1>fude</h1>
        <p className="page__tagline">Setting up for first use.</p>
      </header>

      <section className="page__section">
        <p className="setup__lede">
          fude runs entirely on your Mac, so it needs to fetch its speech
          and cleanup models once. This happens now and never again.
        </p>

        <div className="setup__status">
          {!ev && <span className="setup__line">Preparing...</span>}
          {ev?.stage === "downloading" && (
            <>
              <span className="setup__line">
                Downloading {ev.name} — {mb(ev.received)}
                {ev.total > 0 ? ` / ${mb(ev.total)}` : ""} MB
              </span>
              <div className="setup__bar">
                <div
                  className="setup__fill"
                  style={{ width: pct === null ? "40%" : `${pct}%` }}
                />
              </div>
            </>
          )}
          {ev?.stage === "extracting" && (
            <span className="setup__line">Unpacking {ev.name}...</span>
          )}
          {ev?.stage === "done" && (
            <span className="setup__line setup__line--ok">
              Ready. fude lives in your menu bar now.
            </span>
          )}
          {ev?.stage === "error" && (
            <span className="setup__line setup__line--err">
              Setup failed: {ev.message}. Check your connection and reopen fude.
            </span>
          )}
        </div>
      </section>
    </main>
  );
}
