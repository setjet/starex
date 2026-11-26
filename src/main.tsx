import "@fontsource-variable/inter";
import "./styles/tokens.css";
import "./styles/base.css";
import "./styles/controls.css";
import "./styles/shell.css";
import "./styles/game.css";
import "./styles/dialogs.css";

import { render } from "solid-js/web";
import { App } from "./App";
import { errorText } from "./lib/api";
import { toast } from "./lib/toast";

// Last line of defence: anything that slips past an action's own handling becomes a readable
// message instead of a silent failure.
window.addEventListener("unhandledrejection", (event) => {
  event.preventDefault();
  console.error(event.reason);
  toast("Something went wrong", { tone: "danger", detail: errorText(event.reason) });
});
window.addEventListener("error", (event) => {
  console.error(event.error ?? event.message);
  toast("Something went wrong", { tone: "danger", detail: errorText(event.error ?? event.message) });
});

async function start() {
  // In a plain browser during development, run against a demo library instead of Tauri.
  if (import.meta.env.DEV && !("__TAURI_INTERNALS__" in window)) {
    const { installDemo } = await import("./dev/demo");
    installDemo();
  }
  const root = document.getElementById("root");
  if (root) render(() => <App />, root);
}

void start();
