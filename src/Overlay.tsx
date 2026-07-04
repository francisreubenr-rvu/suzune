import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";

type DictationState =
  | { state: "idle" }
  | { state: "recording" }
  | { state: "processing" }
  | { state: "injected"; text: string; method: string }
  | { state: "error"; message: string };

const HIDE_AFTER_INJECTED_MS = 1400;
const HIDE_AFTER_ERROR_MS = 3800;

// The window itself is always visible (a hidden WKWebView never loads its
// JS). Idle renders nothing and the window ignores clicks; only the
// recording state accepts a click (to cancel).
export default function Overlay() {
  const [dictation, setDictation] = useState<DictationState>({ state: "idle" });
  const hideTimer = useRef<number | undefined>(undefined);

  useEffect(() => {
    const win = getCurrentWindow();
    void win.setIgnoreCursorEvents(true);
    const unlisten = listen<DictationState>("dictation-state", (event) => {
      const next = event.payload;
      window.clearTimeout(hideTimer.current);
      setDictation(next);
      void win.setIgnoreCursorEvents(next.state !== "recording");
      if (next.state === "injected" || next.state === "error") {
        hideTimer.current = window.setTimeout(
          () => setDictation({ state: "idle" }),
          next.state === "injected" ? HIDE_AFTER_INJECTED_MS : HIDE_AFTER_ERROR_MS,
        );
      }
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  if (dictation.state === "idle") return null;

  const cancel = () => {
    if (dictation.state === "recording") void invoke("cancel_dictation");
  };

  return (
    <div
      className={`pill pill--${dictation.state}`}
      onClick={cancel}
      title={dictation.state === "recording" ? "Click to cancel" : "fude"}
    >
      {dictation.state === "recording" && (
        <>
          <span className="pill__dot" />
          <span className="pill__label">listening</span>
        </>
      )}
      {dictation.state === "processing" && (
        <>
          <span className="pill__quill">✎</span>
          <span className="pill__label">writing</span>
        </>
      )}
      {dictation.state === "injected" && (
        <span className="pill__label pill__label--done">placed on the page</span>
      )}
      {dictation.state === "error" && (
        <span className="pill__label pill__label--error">{dictation.message}</span>
      )}
    </div>
  );
}
