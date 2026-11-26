import { HardDrive } from "./icons";
import { Show } from "solid-js";
import { count, timeAgo } from "../lib/format";
import { store } from "../lib/store";
import type { Game } from "../lib/types";
import { Spinner } from "./ui/Spinner";

type Tone = "ok" | "busy" | "warn" | "bad" | "idle" | "live";

const taskLabels = {
  verify: "Checking files",
  baseline: "Recording baseline",
  scan: "Scanning for viruses",
} as const;

/**
 * The state of the game's files on this computer, in one line. Shows, in order of importance:
 * playing, a check in progress, threats, problems, warnings, up to date, or not checked yet.
 */
export function LocalStatus(props: { game: Game }) {
  const g = () => props.game;
  const task = () => store.tasks[g().id];

  const status = (): { tone: Tone; text: string } => {
    if (store.running[g().id]) return { tone: "live", text: "Playing now" };
    const t = task();
    if (t) {
      const p = t.progress;
      const share = p.totalBytes > 0 ? p.doneBytes / p.totalBytes : p.totalFiles > 0 ? p.doneFiles / p.totalFiles : 0;
      return { tone: "busy", text: `${taskLabels[t.task]}${share > 0 ? ` · ${Math.round(share * 100)}%` : "…"}` };
    }
    if (g().lastScan?.status === "threats") return { tone: "bad", text: "Threats found" };
    const v = g().lastVerification;
    if (!v) return { tone: "idle", text: "Not checked yet" };
    if (v.status === "fail") {
      return { tone: "bad", text: v.missing > 0 ? `${count(v.missing, "file")} missing` : "Problems found" };
    }
    if (v.status === "warn") return { tone: "warn", text: "Needs attention" };
    return { tone: "ok", text: "Up to date" };
  };

  const detail = () => {
    const v = g().lastVerification;
    const parts = [v ? `Files checked ${timeAgo(v.at)}` : "Files not checked yet"];
    const scan = g().lastScan;
    if (scan && scan.status !== "unavailable") parts.push(`virus scan ${timeAgo(scan.at)}`);
    return `${parts.join(" · ")}.`;
  };

  return (
    <div class="local-status" data-tone={status().tone} title={detail()} role="status">
      <span class="local-status-icon" aria-hidden="true">
        <Show when={status().tone === "busy"} fallback={<HardDrive />}>
          <Spinner size={16} />
        </Show>
      </span>
      <span class="local-status-copy">
        <span class="local-status-label">Local status</span>
        <span class="local-status-value">
          {status().text}
        </span>
      </span>
    </div>
  );
}
