const relative = new Intl.RelativeTimeFormat(undefined, { numeric: "auto" });

/** "3 minutes ago", "yesterday", "2 months ago". */
export function timeAgo(unixSeconds: number, now = Date.now() / 1000): string {
  const diff = unixSeconds - now;
  const abs = Math.abs(diff);
  const steps: [number, Intl.RelativeTimeFormatUnit][] = [
    [60, "second"],
    [3600, "minute"],
    [86400, "hour"],
    [604800, "day"],
    [2629800, "week"],
    [31557600, "month"],
    [Infinity, "year"],
  ];
  if (abs < 45) return "just now";
  let divisor = 1;
  for (const [limit, unit] of steps) {
    if (abs < limit) return relative.format(Math.round(diff / divisor), unit);
    divisor = limit;
  }
  return relative.format(Math.round(diff / 31557600), "year");
}

/** "14h 22m", "38m", "under a minute". */
export function playtime(seconds: number): string {
  if (seconds < 60) return seconds > 0 ? "under a minute" : "0m";
  const hours = Math.floor(seconds / 3600);
  const minutes = Math.floor((seconds % 3600) / 60);
  if (hours === 0) return `${minutes}m`;
  return minutes === 0 ? `${hours}h` : `${hours}h ${minutes}m`;
}

/** Live session clock: "4:07" or "1:02:45". */
export function clock(seconds: number): string {
  const s = Math.max(0, Math.floor(seconds));
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const sec = String(s % 60).padStart(2, "0");
  return h > 0 ? `${h}:${String(m).padStart(2, "0")}:${sec}` : `${m}:${sec}`;
}

const units = ["B", "KB", "MB", "GB", "TB"];

export function bytes(value: number): string {
  let n = value;
  let i = 0;
  while (n >= 1024 && i < units.length - 1) {
    n /= 1024;
    i++;
  }
  return `${n >= 100 || i === 0 ? Math.round(n) : n.toFixed(1)} ${units[i]}`;
}

export function count(n: number, one: string, many = `${one}s`): string {
  return `${n.toLocaleString()} ${n === 1 ? one : many}`;
}

export function fileName(path: string): string {
  return path.split(/[\\/]/).pop() ?? path;
}

/** The executable's path relative to the game folder, for display. Keeps the system's separators. */
export function relativeTo(root: string, path: string): string {
  const base = root.replace(/[\\/]+$/, "");
  const inside = path.toLowerCase().startsWith(base.toLowerCase()) && /[\\/]/.test(path.charAt(base.length));
  return inside ? path.slice(base.length + 1) : path;
}

export const archLabel: Record<string, string> = {
  x64: "x64",
  x86: "x86",
  arm64: "ARM64",
  arm: "ARM",
  universal: "Universal",
  unknown: "Unknown",
};
