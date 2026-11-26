import { createSignal, onCleanup, onMount } from "solid-js";

const MIN_THUMB = 32;
const INSET = 4;

/**
 * A slim scrollbar drawn over a scroll container, so the content (like a game's banner) runs right
 * to the edge instead of stopping at a native scrollbar gutter. It shows while scrolling or when
 * the pointer is over the area, and the thumb can be dragged.
 */
export function OverlayScrollbar(props: { target: () => HTMLElement | undefined }) {
  const [thumb, setThumb] = createSignal({ top: 0, height: 0 });
  const [scrollable, setScrollable] = createSignal(false);
  const [visible, setVisible] = createSignal(false);
  const [dragging, setDragging] = createSignal(false);
  let track!: HTMLDivElement;
  let hide: number | undefined;

  const measure = () => {
    const el = props.target();
    if (!el) return;
    const { scrollTop, scrollHeight, clientHeight } = el;
    const room = track.clientHeight - INSET * 2;
    const canScroll = scrollHeight > clientHeight + 1;
    setScrollable(canScroll);
    if (!canScroll) return;
    const height = Math.max(MIN_THUMB, (clientHeight / scrollHeight) * room);
    const top = INSET + (scrollTop / (scrollHeight - clientHeight)) * (room - height);
    setThumb({ top, height });
  };

  const onScroll = () => {
    measure();
    setVisible(true);
    window.clearTimeout(hide);
    hide = window.setTimeout(() => setVisible(false), 900);
  };

  const onPointerDown = (e: PointerEvent) => {
    const el = props.target();
    if (!el || !scrollable()) return;
    e.preventDefault();
    const room = track.clientHeight - INSET * 2 - thumb().height;
    const range = el.scrollHeight - el.clientHeight;
    const rect = track.getBoundingClientRect();
    const onThumb = e.target !== track;
    // Clicking the track jumps there; dragging the thumb keeps the grab point under the pointer.
    const grab = onThumb ? e.clientY - rect.top - thumb().top : thumb().height / 2;
    const move = (ev: PointerEvent) => {
      const pos = ev.clientY - rect.top - INSET - grab;
      el.scrollTop = (Math.min(Math.max(pos, 0), room) / room) * range;
    };
    if (!onThumb) move(e);
    setDragging(true);
    track.setPointerCapture(e.pointerId);
    const up = () => {
      setDragging(false);
      track.removeEventListener("pointermove", move);
      track.removeEventListener("pointerup", up);
      track.removeEventListener("pointercancel", up);
    };
    track.addEventListener("pointermove", move);
    track.addEventListener("pointerup", up);
    track.addEventListener("pointercancel", up);
  };

  onMount(() => {
    const el = props.target();
    if (!el) return;
    el.addEventListener("scroll", onScroll, { passive: true });
    // The page changes size as games switch and panels load.
    const resize = new ResizeObserver(measure);
    resize.observe(el);
    const changes = new MutationObserver(() => {
      for (const child of el.children) resize.observe(child);
      measure();
    });
    changes.observe(el, { childList: true, subtree: true });
    for (const child of el.children) resize.observe(child);
    measure();
    onCleanup(() => {
      el.removeEventListener("scroll", onScroll);
      resize.disconnect();
      changes.disconnect();
      window.clearTimeout(hide);
    });
  });

  return (
    <div
      ref={track}
      class="overlay-scrollbar"
      aria-hidden="true"
      data-scrollable={scrollable() || undefined}
      data-visible={(visible() && scrollable()) || undefined}
      data-dragging={dragging() || undefined}
      onPointerDown={onPointerDown}
      style={{ display: scrollable() ? undefined : "none" }}
    >
      <div class="overlay-scrollbar-thumb" style={{ transform: `translateY(${thumb().top}px)`, height: `${thumb().height}px` }} />
    </div>
  );
}
