import { X } from "../icons";
import { createEffect, type JSX, onCleanup, onMount, Show } from "solid-js";
import { Portal } from "solid-js/web";
import { IconButton } from "./Button";

interface Props {
  title: string;
  description?: JSX.Element;
  onClose: () => void;
  children: JSX.Element;
  footer?: JSX.Element;
  size?: "sm" | "md" | "lg" | "xl";
  /** Blocks closing by Escape or a click outside while work is in progress. */
  busy?: boolean;
  class?: string;
  bare?: boolean;
}

const focusable = 'button:not([disabled]), input:not([disabled]), select, textarea, [href], [tabindex]:not([tabindex="-1"])';

/** A modal task. Focus moves into it, stays inside it, and returns to the trigger on close. */
export function Dialog(props: Props) {
  let panel!: HTMLDivElement;
  const previous = document.activeElement as HTMLElement | null;

  const close = () => {
    if (!props.busy) props.onClose();
  };

  // Escape closes from anywhere: focus can sit outside the panel after a native file dialog.
  const onEscape = (e: KeyboardEvent) => {
    if (e.key === "Escape" && !e.defaultPrevented) {
      e.preventDefault();
      close();
    }
  };
  document.addEventListener("keydown", onEscape);
  onCleanup(() => document.removeEventListener("keydown", onEscape));

  const onKey = (e: KeyboardEvent) => {
    if (e.key === "Tab") {
      const items = [...panel.querySelectorAll<HTMLElement>(focusable)].filter((el) => el.offsetParent !== null);
      const first = items[0];
      const last = items[items.length - 1];
      if (!first || !last) return;
      if (e.shiftKey && document.activeElement === first) {
        e.preventDefault();
        last.focus();
      } else if (!e.shiftKey && document.activeElement === last) {
        e.preventDefault();
        first.focus();
      }
    }
  };

  onMount(() => {
    const auto = panel.querySelector<HTMLElement>("[data-autofocus]") ?? panel;
    queueMicrotask(() => auto.focus());
  });
  onCleanup(() => previous?.focus?.());
  createEffect(() => document.body.toggleAttribute("data-dialog-open", true));
  onCleanup(() => document.body.toggleAttribute("data-dialog-open", false));

  return (
    <Portal>
      <div class="dialog-overlay" onPointerDown={(e) => e.target === e.currentTarget && close()}>
        <div
          ref={panel}
          class={`dialog dialog-${props.size ?? "md"} ${props.class ?? ""}`}
          role="dialog"
          aria-modal="true"
          aria-label={props.title}
          tabIndex={-1}
          onKeyDown={onKey}
        >
          <Show when={!props.bare} fallback={props.children}>
            <header class="dialog-header">
              <div class="dialog-heading">
                <h2 class="dialog-title">{props.title}</h2>
                <Show when={props.description}>
                  <p class="dialog-description">{props.description}</p>
                </Show>
              </div>
              <IconButton label="Close" size="sm" onClick={close} disabled={props.busy}>
                <X />
              </IconButton>
            </header>
            <div class="dialog-body">{props.children}</div>
            <Show when={props.footer}>
              <footer class="dialog-footer">{props.footer}</footer>
            </Show>
          </Show>
        </div>
      </div>
    </Portal>
  );
}
