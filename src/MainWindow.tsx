import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import SettingsPage from "./SettingsPage";
import SetupPage from "./SetupPage";

// The main window is either the first-run setup screen (while models
// download) or the settings page. It asks the backend which to show.
export default function MainWindow() {
  const [mode, setMode] = useState<"loading" | "setup" | "settings">("loading");

  useEffect(() => {
    invoke<boolean>("needs_setup")
      .then((needs) => setMode(needs ? "setup" : "settings"))
      .catch(() => setMode("settings"));
  }, []);

  if (mode === "loading") return <main className="page" />;
  if (mode === "setup") return <SetupPage onDone={() => setMode("settings")} />;
  return <SettingsPage />;
}
