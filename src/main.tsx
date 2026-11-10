import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { SettingsWindow } from "./components/SettingsWindow";
import { ProgressWindow } from "./components/ProgressWindow";

const urlParams = new URLSearchParams(window.location.search);
const windowParam = urlParams.get("window");

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    {windowParam === "settings" ? (
      <SettingsWindow />
    ) : windowParam === "progress" ? (
      <ProgressWindow />
    ) : (
      <App />
    )}
  </React.StrictMode>,
);

