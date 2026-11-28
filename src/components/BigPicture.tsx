import { getCurrentWindow } from "@tauri-apps/api/window";
import { createMemo, createSignal, For, onCleanup, onMount, Show } from "solid-js";
import { fileUrl } from "../lib/api";
import { generatedArt } from "../lib/art";
import { clock, playtime, timeAgo } from "../lib/format";
import { dialog, lastPlayedOf, launch, playtimeOf, select, selectedId, setBigPicture, stop, store } from "../lib/store";
import { FadeImage, GameCover } from "./GameArt";
import { Play, Square } from "./icons";
import { StarMark } from "./StarMark";
import { Spinner } from "./ui/Spinner";

/** Standard gamepad buttons (the layout every browser maps controllers to). */
const PAD = { a: 0, b: 1, x: 2, lb: 4, rb: 5, start: 9, left: 14, right: 15 } as const;
/** Holding a direction repeats after this delay, then at this rate. */
const REPEAT_DELAY = 360;
const REPEAT_RATE = 110;
/** How long the dark cover takes to fade over (or off) the desktop view around the fullscreen switch. */
const COVER_MS = 220;
/** How long the view takes to fade to dark when leaving. */
const LEAVE_MS = 200;

const wait = (ms: number) => new Promise<void>((r) => setTimeout(r, ms));
const windowSize = () => `${innerWidth}x${innerHeight}`;

const timeOfDay = new Intl.DateTimeFormat(undefined, { hour: "numeric", minute: "2-digit" });

/**
 * Big Picture: a fullscreen shelf of covers for the couch. Arrows or the d-pad browse, A or
 * Enter plays, B or Escape leaves, X stops a running game.
 */
export function BigPicture() {
  const win = getCurrentWindow();
  const games = createMemo(() =>
    store.games
      .filter((g) => !g.hidden)
      .sort((a, b) => {
        if (a.favorite !== b.favorite) return a.favorite ? -1 : 1;
        return (lastPlayedOf(b) ?? b.addedAt) - (lastPlayedOf(a) ?? a.addedAt);
      }),
  );
  const start = Math.max(0, games().findIndex((g) => g.id === selectedId()));
  const [index, setIndex] = createSignal(start);
  const [now, setNow] = createSignal(Date.now() / 1000);
  const current = () => games()[Math.min(index(), games().length - 1)];
  const running = () => {
    const g = current();
    return g ? store.running[g.id] : undefined;
  };

  const move = (by: number) => {
    const n = games().length;
    if (n > 0) setIndex(Math.max(0, Math.min(n - 1, index() + by)));
  };
  const play = () => {
    const g = current();
    if (g && !store.running[g.id]) void launch(g.id);
  };
  const halt = () => {
    const g = current();
    if (g && store.running[g.id]) void stop(g.id);
  };
  // Fullscreen while open; back to the window as it was when closed. The window's jumps in and out
  // of fullscreen can't be animated, so they happen behind a dark cover. Going in, the view
  // animates in only once the window has settled at its new size. Leaving, the view fades to dark,
  // the window shrinks back behind it, and then the dark cover fades away.
  let wentFullscreen = false;
  let closed = false;
  const [ready, setReady] = createSignal(false);
  const [leaving, setLeaving] = createSignal<"" | "fade" | "reveal">("");
  let entering: Promise<void> = Promise.resolve();
  const enter = async () => {
    if (await win.isFullscreen().catch(() => false)) return;
    await wait(COVER_MS);
    if (closed || leaving()) return;
    const before = windowSize();
    wentFullscreen = true;
    await win.setFullscreen(true).catch(() => undefined);
    await windowSettled(before);
  };
  onMount(() => {
    entering = enter().then(() => {
      if (!leaving()) setReady(true);
    });
  });
  onCleanup(() => {
    closed = true;
    if (wentFullscreen) void win.setFullscreen(false).catch(() => undefined);
  });

  const exit = async () => {
    if (leaving()) return;
    setLeaving("fade");
    await Promise.all([entering, wait(LEAVE_MS)]);
    if (wentFullscreen) {
      const before = windowSize();
      wentFullscreen = false;
      await win.setFullscreen(false).catch(() => undefined);
      await windowSettled(before);
    }
    const g = current();
    if (g) select(g.id);
    setLeaving("reveal");
    await wait(COVER_MS);
    setBigPicture(false);
  };

  const timer = setInterval(() => setNow(Date.now() / 1000), 1000);
  onCleanup(() => clearInterval(timer));

  const onKey = (e: KeyboardEvent) => {
    if (dialog() || leaving() || e.ctrlKey || e.metaKey || e.altKey) return;
    const keys: Record<string, () => void> = {
      ArrowLeft: () => move(-1),
      ArrowRight: () => move(1),
      Home: () => setIndex(0),
      End: () => setIndex(games().length - 1),
      Enter: play,
      " ": play,
      Escape: exit,
    };
    const action = keys[e.key];
    if (action) {
      e.preventDefault();
      action();
    }
  };
  document.addEventListener("keydown", onKey);
  onCleanup(() => document.removeEventListener("keydown", onKey));

  // Controllers have no events for buttons: read them every frame and act on new presses.
  let frame = 0;
  let held: boolean[] = [];
  let direction = 0;
  let nextRepeat = 0;
  const poll = (time: number) => {
    frame = requestAnimationFrame(poll);
    if (dialog() || leaving()) return;
    const pads = navigator.getGamepads?.() ?? [];
    const pressed: boolean[] = [];
    let axis = 0;
    for (const pad of pads) {
      if (!pad) continue;
      pad.buttons.forEach((b, i) => {
        pressed[i] = pressed[i] || b.pressed;
      });
      const x = pad.axes[0] ?? 0;
      if (Math.abs(x) > Math.abs(axis)) axis = x;
    }
    const tapped = (button: number) => pressed[button] && !held[button];
    if (tapped(PAD.a) || tapped(PAD.start)) play();
    if (tapped(PAD.b)) void exit();
    if (tapped(PAD.x)) halt();
    if (tapped(PAD.lb)) move(-5);
    if (tapped(PAD.rb)) move(5);

    const dir = pressed[PAD.left] || axis < -0.5 ? -1 : pressed[PAD.right] || axis > 0.5 ? 1 : 0;
    if (dir !== direction) {
      direction = dir;
      if (dir !== 0) {
        move(dir);
        nextRepeat = time + REPEAT_DELAY;
      }
    } else if (dir !== 0 && time >= nextRepeat) {
      move(dir);
      nextRepeat = time + REPEAT_RATE;
    }
    held = pressed;
  };
  frame = requestAnimationFrame(poll);
  onCleanup(() => cancelAnimationFrame(frame));

  return (
    <div class="bp" role="dialog" aria-modal="true" aria-label="Big Picture"
      data-ready={ready() ? "" : undefined}
      data-leaving={leaving() || undefined}
    >
      <div class="bp-scene" aria-hidden="true">
        <Show when={current()} keyed>
          {(g) => (
            <div class="bp-backdrop" style={{ background: generatedArt(g.name, 1.35) }}>
              <FadeImage src={fileUrl(g.artwork.banner ?? g.artwork.cover)} class="bp-art" />
            </div>
          )}
        </Show>
        <div class="bp-scrim" />
      </div>

      <header class="bp-top">
        <div class="bp-brand" aria-label="Starex">
          <StarMark size={34} onDark />
        </div>
        <div class="bp-top-right">
          <span class="bp-clock">{timeOfDay.format(now() * 1000)}</span>
          <button type="button" class="bp-exit" onClick={exit}>
            <span class="bp-key">B</span>
            Exit
          </button>
        </div>
      </header>

      <Show
        when={current()}
        fallback={
          <main class="bp-main">
            <h1 class="bp-title">Your library is empty</h1>
            <p class="bp-meta">Add a game from the desktop view first.</p>
          </main>
        }
      >
        {(g) => (
          <main class="bp-main">
            <Show when={g().artwork.logo} fallback={<h1 class="bp-title">{g().name}</h1>}>
              <h1 class="bp-logo-wrap">
                <FadeImage
                  src={fileUrl(g().artwork.logo)}
                  alt={g().name}
                  class="bp-logo"
                  fallback={<span class="bp-title">{g().name}</span>}
                />
              </h1>
            </Show>
            <p class="bp-meta">
              {lastPlayedOf(g()) ? `Played ${timeAgo(lastPlayedOf(g()) ?? 0)}` : "Never played"}
              <Show when={playtimeOf(g()) > 0}> · {playtime(playtimeOf(g()))} in total</Show>
            </p>
            <div class="bp-actions">
              <Show
                when={running()}
                fallback={
                  <button type="button" class="bp-play" disabled={store.launching[g().id]} onClick={play}>
                    <Show when={store.launching[g().id]} fallback={<Play />}>
                      <Spinner size={22} />
                    </Show>
                    <span>{store.launching[g().id] ? "Starting" : "Play"}</span>
                  </button>
                }
              >
                {(at) => (
                  <>
                    <div class="bp-play is-running" role="status">
                      <span class="play-pulse" aria-hidden="true" />
                      <span>Playing</span>
                      <span class="bp-play-clock">{clock(now() - at())}</span>
                    </div>
                    <button type="button" class="bp-stop" onClick={halt}>
                      <Square />
                      Stop
                    </button>
                  </>
                )}
              </Show>
            </div>
          </main>
        )}
      </Show>

      <div class="bp-shelf" aria-label="Games">
        <div class="bp-track" style={{ transform: `translateX(calc(${index()} * -1 * (var(--bp-cover) + var(--bp-gap))))` }}>
          <For each={games()}>
            {(game, i) => (
              <button
                type="button"
                class="bp-cover"
                data-active={i() === index() || undefined}
                data-running={store.running[game.id] ? "" : undefined}
                aria-label={game.name}
                aria-current={i() === index() || undefined}
                onClick={() => (i() === index() ? play() : setIndex(i()))}
              >
                <GameCover game={game} />
              </button>
            )}
          </For>
        </div>
      </div>

      <footer class="bp-hints" aria-hidden="true">
        <span>
          <span class="bp-key">A</span>
          {running() ? "Playing" : "Play"}
        </span>
        <Show when={running()}>
          <span>
            <span class="bp-key">X</span>
            Stop
          </span>
        </Show>
        <span>
          <span class="bp-key bp-key-wide">◀ ▶</span>
          Browse
        </span>
        <span>
          <span class="bp-key">B</span>
          Exit
        </span>
      </footer>
    </div>
  );
}

/**
 * Resolves once the window has changed from `before` (its size as "WxH") and then held still for
 * a moment, since going fullscreen takes a few frames. Gives up after a short limit, for when the
 * size never changes.
 */
function windowSettled(before: string): Promise<void> {
  return new Promise((resolve) => {
    const started = Date.now();
    let last = before;
    let still = 0;
    const tick = setInterval(() => {
      const size = windowSize();
      still = size !== before && size === last ? still + 1 : 0;
      last = size;
      if (still >= 2 || Date.now() - started > 700) {
        clearInterval(tick);
        resolve();
      }
    }, 40);
  });
}
