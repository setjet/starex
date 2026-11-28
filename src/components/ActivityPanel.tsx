import { createMemo, createSignal, For, Show } from "solid-js";
import { playtime } from "../lib/format";
import type { Game, PlaySession } from "../lib/types";

const DAY = 86400;
const dayName = new Intl.DateTimeFormat(undefined, { weekday: "short" });
const fullDate = new Intl.DateTimeFormat(undefined, { weekday: "long", day: "numeric", month: "short" });
const sessionDate = new Intl.DateTimeFormat(undefined, { day: "numeric", month: "short", hour: "numeric", minute: "2-digit" });

/** Midnight (local time) of the day `unix` falls on, in Unix seconds. */
function dayStart(unix: number): number {
  const d = new Date(unix * 1000);
  d.setHours(0, 0, 0, 0);
  return Math.floor(d.getTime() / 1000);
}

/** A bar per day for the last seven days, from play sessions of one game or many. */
export function WeekChart(props: { sessions: PlaySession[] }) {
  const [hovered, setHovered] = createSignal<number | null>(null);

  const days = createMemo(() => {
    const today = dayStart(Date.now() / 1000);
    const list = Array.from({ length: 7 }, (_, i) => ({ start: today - (6 - i) * DAY, seconds: 0 }));
    for (const s of props.sessions) {
      const bucket = list.find((d) => s.start >= d.start && s.start < d.start + DAY);
      if (bucket) bucket.seconds += Math.max(0, s.end - s.start);
    }
    return list;
  });
  const week = () => days().reduce((sum, d) => sum + d.seconds, 0);
  // A round top for the scale, in whole hours, at least one.
  const scale = () => Math.max(3600, Math.ceil(Math.max(...days().map((d) => d.seconds)) / 3600) * 3600);

  return (
    <figure class="activity-chart" aria-label={`Play time over the last 7 days, ${playtime(week())} in total`}>
      <div class="activity-plot">
        <div class="activity-grid" aria-hidden="true">
          <span data-label={`${scale() / 3600}h`} />
          <span data-label={`${scale() / 7200}h`} />
          <span data-label="0" />
        </div>
        <For each={days()}>
          {(day, i) => (
            <div
              class="activity-column"
              onPointerEnter={() => setHovered(i())}
              onPointerLeave={() => setHovered(null)}
              data-today={i() === 6 || undefined}
            >
              <Show when={hovered() === i()}>
                <div class="activity-tooltip" role="tooltip">
                  <span>{fullDate.format(day.start * 1000)}</span>
                  <strong>{day.seconds > 0 ? playtime(day.seconds) : "Not played"}</strong>
                </div>
              </Show>
              <div
                class="activity-bar"
                data-empty={day.seconds === 0 || undefined}
                style={{ height: day.seconds > 0 ? `max(4px, ${(day.seconds / scale()) * 100}%)` : "2px" }}
              />
            </div>
          )}
        </For>
      </div>
      <div class="activity-days" aria-hidden="true">
        <For each={days()}>{(day, i) => <span data-today={i() === 6 || undefined}>{i() === 6 ? "Today" : dayName.format(day.start * 1000)}</span>}</For>
      </div>
      {/* The same numbers as a table, for screen readers. */}
      <table class="sr-only">
        <caption>Play time per day</caption>
        <tbody>
          <For each={days()}>
            {(day) => (
              <tr>
                <th scope="row">{fullDate.format(day.start * 1000)}</th>
                <td>{playtime(day.seconds)}</td>
              </tr>
            )}
          </For>
        </tbody>
      </table>
    </figure>
  );
}

/** Play time over the last seven days, and the most recent sessions. */
export function ActivityPanel(props: { game: Game }) {
  const recent = () => [...props.game.history].reverse().slice(0, 6);

  return (
    <section class="panel activity" aria-labelledby="activity-title">
      <header class="panel-header-inline">
        <h2 id="activity-title" class="panel-eyebrow">
          Activity
        </h2>
      </header>

      <div class="activity-body">
        <WeekChart sessions={props.game.history} />

        <div class="activity-sessions">
          <h3 class="activity-sessions-title">Recent sessions</h3>
          <Show when={recent().length > 0} fallback={<p class="panel-note">Sessions appear here after you play.</p>}>
            <ul>
              <For each={recent()}>
                {(s) => (
                  <li class="activity-session">
                    <span>{sessionDate.format(s.start * 1000)}</span>
                  </li>
                )}
              </For>
            </ul>
          </Show>
        </div>
      </div>
    </section>
  );
}
