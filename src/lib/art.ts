// Generated artwork for games without images: a soft, deterministic light field drawn from
// Burn's spectrum wash, so every game gets its own look without a network request.

const palette = ["#6fffc4", "#6db8ff", "#bb66ff", "#ff66c0", "#ffb875", "#74b9ff", "#d98ac9", "#6bc7d9"];

function hash(text: string): number {
  let h = 2166136261;
  for (let i = 0; i < text.length; i++) {
    h ^= text.charCodeAt(i);
    h = Math.imul(h, 16777619);
  }
  return h >>> 0;
}

function rng(seed: number) {
  let s = seed || 1;
  return () => {
    s ^= s << 13;
    s ^= s >>> 17;
    s ^= s << 5;
    return ((s >>> 0) % 10000) / 10000;
  };
}

function alpha(hex: string, a: number): string {
  const n = Number.parseInt(hex.slice(1), 16);
  return `rgba(${(n >> 16) & 255}, ${(n >> 8) & 255}, ${n & 255}, ${a})`;
}

/** A CSS background for `name`. The same name always gives the same image. */
export function generatedArt(name: string, intensity = 1): string {
  const random = rng(hash(name.trim().toLowerCase()));
  const start = Math.floor(random() * palette.length);
  const pick = (offset: number) => palette[(start + offset) % palette.length] ?? "#6db8ff";
  const layers = [0, 2, 5].map((offset, i) => {
    const x = Math.round(15 + random() * 70);
    const y = Math.round(10 + random() * 80);
    const size = Math.round(60 + random() * 50);
    const a = (0.62 - i * 0.12) * intensity;
    return `radial-gradient(${size}% ${Math.round(size * 0.9)}% at ${x}% ${y}%, ${alpha(pick(offset), a)} 0%, ${alpha(pick(offset), 0)} 70%)`;
  });
  return [...layers, "linear-gradient(160deg, #1f1f1f 0%, #0d0d0d 100%)"].join(", ");
}

/** One or two letters for a game's monogram tile. */
export function monogram(name: string): string {
  const words = name
    .replace(/[^\p{L}\p{N} ]/gu, " ")
    .split(/\s+/)
    .filter(Boolean);
  if (words.length === 0) return "?";
  const first = words[0] ?? "";
  if (words.length === 1) return first.slice(0, 2).toUpperCase();
  return ((first[0] ?? "") + (words[1]?.[0] ?? "")).toUpperCase();
}
