import { ArrowDownUp, LayoutGrid, Plus, Search, Settings, Star, X } from "./icons";
import { For, Show } from "solid-js";
import {
  openContextMenu,
  query,
  saveSettings,
  select,
  selectedId,
  setDialog,
  setQuery,
  setShowHidden,
  showHidden,
  store,
  visibleGames,
} from "../lib/store";
import type { LibrarySort } from "../lib/types";
import { updater } from "../lib/updater";
import { GameIcon } from "./GameArt";
import { IconButton } from "./ui/Button";
import { Menu } from "./ui/Menu";

const sortLabels: Record<LibrarySort, string> = {
  recent: "Recently played",
  name: "Name",
  playtime: "Play time",
};

export function Sidebar(props: { searchRef: (el: HTMLInputElement) => void }) {
  return (
    <nav class="sidebar" aria-label="Library">
      <div class="sidebar-search">
        <Search class="sidebar-search-icon" />
        <input
          ref={props.searchRef}
          class="sidebar-search-input"
          type="search"
          placeholder="Search"
          spellcheck={false}
          value={query()}
          onInput={(e) => setQuery(e.currentTarget.value)}
          onKeyDown={(e) => {
            if (e.key === "Escape") {
              setQuery("");
              e.currentTarget.blur();
            }
            if (e.key === "Enter") {
              const first = visibleGames()[0];
              if (first) select(first.id);
            }
          }}
          aria-label="Search your library"
        />
        <Show when={query()}>
          <button type="button" class="sidebar-search-clear" aria-label="Clear search" onClick={() => setQuery("")}>
            <X />
          </button>
        </Show>
      </div>

      <button
        type="button"
        class="sidebar-item sidebar-home"
        data-active={selectedId() === null || undefined}
        onClick={() => select(null)}
      >
        <span class="sidebar-home-icon">
          <LayoutGrid />
        </span>
        <span class="sidebar-item-name">Library</span>
        <span class="sidebar-count">{store.games.length}</span>
      </button>

      <div class="sidebar-section">
        <span>Games</span>
        <Menu
          items={() => [
            ...(Object.keys(sortLabels) as LibrarySort[]).map((sort) => ({
              label: sortLabels[sort],
              icon: store.settings.librarySort === sort ? <CheckDot /> : <span class="menu-icon-space" />,
              onSelect: () => void saveSettings({ librarySort: sort }),
            })),
            {
              label: "Show hidden games",
              icon: showHidden() ? <CheckDot /> : <span class="menu-icon-space" />,
              separatorBefore: true,
              onSelect: () => setShowHidden(!showHidden()),
            },
          ]}
          align="start"
          trigger={(p) => (
            <IconButton label={`Sort: ${sortLabels[store.settings.librarySort]}`} size="sm" {...p}>
              <ArrowDownUp />
            </IconButton>
          )}
        />
      </div>

      <div class="sidebar-list scroll-fade">
        <For
          each={visibleGames()}
          fallback={
            <Show when={store.games.length > 0}>
              <p class="sidebar-empty">{query() ? `No games match "${query()}".` : "All your games are hidden."}</p>
            </Show>
          }
        >
          {(game) => (
            <button
              type="button"
              class="sidebar-item"
              data-active={selectedId() === game.id || undefined}
              data-hidden={game.hidden || undefined}
              onClick={() => select(game.id)}
              onContextMenu={(e) => openContextMenu(game.id, e)}
              onDblClick={() => select(game.id)}
            >
              <GameIcon game={game} size={24} />
              <span class="sidebar-item-name">{game.name}</span>
              <Show
                when={store.running[game.id]}
                fallback={
                  <Show when={game.favorite}>
                    <Star class="sidebar-favorite" aria-label="Favorite" />
                  </Show>
                }
              >
                <span class="sidebar-running" aria-label="Running" />
              </Show>
            </button>
          )}
        </For>
      </div>

      <div class="sidebar-footer">
        <button type="button" class="sidebar-add" onClick={() => setDialog({ type: "add" })}>
          <Plus />
          <span>Add game</span>
          <kbd>Ctrl N</kbd>
        </button>
        <div class="sidebar-footer-row">
          <IconButton label="Settings (Ctrl+,)" size="sm" onClick={() => setDialog({ type: "settings" })}>
            <Settings />
          </IconButton>
          <button type="button" class="sidebar-version" onClick={() => setDialog({ type: "settings", section: "updates" })}>
            Starex {updater.appVersion()}
            <Show when={updater.available()}>
              <span class="sidebar-version-dot" aria-label="Update available" />
            </Show>
          </button>
        </div>
      </div>
    </nav>
  );
}

function CheckDot() {
  return <span class="menu-check" aria-hidden="true" />;
}
