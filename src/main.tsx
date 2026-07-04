import React from "react";
import ReactDOM from "react-dom/client";
import { getCurrentWindow } from "@tauri-apps/api/window";
import Overlay from "./Overlay";
import MainWindow from "./MainWindow";
import "./App.css";

const label = getCurrentWindow().label;

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    {label === "overlay" ? <Overlay /> : <MainWindow />}
  </React.StrictMode>,
);
