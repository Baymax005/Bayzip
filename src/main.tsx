import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { SettingsWindow } from "./components/SettingsWindow";

const urlParams = new URLSearchParams(window.location.search);
const isSettingsWindow = urlParams.get("window") === "settings";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    {isSettingsWindow ? <SettingsWindow /> : <App />}
  </React.StrictMode>,
);

