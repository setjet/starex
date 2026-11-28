import { createEffect, createSignal, onCleanup, Show } from "solid-js";
import { store } from "../lib/store";

/** The loader shows at least this long, so the welcome is seen; then it waits for Starex's work. */
const MIN_MS = 5000;
/** The dissolve and the fade to the app (matches splash.css). */
const LEAVE_MS = 820;
/** How long each language stays before the next. */
const SWAP_EVERY_MS = 1500;
/** The text swap's exit, then entry (transitions.dev "Text states swap", --text-swap-dur). */
const SWAP_MS = 150;

/** "Hello" in English, Spanish, Russian, French, Chinese, Japanese and German, then round again. */
const WELCOME = [
  { word: "Hello", comma: ", " },
  { word: "Hola", comma: ", " },
  { word: "Привет", comma: ", " },
  { word: "Bonjour", comma: ", " },
  { word: "你好", comma: "，" },
  { word: "こんにちは", comma: "、" },
  { word: "Hallo", comma: ", " },
];

/**
 * The launch loader: "Hello, <name>" cycling through languages above a "Starting..." bar that
 * follows Starex's startup work. It stays at least five seconds, leaves once `ready` (the work is
 * done), dissolves away and calls `onDone`. It starts when `play` turns true.
 */
export function Splash(props: { play: boolean; ready: boolean; progress: number; onDone: () => void }) {
  const reduce = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
  const [visible, setVisible] = createSignal(!reduce);
  const [elapsed, setElapsed] = createSignal(0);
  const [leaving, setLeaving] = createSignal(false);
  const [language, setLanguage] = createSignal(0);
  let swapText!: HTMLSpanElement;
  let started = 0;
  const timers: ReturnType<typeof setTimeout>[] = [];
  onCleanup(() => timers.forEach(clearTimeout));

  // With reduced motion there is no loader: the app shows and the work runs behind it.
  if (reduce) queueMicrotask(props.onDone);

  /** The part that changes language: "Hello," (with the language's own comma when a name follows). */
  const greeting = () => {
    const { word, comma } = WELCOME[language()] ?? WELCOME[0]!;
    return store.userName ? `${word}${comma.trimEnd()}` : word;
  };

  /** The three-step swap: the text slides up and blurs away, changes, then settles in from below. */
  const swapLanguage = () => {
    if (leaving() || !swapText) return;
    swapText.classList.add("is-exit");
    timers.push(
      setTimeout(() => {
        setLanguage((i) => (i + 1) % WELCOME.length);
        swapText.classList.remove("is-exit");
        swapText.classList.add("is-enter-start");
        void swapText.offsetWidth;
        swapText.classList.remove("is-enter-start");
      }, SWAP_MS),
    );
  };

  const leave = () => {
    setLeaving(true);
    timers.push(
      setTimeout(() => {
        setVisible(false);
        props.onDone();
      }, LEAVE_MS),
    );
  };

  // Start once the window is on screen. Timers (not animation frames) keep it going even while the
  // window waits hidden in the tray.
  const intervals: ReturnType<typeof setInterval>[] = [];
  onCleanup(() => intervals.forEach(clearInterval));
  createEffect(() => {
    if (!props.play || started || reduce) return;
    started = Date.now();
    intervals.push(setInterval(() => setElapsed(Date.now() - started), 100));
    intervals.push(setInterval(swapLanguage, SWAP_EVERY_MS));
  });

  createEffect(() => {
    // Read every input first, so the effect follows all of them from its first run.
    const [ready, time, gone] = [props.ready, elapsed(), leaving()];
    if (started && !gone && ready && time >= MIN_MS) leave();
  });

  // The bar moves with time for the minimum five seconds, and never ahead of the work.
  const shown = () => Math.min(elapsed() / MIN_MS, props.ready ? 1 : 0.15 + 0.8 * props.progress);

  return (
    <Show when={visible()}>
      <div class="splash" data-play={props.play || undefined} data-leaving={leaving() || undefined} aria-hidden="true">
        <div class="splash-stage">
          {/* The greeting changes language; the name beside it stays still. */}
          <h1 class="splash-welcome" data-named={store.userName ? "" : undefined}>
            <span class="splash-part splash-part-greeting">
              <span class="splash-swap" ref={swapText}>
                {greeting()}
              </span>
            </span>
            <Show when={store.userName}>
              {(name) => (
                <span class="splash-part splash-part-name">
                  {name()}
                </span>
              )}
            </Show>
          </h1>
          <div class="splash-status">
            <span class="splash-starting">
              Starting<span class="splash-dot">.</span>
              <span class="splash-dot">.</span>
              <span class="splash-dot">.</span>
            </span>
            <span class="splash-bar">
              <span class="splash-bar-fill" style={{ transform: `scaleX(${shown()})` }} />
            </span>
          </div>
        </div>
      </div>
    </Show>
  );
}
