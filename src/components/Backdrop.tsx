import { Match, Show, Switch } from "solid-js";
import { fileUrl } from "../lib/api";
import { selectedGame, store } from "../lib/store";
import { FadeImage } from "./GameArt";

/** What fills the window behind the library and game pages (Settings > Appearance). */
export function Backdrop() {
  const mode = () => store.settings.backdrop;
  // The selected game's art, or the most recently played game's on the library page.
  const gameArt = () => {
    const game =
      selectedGame() ??
      [...store.games].filter((g) => !g.hidden).sort((a, b) => (b.lastPlayed ?? 0) - (a.lastPlayed ?? 0))[0];
    return fileUrl(game?.artwork.banner ?? game?.artwork.cover);
  };

  return (
    <Show when={mode() !== "none"}>
      <div
        class="backdrop"
        data-mode={mode()}
        style={{ "--backdrop-strength": String(store.settings.backdropStrength / 100) }}
        aria-hidden="true"
      >
        <Switch>
          <Match when={mode() === "custom" && store.settings.backdropImage}>
            <Show when={fileUrl(store.settings.backdropImage)} keyed>
              {(src) => <FadeImage src={src} class="backdrop-image" />}
            </Show>
          </Match>
          <Match when={mode() === "game" && gameArt()}>
            <Show when={gameArt()} keyed>
              {(src) => <FadeImage src={src} class="backdrop-image backdrop-blur" />}
            </Show>
          </Match>
          <Match when={true}>
            <div class="backdrop-aurora" />
          </Match>
        </Switch>
        <div class="backdrop-dim" />
      </div>
    </Show>
  );
}
