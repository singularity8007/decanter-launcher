import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import App from "./App";
import "./styles.css";

// The webview's own context menu (Reload, Inspect...) doesn't belong in an app.
if (!import.meta.env.DEV) {
  window.addEventListener("contextmenu", (e) => e.preventDefault());
}

// In a plain browser during development, stand in for the Rust backend.
if (import.meta.env.DEV && !("__TAURI_INTERNALS__" in window)) {
  const { installMock } = await import("./mock");
  installMock();
}

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
