import type { Accent } from "./types";

/** Each accent's main colour, hover colour and the text that sits on it. */
const ACCENTS: Record<Accent, { base: string; hover: string; text: string; ink: string }> = {
  blue: { base: "#007cf7", hover: "#1f8fff", text: "#ffffff", ink: "#42a0ff" },
  violet: { base: "#7c5cff", hover: "#9277ff", text: "#ffffff", ink: "#a68fff" },
  pink: { base: "#e23d8c", hover: "#ef5a9f", text: "#ffffff", ink: "#f27bb4" },
  green: { base: "#17a65a", hover: "#22bd6b", text: "#ffffff", ink: "#3fd07f" },
  orange: { base: "#f06a1d", hover: "#ff8338", text: "#ffffff", ink: "#ff9a5c" },
  white: { base: "#f0f0f0", hover: "#ffffff", text: "#141414", ink: "#f0f0f0" },
};

export const ACCENT_ORDER: Accent[] = ["blue", "violet", "pink", "green", "orange", "white"];

export function accentColor(accent: Accent): string {
  return ACCENTS[accent].base;
}

function soft(hex: string, alpha: number): string {
  const n = Number.parseInt(hex.slice(1), 16);
  return `rgba(${(n >> 16) & 255}, ${(n >> 8) & 255}, ${n & 255}, ${alpha})`;
}

/** Points the accent tokens at the chosen colour. Everything that uses them follows. */
export function applyAccent(accent: Accent) {
  const a = ACCENTS[accent] ?? ACCENTS.blue;
  const root = document.documentElement.style;
  root.setProperty("--sx-play", a.base);
  root.setProperty("--sx-play-hover", a.hover);
  root.setProperty("--sx-play-text", a.text);
  root.setProperty("--sx-accent", a.base);
  root.setProperty("--sx-accent-hover", a.hover);
  root.setProperty("--sx-accent-text", a.ink);
  root.setProperty("--sx-accent-soft", soft(a.base, 0.16));
}
