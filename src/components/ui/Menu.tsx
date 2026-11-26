import { createSignal, For, type JSX, onCleanup, onMount, Show } from "solid-js";
import { Portal } from "solid-js/web";

export interface MenuItem {
  label: string;
  icon?: JSX.Element;
  onSelect: () => void;
  danger?: boolean;
  disabled?: boolean;
  separatorBefore?: boolean;
}

interface Props {
  items: () => MenuItem[];
  trigger: (props: { onClick: (e: MouseEvent) => void; "aria-expanded": boolean; "aria-haspopup": "menu" }) => JSX.Element;
  align?: "start" | "end";
}

/** A list of actions anchored to a trigger. Arrow keys move, Enter selects, Escape closes. */
export function Menu(props: Props) {
  const [open, setOpen] = createSignal(false);
  const [position, setPosition] = createSignal({ top: 0, left: 0, right: 0 });
  let list: HTMLDivElement | undefined;
  let trigger: HTMLElement | undefined;

  const close = (refocus = true) => {
    setOpen(false);
    if (refocus) trigger?.focus();
  };

  const onDocPointer = (e: PointerEvent) => {
    if (list && !list.contains(e.target as Node) && !trigger?.contains(e.target as Node)) close(false);
  };
  const onBlur = () => setOpen(false);
  document.addEventListener("pointerdown", onDocPointer);
  window.addEventListener("blur", onBlur);
  onCleanup(() => {
    document.removeEventListener("pointerdown", onDocPointer);
    window.removeEventListener("blur", onBlur);
  });

  const toggle = (e: MouseEvent) => {
    trigger = e.currentTarget as HTMLElement;
    const rect = trigger.getBoundingClientRect();
    setPosition({ top: rect.bottom + 6, left: rect.left, right: window.innerWidth - rect.right });
    setOpen((v) => !v);
    queueMicrotask(() => list?.querySelector<HTMLElement>("[role=menuitem]:not([disabled])")?.focus());
  };

  const onKey = (e: KeyboardEvent) => {
    const items = [...(list?.querySelectorAll<HTMLElement>("[role=menuitem]:not([disabled])") ?? [])];
    const index = items.indexOf(document.activeElement as HTMLElement);
    if (e.key === "Escape") {
      e.stopPropagation();
      close();
    } else if (e.key === "ArrowDown") {
      e.preventDefault();
      items[(index + 1) % items.length]?.focus();
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      items[(index - 1 + items.length) % items.length]?.focus();
    } else if (e.key === "Tab") {
      close(false);
    }
  };

  return (
    <>
      {props.trigger({ onClick: toggle, "aria-expanded": open(), "aria-haspopup": "menu" })}
      <Show when={open()}>
        <Portal>
          <div
            ref={list}
            class="menu"
            role="menu"
            onKeyDown={onKey}
            style={
              props.align === "start"
                ? { top: `${position().top}px`, left: `${position().left}px` }
                : { top: `${position().top}px`, right: `${position().right}px` }
            }
          >
            <MenuItems items={props.items()} onPick={() => close(false)} />
          </div>
        </Portal>
      </Show>
    </>
  );
}

function MenuItems(props: { items: MenuItem[]; onPick: () => void }) {
  return (
    <For each={props.items}>
      {(item) => (
        <>
          <Show when={item.separatorBefore}>
            <div class="menu-separator" role="separator" />
          </Show>
          <button
            type="button"
            role="menuitem"
            class="menu-item"
            data-danger={item.danger || undefined}
            disabled={item.disabled}
            onClick={() => {
              props.onPick();
              item.onSelect();
            }}
          >
            {item.icon}
            <span>{item.label}</span>
          </button>
        </>
      )}
    </For>
  );
}

/** The same list of actions, opened at the pointer by a right-click. */
export function ContextMenu(props: { x: number; y: number; items: MenuItem[]; onClose: () => void }) {
  let list!: HTMLDivElement;
  const [position, setPosition] = createSignal({ left: props.x, top: props.y });

  onMount(() => {
    // Keep the whole menu on screen near the window's edges.
    const { width, height } = list.getBoundingClientRect();
    setPosition({
      left: Math.max(8, Math.min(props.x, window.innerWidth - width - 8)),
      top: Math.max(8, Math.min(props.y, window.innerHeight - height - 8)),
    });
    list.querySelector<HTMLElement>("[role=menuitem]:not([disabled])")?.focus();

    const onPointer = (e: PointerEvent) => {
      if (!list.contains(e.target as Node)) props.onClose();
    };
    const close = () => props.onClose();
    document.addEventListener("pointerdown", onPointer, true);
    window.addEventListener("blur", close);
    window.addEventListener("resize", close);
    document.addEventListener("wheel", close, { passive: true });
    onCleanup(() => {
      document.removeEventListener("pointerdown", onPointer, true);
      window.removeEventListener("blur", close);
      window.removeEventListener("resize", close);
      document.removeEventListener("wheel", close);
    });
  });

  const onKey = (e: KeyboardEvent) => {
    const items = [...list.querySelectorAll<HTMLElement>("[role=menuitem]:not([disabled])")];
    const index = items.indexOf(document.activeElement as HTMLElement);
    if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      props.onClose();
    } else if (e.key === "ArrowDown") {
      e.preventDefault();
      items[(index + 1) % items.length]?.focus();
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      items[(index - 1 + items.length) % items.length]?.focus();
    } else if (e.key === "Tab") {
      props.onClose();
    }
  };

  return (
    <Portal>
      <div
        ref={list}
        class="menu menu-context"
        role="menu"
        onKeyDown={onKey}
        onContextMenu={(e) => e.preventDefault()}
        style={{ top: `${position().top}px`, left: `${position().left}px` }}
      >
        <MenuItems items={props.items} onPick={props.onClose} />
      </div>
    </Portal>
  );
}
