import React from "react";
import ReactDOM from "react-dom/client";
import { getCurrentWindow } from "@tauri-apps/api/window";
import App from "./App";
import GraphExplorer from "./GraphExplorer";
import SettingsWindow from "./SettingsWindow";

// All three windows (`tauri.conf.json`'s "query", "graph", and "settings"
// labels) load this same entry point — branching here by window label
// avoids a second Vite entry/router for what's still just three screens.
const label = getCurrentWindow().label;

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    {label === "graph" ? (
      <GraphExplorer />
    ) : label === "settings" ? (
      <SettingsWindow />
    ) : (
      <App />
    )}
  </React.StrictMode>,
);
