import { createEffect, createSignal, onCleanup, Show } from "solid-js";

// The star from the Starex mark, in a 24-unit box.
const STAR =
  "M12 1.5c.55 5.1 2.4 8.4 10.5 10.1v.8c-8.1 1.7-9.95 5-10.5 10.1-.55-5.1-2.4-8.4-10.5-10.1v-.8C9.6 9.9 11.45 6.6 12 1.5Z";

// A jagged crack from the top-left corner to the bottom-right one. Each half of the star is the
// star clipped to one side of it.
const CRACK = "0,0 6,4.5 9.6,8.6 8.9,10.3 12.6,12.4 11.6,14.2 15,16.6 24,24";
const CRACK_VISIBLE = "6,4.5 9.6,8.6 8.9,10.3 12.6,12.4 11.6,14.2 15,16.6 18,19.6";

const DURATION_MS = 1850;

/**
 * The launch animation: the star appears, a streak of light pierces it, a crack runs through
 * it, and the two halves part to reveal the app. It plays once, when `play` turns true.
 */
export function Splash(props: { play: boolean }) {
  const [visible, setVisible] = createSignal(true);
  let timer: ReturnType<typeof setTimeout> | undefined;

  const reduce = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
  // Start the clock only once the window is on screen, so the whole animation is seen.
  createEffect(() => {
    if (props.play && timer === undefined) {
      timer = setTimeout(() => setVisible(false), reduce ? 0 : DURATION_MS);
    }
  });
  onCleanup(() => clearTimeout(timer));

  return (
    <Show when={visible()}>
      <div class="splash" data-play={props.play || undefined} aria-hidden="true">
        <div class="splash-stage">
          <div class="splash-glow" />
          <svg class="splash-art" viewBox="-12 -12 48 48" width="240" height="240">
            <defs>
              <linearGradient id="splash-fill" x1="0.2" y1="0" x2="0.8" y2="1">
                <stop offset="0" stop-color="#ffffff" />
                <stop offset="1" stop-color="#d6d6dc" />
              </linearGradient>
              <radialGradient id="splash-flash">
                <stop offset="0" stop-color="#ffffff" stop-opacity="1" />
                <stop offset="0.35" stop-color="#bfe0ff" stop-opacity="0.55" />
                <stop offset="1" stop-color="#6db8ff" stop-opacity="0" />
              </radialGradient>
              <clipPath id="splash-lower">
                <polygon points={`${CRACK} 0,24`} />
              </clipPath>
              <clipPath id="splash-upper">
                <polygon points={`${CRACK} 24,0`} />
              </clipPath>
            </defs>

            <g class="splash-star">
              <g class="splash-half splash-half-lower" clip-path="url(#splash-lower)">
                <path d={STAR} fill="url(#splash-fill)" />
              </g>
              <g class="splash-half splash-half-upper" clip-path="url(#splash-upper)">
                <path d={STAR} fill="url(#splash-fill)" />
              </g>
              <polyline class="splash-crack" points={CRACK_VISIBLE} pathLength="1" />
            </g>

            <circle class="splash-flash" cx="12" cy="12" r="11" fill="url(#splash-flash)" />
            <line class="splash-beam" x1="-14" y1="-14" x2="38" y2="38" pathLength="100" />
          </svg>
          <div class="splash-word">Starex</div>
        </div>
      </div>
    </Show>
  );
}
