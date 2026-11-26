import { getCurrentWindow } from "@tauri-apps/api/window";
import { ArrowDownToLine } from "./icons";
import { createSignal, onCleanup, onMount, Show } from "solid-js";
import { setDialog } from "../lib/store";
import { updater } from "../lib/updater";
import { StarMark } from "./StarMark";

/** The window's own title bar: drag area, update notice and window controls. */
export function TitleBar() {
  const win = getCurrentWindow();
  const [maximized, setMaximized] = createSignal(false);

  onMount(async () => {
    setMaximized(await win.isMaximized());
    const unlisten = await win.onResized(async () => setMaximized(await win.isMaximized()));
    onCleanup(unlisten);
  });

  return (
    <header class="titlebar" data-tauri-drag-region>
      <div class="titlebar-brand" data-tauri-drag-region>
        <StarMark size={14} class="titlebar-mark" />
        <span class="titlebar-name" data-tauri-drag-region>
          Starex
        </span>
      </div>
      <div class="titlebar-spacer" data-tauri-drag-region />
      <Show when={updater.available()}>
        {(update) => (
          <button type="button" class="titlebar-update" onClick={() => setDialog({ type: "update" })}>
            <ArrowDownToLine />
            <span>Update to {update().version}</span>
          </button>
        )}
      </Show>
      <div class="window-controls">
        <button type="button" class="window-control" aria-label="Minimize" onClick={() => void win.minimize()}>
          <svg viewBox="0 0 10 10" aria-hidden="true">
            <path d="M1 5h8" />
          </svg>
        </button>
        <button
          type="button"
          class="window-control"
          aria-label={maximized() ? "Restore" : "Maximize"}
          onClick={() => void win.toggleMaximize()}
        >
          <Show
            when={maximized()}
            fallback={
              <svg viewBox="0 0 10 10" aria-hidden="true">
                <rect x="1.5" y="1.5" width="7" height="7" rx="1" />
              </svg>
            }
          >
            <svg viewBox="0 0 10 10" aria-hidden="true">
              <rect x="1.5" y="3" width="5.5" height="5.5" rx="1" />
              <path d="M3.5 1.5h4a1 1 0 0 1 1 1v4" />
            </svg>
          </Show>
        </button>
        <button type="button" class="window-control window-close" aria-label="Close" onClick={() => void win.close()}>
          <svg viewBox="0 0 10 10" aria-hidden="true">
            <path d="M1.5 1.5l7 7M8.5 1.5l-7 7" />
          </svg>
        </button>
      </div>
    </header>
  );
}
