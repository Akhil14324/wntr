import React from "react";
import ReactDOM from "react-dom/client";
import { getCurrentWindow } from "@tauri-apps/api/window";
import "./index.css";
import { initStore } from "./lib/store";
import Notch from "./notch/Notch";
import SettingsApp from "./settings/Settings";

const label = getCurrentWindow().label;
const isSettings = label === "settings";
document.body.classList.add(isSettings ? "settings" : "notch");

// The webview's own context menu makes no sense in a notch.
window.addEventListener("contextmenu", (e) => {
  const t = e.target as HTMLElement;
  if (!t.closest("input, textarea")) e.preventDefault();
});

initStore();

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>{isSettings ? <SettingsApp /> : <Notch />}</React.StrictMode>,
);
