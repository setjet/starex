import { createResource, createSignal, For, Show } from "solid-js";
import { api, errorText } from "../lib/api";
import { timeAgo } from "../lib/format";
import { setDialog, store } from "../lib/store";
import type { Achievement, Achievements, Game, NewsItem } from "../lib/types";
import { FadeImage } from "./GameArt";
import { Lock, Trophy } from "./icons";
import { Button } from "./ui/Button";
import { Progress } from "./ui/Spinner";

/** Answers are kept for ten minutes, so moving between games does not ask Steam again. */
const KEEP_MS = 10 * 60 * 1000;
const cache = new Map<string, { at: number; value: Promise<unknown> }>();

function cached<T>(key: string, load: () => Promise<T>): Promise<T> {
  const hit = cache.get(key);
  if (hit && Date.now() - hit.at < KEEP_MS) return hit.value as Promise<T>;
  const value = load();
  cache.set(key, { at: Date.now(), value });
  value.catch(() => cache.delete(key));
  return value;
}

type Loaded<T> = { ok: true; value: T } | { ok: false; error: string };

async function settle<T>(work: Promise<T>): Promise<Loaded<T>> {
  try {
    return { ok: true, value: await work };
  } catch (e) {
    return { ok: false, error: errorText(e) };
  }
}

const notSteam = (r: Loaded<unknown> | undefined) => !r || (!r.ok && r.error.startsWith("This is not a Steam game"));

/** Achievements and the latest news, for games from Steam. Other games show neither. */
export function SteamPanels(props: { game: Game }) {
  const [news] = createResource(
    () => props.game.id,
    (id) => settle(cached(`news:${id}`, () => api.steamNews(id))),
  );
  // Settings can change the key; ask again when they do.
  const [achievements] = createResource(
    () => ({ id: props.game.id, key: store.settings.steamApiKey, steamId: store.settings.steamId }),
    (s) => settle(cached(`achievements:${s.id}:${s.key}:${s.steamId}`, () => api.steamAchievements(s.id))),
  );

  return (
    <Show when={!notSteam(news())}>
      <Show when={achievements()}>{(a) => <AchievementsPanel result={a()} />}</Show>
      <Show when={news()?.ok && news()}>
        {(n) => <NewsPanel game={props.game} items={(n() as { ok: true; value: NewsItem[] }).value} />}
      </Show>
    </Show>
  );
}

function AchievementsPanel(props: { result: Loaded<Achievements> }) {
  const [open, setOpen] = createSignal(false);
  const data = () => (props.result.ok ? props.result.value : null);
  const shown = () => {
    const items = data()?.items ?? [];
    return open() ? items : items.slice(0, 6);
  };

  return (
    <Show when={!props.result.ok || (data()?.total ?? 0) > 0}>
      <section class="panel achievements" aria-labelledby="achievements-title">
        <header class="panel-header-inline">
          <h2 id="achievements-title" class="panel-eyebrow">
            Achievements
          </h2>
          <Show when={(data()?.items.length ?? 0) > 6}>
            <Button variant="ghost" size="sm" onClick={() => setOpen((v) => !v)}>
              {open() ? "Show less" : "Show all"}
            </Button>
          </Show>
        </header>
        <Show
          when={data()}
          fallback={
            <div class="baseline">
              <Trophy class="baseline-icon" />
              <div class="baseline-copy">
                <span class="baseline-title">Achievements are not available yet</span>
                <span class="baseline-detail">{props.result.ok ? "" : props.result.error}</span>
              </div>
              <Button
                size="sm"
                variant="ghost"
                class="baseline-action"
                onClick={() => setDialog({ type: "settings", section: "integrations" })}
              >
                Open Settings
              </Button>
            </div>
          }
        >
          {(d) => (
            <>
              <div class="achievements-progress">
                <div class="achievements-count">
                  <strong>{d().unlocked}</strong>
                  <span>
                    of {d().total} unlocked · {Math.round((d().unlocked / Math.max(1, d().total)) * 100)}%
                  </span>
                </div>
                <Progress value={d().unlocked / Math.max(1, d().total)} />
              </div>
              <div class="achievements-grid">
                <For each={shown()}>{(a) => <AchievementRow achievement={a} />}</For>
              </div>
            </>
          )}
        </Show>
      </section>
    </Show>
  );
}

function AchievementRow(props: { achievement: Achievement }) {
  const a = () => props.achievement;
  const detail = () => {
    if (a().achieved && a().unlockedAt) return `Unlocked ${timeAgo(a().unlockedAt ?? 0)}`;
    if (a().percent !== null) return `${(a().percent ?? 0).toFixed(1)}% of players`;
    return a().description || "Locked";
  };
  return (
    <div class="achievement" data-locked={!a().achieved || undefined} title={a().description || undefined}>
      <span class="achievement-icon">
        <FadeImage src={(a().achieved ? a().icon : a().iconGray) || undefined} fallback={a().achieved ? <Trophy /> : <Lock />} />
      </span>
      <span class="achievement-copy">
        <span class="achievement-name">{a().name}</span>
        <span class="achievement-detail">{detail()}</span>
      </span>
    </div>
  );
}

function NewsPanel(props: { game: Game; items: NewsItem[] }) {
  return (
    <Show when={props.items.length > 0}>
      <section class="panel news" aria-labelledby="news-title">
        <header class="panel-header-inline">
          <h2 id="news-title" class="panel-eyebrow">
            News
          </h2>
        </header>
        <div class="news-list">
          <For each={props.items.slice(0, 3)}>
            {(item) => (
              <button type="button" class="news-item" onClick={() => void api.openSteamNews(props.game.id, item.gid)}>
                <span class="news-title">{item.title}</span>
                <span class="news-meta">
                  {timeAgo(item.date)}
                  {item.source ? ` · ${item.source}` : ""}
                </span>
                <Show when={item.summary}>
                  <span class="news-summary">{item.summary}</span>
                </Show>
              </button>
            )}
          </For>
        </div>
      </section>
    </Show>
  );
}
