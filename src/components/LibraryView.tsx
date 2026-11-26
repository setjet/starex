import { FolderInput, Play, Plus } from "./icons";
import { For, Show } from "solid-js";
import { playtime, timeAgo } from "../lib/format";
import { launch, query, select, setDialog, store, visibleGames } from "../lib/store";
import { GameCover } from "./GameArt";
import { StarMark } from "./StarMark";
import { Button } from "./ui/Button";

export function LibraryView() {
  const totalPlaytime = () => store.games.reduce((sum, g) => sum + g.playtimeSecs, 0);

  return (
    <Show when={store.games.length > 0} fallback={<EmptyLibrary />}>
      <div class="library page-enter">
        <header class="library-header">
          <div>
            <h1 class="library-title">Library</h1>
            <p class="library-subtitle">
              {store.games.length} {store.games.length === 1 ? "game" : "games"}
              <Show when={totalPlaytime() > 0}> · {playtime(totalPlaytime())} played</Show>
            </p>
          </div>
          <Button variant="primary" icon={<Plus />} onClick={() => setDialog({ type: "add" })}>
            Add game
          </Button>
        </header>

        <Show
          when={visibleGames().length > 0}
          fallback={<p class="library-no-results">No games match "{query()}".</p>}
        >
          <div class="library-grid">
            <For each={visibleGames()}>
              {(game, index) => (
                <article
                  class="library-card"
                  style={{ "--stagger": `${Math.min(index(), 12) * 24}ms` }}
                  data-running={store.running[game.id] ? "" : undefined}
                >
                  <button type="button" class="library-card-open" onClick={() => select(game.id)} aria-label={`Open ${game.name}`}>
                    <GameCover game={game} />
                  </button>
                  <button
                    type="button"
                    class="library-card-play"
                    aria-label={`Play ${game.name}`}
                    disabled={!!store.running[game.id] || store.launching[game.id]}
                    onClick={() => void launch(game.id)}
                  >
                    <Play />
                  </button>
                  <div class="library-card-meta">
                    {/* Generated covers already carry the name. */}
                    <Show when={game.artwork.cover}>
                      <span class="library-card-name">{game.name}</span>
                    </Show>
                    <span class="library-card-detail">
                      <Show
                        when={store.running[game.id]}
                        fallback={game.lastPlayed ? `Played ${timeAgo(game.lastPlayed)}` : "Never played"}
                      >
                        <span class="running-label">Playing now</span>
                      </Show>
                    </span>
                  </div>
                </article>
              )}
            </For>
          </div>
        </Show>
      </div>
    </Show>
  );
}

function EmptyLibrary() {
  return (
    <div class="empty page-enter">
      <div class="empty-mark" aria-hidden="true">
        <div class="empty-glow" />
        <StarMark size={44} />
      </div>
      <h1 class="empty-title">Your library is empty</h1>
      <p class="empty-text">
        Add a game folder and Starex finds the executable, checks the files and keeps track of your play time.
      </p>
      <Button variant="primary" size="lg" icon={<FolderInput />} onClick={() => setDialog({ type: "add" })} data-autofocus>
        Add a game
      </Button>
      <p class="empty-hint">You can also drop a game folder anywhere on this window.</p>
    </div>
  );
}
