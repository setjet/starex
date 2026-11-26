import { getCurrentWebview } from "@tauri-apps/api/webview";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { CircleCheck, CircleX, FolderInput, Info, TriangleAlert } from "./components/icons";
import { createEffect, createSignal, ErrorBoundary, For, type JSX, Match, onCleanup, onMount, Show, Switch } from "solid-js";
import { AddGameDialog } from "./components/AddGameDialog";
import { GameView } from "./components/GameView";
import { LibraryView } from "./components/LibraryView";
import { PropertiesDialog } from "./components/PropertiesDialog";
import { SettingsDialog } from "./components/SettingsDialog";
import { Sidebar } from "./components/Sidebar";
import { RemoveDialog, SavesDialog, ThreatsDialog, UpdateDialog } from "./components/SmallDialogs";
import { TitleBar } from "./components/TitleBar";
import { errorText } from "./lib/api";
import { dialog, init, launch, selectedGame, setDialog, store } from "./lib/store";
import { dismiss, toast, toasts } from "./lib/toast";

export function App() {
  let search!: HTMLInputElement;
  const [dropping, setDropping] = createSignal(false);
  const [failed, setFailed] = createSignal("");

  onMount(async () => {
    try {
      await init();
    } catch (e) {
      setFailed(errorText(e));
    }
    // The window starts hidden so it never flashes white; show it once the first frame is ready.
    requestAnimationFrame(() => void getCurrentWindow().show());

    const unlisten = await getCurrentWebview().onDragDropEvent((event) => {
      const kind = event.payload.type;
      if (kind === "enter" || kind === "over") setDropping(true);
      else setDropping(false);
      if (kind === "drop") {
        const path = event.payload.paths[0];
        if (path && !dialog()) setDialog({ type: "add", path });
        else if (path) toast("Close the open dialog first, then drop the folder again.");
      }
    });
    onCleanup(unlisten);
  });

  const onKey = (e: KeyboardEvent) => {
    const mod = e.ctrlKey || e.metaKey;
    if (dialog()) return;
    if (mod && e.key.toLowerCase() === "f") {
      e.preventDefault();
      search.focus();
      search.select();
    } else if (mod && e.key.toLowerCase() === "n") {
      e.preventDefault();
      setDialog({ type: "add" });
    } else if (mod && e.key === ",") {
      e.preventDefault();
      setDialog({ type: "settings" });
    } else if (mod && e.key === "Enter") {
      const game = selectedGame();
      if (game) void launch(game.id);
    }
  };
  document.addEventListener("keydown", onKey);
  onCleanup(() => document.removeEventListener("keydown", onKey));

  // Disable the webview's own context menu and reload shortcuts in release builds.
  if (!import.meta.env.DEV) {
    const block = (e: Event) => {
      const target = e.target as HTMLElement;
      if (!target.closest("input, textarea")) e.preventDefault();
    };
    document.addEventListener("contextmenu", block);
    onCleanup(() => document.removeEventListener("contextmenu", block));
  }

  createEffect(() => {
    document.documentElement.toggleAttribute("data-reduce-motion", store.settings.reduceMotion);
  });

  return (
    <div class="app" data-os={store.hostOs || undefined}>
      <TitleBar />
      <div class="shell">
        <Sidebar searchRef={(el) => (search = el)} />
        <main class="content" data-ready={store.ready || undefined}>
          <Show when={!failed()} fallback={<StartupError message={failed()} />}>
            <Show when={store.ready}>
              <Guard>
                <Show when={selectedGame()} keyed fallback={<LibraryView />}>
                  {(game) => <GameView game={game} />}
                </Show>
              </Guard>
            </Show>
          </Show>
        </main>
      </div>

      <Show when={dropping()}>
        <div class="drop-overlay" aria-hidden="true">
          <div class="drop-card">
            <FolderInput />
            <span>Drop to add this game</span>
          </div>
        </div>
      </Show>

      <Show when={dialog()} keyed>
        {(d) => (
          <Guard onReset={() => setDialog(null)}>
          <Switch>
            <Match when={d.type === "add" && d}>{(a) => <AddGameDialog initialPath={a().path} />}</Match>
            <Match when={d.type === "settings" && d}>{(s) => <SettingsDialog section={s().section} />}</Match>
            <Match when={d.type === "properties" && d}>
              {(p) => (
                <Show when={store.games.find((g) => g.id === p().id)} keyed>
                  {(game) => <PropertiesDialog game={game} tab={p().tab} />}
                </Show>
              )}
            </Match>
            <Match when={d.type === "remove" && d}>
              {(r) => (
                <Show when={store.games.find((g) => g.id === r().id)} keyed>
                  {(game) => <RemoveDialog game={game} />}
                </Show>
              )}
            </Match>
            <Match when={d.type === "saves" && d}>
              {(r) => (
                <Show when={store.games.find((g) => g.id === r().id)} keyed>
                  {(game) => <SavesDialog game={game} />}
                </Show>
              )}
            </Match>
            <Match when={d.type === "threats" && d}>
              {(r) => (
                <Show when={store.games.find((g) => g.id === r().id)} keyed>
                  {(game) => <ThreatsDialog game={game} />}
                </Show>
              )}
            </Match>
            <Match when={d.type === "update"}>
              <UpdateDialog />
            </Match>
          </Switch>
          </Guard>
        )}
      </Show>

      <div class="toasts" role="status" aria-live="polite">
        <For each={toasts()}>
          {(t) => (
            <div class="toast" data-tone={t.tone} data-leaving={t.leaving || undefined} onClick={() => dismiss(t.id)}>
              <Switch fallback={<Info class="toast-icon" />}>
                <Match when={t.tone === "success"}>
                  <CircleCheck class="toast-icon" />
                </Match>
                <Match when={t.tone === "warning"}>
                  <TriangleAlert class="toast-icon" />
                </Match>
                <Match when={t.tone === "danger"}>
                  <CircleX class="toast-icon" />
                </Match>
              </Switch>
              <div class="toast-copy">
                <span class="toast-title">{t.title}</span>
                <Show when={t.detail}>
                  <span class="toast-detail">{t.detail}</span>
                </Show>
              </div>
            </div>
          )}
        </For>
      </div>
    </div>
  );
}

/** Keeps one broken view from blanking the whole window. */
function Guard(props: { children: JSX.Element; onReset?: () => void }) {
  return (
    <ErrorBoundary
      fallback={(error, reset) => {
        console.error(error);
        return (
          <div class="empty">
            <h1 class="empty-title">This view ran into a problem</h1>
            <p class="empty-text">{errorText(error)}</p>
            <button
              type="button"
              class="btn btn-secondary btn-md"
              onClick={() => {
                props.onReset?.();
                reset();
              }}
            >
              Try again
            </button>
          </div>
        );
      }}
    >
      {props.children}
    </ErrorBoundary>
  );
}

function StartupError(props: { message: string }) {
  return (
    <div class="empty">
      <h1 class="empty-title">Starex could not open your library</h1>
      <p class="empty-text">{props.message}</p>
    </div>
  );
}
