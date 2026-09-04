import React from "react";
import ReactDOM from "react-dom/client";
import { invoke } from "@tauri-apps/api/core";
import App from "./App";
import "./styles.css";

// A crash in here leaves a blank window and nothing else: the webview console
// is unreachable from a packaged app. Forward it to the Rust logs instead.
function report(message: string) {
  invoke("log_frontend_error", { message }).catch(() => {});
}

window.addEventListener("error", (event) => {
  report(`${event.message} (${event.filename}:${event.lineno}:${event.colno})`);
});

window.addEventListener("unhandledrejection", (event) => {
  report(`Unhandled rejection: ${event.reason}`);
});

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>
);
