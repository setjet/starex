import { createEffect, createMemo, createSignal, For, type JSX, on, Show } from "solid-js";
import { timeAgo } from "../lib/format";
import { lastPlayedOf, launch, select, setBigPicture, setDialog, store } from "../lib/store";
import type { Game } from "../lib/types";
import { GameIcon } from "./GameArt";
import { Gamepad2, LayoutGrid, Plus, Search, Settings } from "./icons";
import { Dialog } from "./ui/Dialog";

interface Action {
  label: string;
  icon: () => JSX.Element;
  run: () => void;
}

type Item = { kind: "game"; game: Game } | { kind: "action"; action: Action };

const actions: Action[] = [
  {
    label: "Go to Library",
    icon: () => <LayoutGrid />,
    run: () => {
      setDialog(null);
      select(null);
    },
  },
  { label: "Add a game", icon: () => <Plus />, run: () => setDialog({ type: "add" }) },
  {
    label: "Big Picture mode",
    icon: () => <Gamepad2 />,
    run: () => {
      setDialog(null);
      setBigPicture(true);
    },
  },
  { label: "Settings", icon: () => <Settings />, run: () => setDialog({ type: "settings" }) },
];

/** How well `text` matches `query`: higher is better, -1 is no match. Whole words beat letters in order. */
function score(text: string, query: string): number {
  const t = text.toLowerCase();
  const at = t.indexOf(query);
  if (at === 0) return 300;
  if (at > 0) return (/\s/.test(t[at - 1] ?? "") ? 200 : 100) - at;
  let i = 0;
  for (const c of t) if (c === query[i]) i++;
  return i === query.length ? 10 : -1;
}

/** Ctrl+K: jump to any game or action by typing. */
export function CommandPalette() {
  let list: HTMLDivElement | undefined;
  const [q, setQ] = createSignal("");
  const [active, setActive] = createSignal(0);

  const items = createMemo<Item[]>(() => {
    const query = q().trim().toLowerCase();
    const games = query
      ? store.games
          .map((game) => ({ game, s: score(game.name, query) }))
          .filter((x) => x.s >= 0)
          .sort((a, b) => b.s - a.s)
          .slice(0, 8)
          .map((x) => x.game)
      : [...store.games]
          .filter((g) => !g.hidden)
          .sort((a, b) => (lastPlayedOf(b) ?? b.addedAt) - (lastPlayedOf(a) ?? a.addedAt))
          .slice(0, 5);
    const matched = query ? actions.filter((a) => score(a.label, query) >= 0) : actions;
    return [
      ...games.map((game): Item => ({ kind: "game", game })),
      ...matched.map((action): Item => ({ kind: "action", action })),
    ];
  });
  const gameCount = () => items().filter((i) => i.kind === "game").length;

  createEffect(on(q, () => setActive(0)));
  createEffect(() => {
    active();
    list?.querySelector("[data-active]")?.scrollIntoView({ block: "nearest" });
  });

  const run = (item: Item | undefined, play: boolean) => {
    if (!item) return;
    if (item.kind === "action") {
      item.action.run();
      return;
    }
    setDialog(null);
    select(item.game.id);
    if (play) void launch(item.game.id);
  };

  const onKey = (e: KeyboardEvent) => {
    const n = items().length;
    if (e.key === "ArrowDown" && n > 0) {
      e.preventDefault();
      setActive((active() + 1) % n);
    } else if (e.key === "ArrowUp" && n > 0) {
      e.preventDefault();
      setActive((active() - 1 + n) % n);
    } else if (e.key === "Enter") {
      e.preventDefault();
      run(items()[active()], e.ctrlKey || e.metaKey);
    }
  };

  return (
    <Dialog title="Search" onClose={() => setDialog(null)} size="md" bare class="palette">
      <div class="palette-search">
        <Search />
        <input
          class="palette-input"
          placeholder="Search games and actions"
          value={q()}
          spellcheck={false}
          autocomplete="off"
          onInput={(e) => setQ(e.currentTarget.value)}
          onKeyDown={onKey}
          aria-label="Search games and actions"
          data-autofocus
        />
      </div>
      <div class="palette-list" ref={list} role="listbox" aria-label="Results">
        <Show when={items().length > 0} fallback={<p class="palette-empty">Nothing matches "{q()}".</p>}>
          <For each={items()}>
            {(item, i) => (
              <>
                <Show when={i() === 0 && item.kind === "game"}>
                  <p class="palette-group">{q().trim() ? "Games" : "Recent"}</p>
                </Show>
                <Show when={i() === gameCount() && item.kind === "action"}>
                  <p class="palette-group">Actions</p>
                </Show>
                <button
                  type="button"
                  role="option"
                  class="palette-item"
                  aria-selected={active() === i()}
                  data-active={active() === i() || undefined}
                  onPointerMove={() => setActive(i())}
                  onClick={(e) => run(item, e.ctrlKey || e.metaKey)}
                >
                  <Show
                    when={item.kind === "game" && item.game}
                    fallback={<span class="palette-item-icon">{item.kind === "action" && item.action.icon()}</span>}
                  >
                    {(game) => <GameIcon game={game()} size={28} />}
                  </Show>
                  <span class="palette-item-copy">
                    <span class="palette-item-name">{item.kind === "game" ? item.game.name : item.action.label}</span>
                    <Show when={item.kind === "game" && item.game}>
                      {(game) => (
                        <span class="palette-item-detail">
                          {store.running[game().id]
                            ? "Playing now"
                            : lastPlayedOf(game())
                              ? `Played ${timeAgo(lastPlayedOf(game()) ?? 0)}`
                              : "Never played"}
                        </span>
                      )}
                    </Show>
                  </span>
                  <Show when={item.kind === "game"}>
                    <span class="palette-item-hint">
                      <kbd>Ctrl Enter</kbd>Play
                    </span>
                  </Show>
                </button>
              </>
            )}
          </For>
        </Show>
      </div>
      <footer class="palette-footer">
        <span>
          <kbd>↑↓</kbd>Move
        </span>
        <span>
          <kbd>Enter</kbd>Open
        </span>
        <span>
          <kbd>Ctrl Enter</kbd>Play
        </span>
        <span>
          <kbd>Esc</kbd>Close
        </span>
      </footer>
    </Dialog>
  );
}
