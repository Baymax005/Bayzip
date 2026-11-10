import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { SettingsWindow } from "./components/SettingsWindow";
import { ProgressWindow } from "./components/ProgressWindow";
import { MiniExtractWindow } from "./components/MiniExtractWindow";

const urlParams = new URLSearchParams(window.location.search);
const windowParam = urlParams.get("window");

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    {windowParam === "settings" ? (
      <SettingsWindow />
    ) : windowParam === "progress" ? (
      <ProgressWindow />
    ) : windowParam === "mini-extract" ? (
      <MiniExtractWindow />
    ) : (
      <App />
    )}
  </React.StrictMode>,
);

