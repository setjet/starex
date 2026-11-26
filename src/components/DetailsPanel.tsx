import { createSignal, For, Show } from "solid-js";
import { playtime } from "../lib/format";
import type { GameDetails } from "../lib/types";

const year = new Intl.DateTimeFormat(undefined, { year: "numeric", month: "long", day: "numeric" });

/** About the game, from IGDB: a description and the facts that matter before playing. */
export function DetailsPanel(props: { details: GameDetails }) {
  const [open, setOpen] = createSignal(false);
  const d = () => props.details;
  const long = () => (d().summary?.length ?? 0) > 280;

  return (
    <section class="panel details" aria-labelledby="details-title">
      <h2 id="details-title" class="panel-eyebrow">
        About
      </h2>
      <Show when={d().summary}>
        {(summary) => (
          <div class="details-summary-wrap">
            <p class="details-summary" data-open={open() || !long() || undefined}>
              {summary()}
            </p>
            <Show when={long()}>
              <button type="button" class="check-toggle details-more" onClick={() => setOpen((v) => !v)}>
                {open() ? "Less" : "More"}
              </button>
            </Show>
          </div>
        )}
      </Show>
      <dl class="details-facts">
        <Show when={d().genres.length > 0}>
          <div>
            <dt>Genre</dt>
            <dd class="details-genres">
              <For each={d().genres.slice(0, 4)}>{(g) => <span class="badge">{g}</span>}</For>
            </dd>
          </div>
        </Show>
        <Show when={d().releaseDate}>
          {(at) => (
            <div>
              <dt>Released</dt>
              <dd>{year.format(at() * 1000)}</dd>
            </div>
          )}
        </Show>
        <Show when={d().developer}>
          {(dev) => (
            <div>
              <dt>Developer</dt>
              <dd>{dev()}</dd>
            </div>
          )}
        </Show>
        <Show when={d().timeToBeat || d().timeToComplete}>
          <div>
            <dt>Time to beat</dt>
            <dd>
              <Show when={d().timeToBeat}>{(t) => <>{playtime(t())} story</>}</Show>
              <Show when={d().timeToBeat && d().timeToComplete}> · </Show>
              <Show when={d().timeToComplete}>{(t) => <>{playtime(t())} everything</>}</Show>
            </dd>
          </div>
        </Show>
      </dl>
    </section>
  );
}
