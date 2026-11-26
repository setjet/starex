import { createSignal } from "solid-js";

export type ToastTone = "neutral" | "success" | "warning" | "danger";

export interface Toast {
  id: number;
  tone: ToastTone;
  title: string;
  detail?: string;
  leaving?: boolean;
}

const [toasts, setToasts] = createSignal<Toast[]>([]);
let next = 1;

export { toasts };

export function dismiss(id: number) {
  setToasts((list) => list.map((t) => (t.id === id ? { ...t, leaving: true } : t)));
  setTimeout(() => setToasts((list) => list.filter((t) => t.id !== id)), 200);
}

/** Shows a short message at the bottom of the window. Errors stay longer. */
export function toast(title: string, options: { tone?: ToastTone; detail?: string; duration?: number } = {}) {
  const id = next++;
  const tone = options.tone ?? "neutral";
  setToasts((list) => [...list.slice(-3), { id, tone, title, detail: options.detail }]);
  const duration = options.duration ?? (tone === "danger" ? 7000 : 4000);
  setTimeout(() => dismiss(id), duration);
  return id;
}
