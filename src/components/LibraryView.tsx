import { Play, Plus } from "./icons";
import { For, Show } from "solid-js";
import { generatedArt } from "../lib/art";
import { timeAgo } from "../lib/format";
import { launch, openContextMenu, query, select, setDialog, store, visibleGames } from "../lib/store";
import { GameCover } from "./GameArt";
import { StarMark } from "./StarMark";
import { Button } from "./ui/Button";

export function LibraryView() {

  return (
    <Show when={store.games.length > 0} fallback={<EmptyLibrary />}>
      <div class="library page-enter">
        <header class="library-header">
          <div>
            <h1 class="library-title">Library</h1>
            <p class="library-subtitle">
              {store.games.length} {store.games.length === 1 ? "game" : "games"}
            </p>
          </div>
          <Button variant="primary" icon={<Plus />} onClick={() => setDialog({ type: "add" })}>
            Add game
          </Button>
        </header>

        <Show
          when={visibleGames().length > 0}
          fallback={<p class="library-no-results">{query() ? `No games match "${query()}".` : "No games in this view."}</p>}
        >
          <div class="library-grid">
            <For each={visibleGames()}>
              {(game, index) => (
                <article
                  onContextMenu={(e) => openContextMenu(game.id, e)}
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

/** First run: a fan of covers previews the library this screen is waiting for. */
function EmptyLibrary() {
  return (
    <div class="onboard">
      <div class="onboard-fan" aria-hidden="true">
        <div class="onboard-cover onboard-cover-left" style={{ background: generatedArt("Starex left") }} />
        <div class="onboard-cover onboard-cover-right" style={{ background: generatedArt("Starex right") }} />
        <div class="onboard-cover onboard-cover-center" style={{ background: generatedArt("Starex") }}>
          <StarMark size={56} />
        </div>
      </div>

      <h1 class="onboard-title">Build your library</h1>
      <p class="onboard-text">
        Point Starex at any game folder. It finds the game, checks every file and keeps your saves safe.
      </p>

      <Button variant="primary" size="lg" icon={<Plus />} onClick={() => setDialog({ type: "add" })} data-autofocus>
        Add a game
      </Button>
      <p class="onboard-hint">
        <kbd>Ctrl</kbd>
        <kbd>N</kbd>
        <span>or drop a folder anywhere</span>
      </p>
    </div>
  );
}
