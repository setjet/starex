import { createSignal, type JSX, Show } from "solid-js";
import { fileUrl } from "../lib/api";
import { generatedArt, monogram } from "../lib/art";
import type { Game } from "../lib/types";

/** An image that fades in once decoded and falls back when it fails to load. */
export function FadeImage(props: { src?: string; alt?: string; class?: string; fallback?: JSX.Element; draggable?: boolean }) {
  const [state, setState] = createSignal<"loading" | "ready" | "failed">("loading");
  return (
    <Show when={props.src && state() !== "failed"} fallback={props.fallback}>
      <img
        class={`fade-image ${props.class ?? ""}`}
        data-ready={state() === "ready" || undefined}
        src={props.src}
        alt={props.alt ?? ""}
        draggable={props.draggable ?? false}
        decoding="async"
        onLoad={() => setState("ready")}
        onError={() => setState("failed")}
      />
    </Show>
  );
}

/** Square icon: the game's icon, else a crop of its cover, else a monogram on generated art. */
export function GameIcon(props: { game: Game; size?: number; class?: string }) {
  const src = () => fileUrl(props.game.artwork.icon ?? props.game.artwork.cover ?? props.game.artwork.banner);
  return (
    <span
      class={`game-icon ${props.class ?? ""}`}
      style={{ "--icon-size": `${props.size ?? 24}px`, background: generatedArt(props.game.name, 1.1) }}
      aria-hidden="true"
    >
      <FadeImage src={src()} fallback={<span class="game-icon-monogram">{monogram(props.game.name)}</span>} />
    </span>
  );
}

/** Portrait cover for the library grid. */
export function GameCover(props: { game: Game }) {
  return (
    <div class="game-cover" style={{ background: generatedArt(props.game.name) }}>
      <FadeImage
        src={fileUrl(props.game.artwork.cover)}
        fallback={
          <Show
            when={props.game.artwork.banner}
            fallback={
              <div class="game-cover-generated">
                <span class="game-cover-title">{props.game.name}</span>
              </div>
            }
          >
            <FadeImage src={fileUrl(props.game.artwork.banner)} class="game-cover-banner-crop" />
            <div class="game-cover-generated">
              <span class="game-cover-title">{props.game.name}</span>
            </div>
          </Show>
        }
      />
    </div>
  );
}
