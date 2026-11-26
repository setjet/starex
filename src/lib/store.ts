import { batch, createMemo, createSignal } from "solid-js";
import { createStore, produce, reconcile } from "solid-js/store";
import { api, errorText, events } from "./api";
import { count, playtime } from "./format";
import { toast } from "./toast";
import type { Game, ScanReport, Settings, TaskProgress, VerifyMode, VerifyReport } from "./types";
import { checkForUpdates, loadVersion } from "./updater";

const defaultSettings: Settings = {
  launchBehavior: "minimize",
  restoreOnExit: true,
  verifyBeforeLaunch: false,
  checkForUpdates: true,
  librarySort: "recent",
  autoArtwork: true,
  steamgriddbKey: "",
  reduceMotion: false,
  scanNewGames: true,
  winePath: "",
};

interface AppStore {
  ready: boolean;
  games: Game[];
  settings: Settings;
  /** Game id -> session start (Unix seconds). */
  running: Record<string, number>;
  launching: Record<string, boolean>;
  tasks: Record<string, TaskProgress | undefined>;
  /** The latest full report per game, for this run of the app. */
  reports: Record<string, VerifyReport | undefined>;
  dataDir: string;
  hostArch: string;
  hostOs: string;
  repository: string;
}

export const [store, setStore] = createStore<AppStore>({
  ready: false,
  games: [],
  settings: defaultSettings,
  running: {},
  launching: {},
  tasks: {},
  reports: {},
  dataDir: "",
  hostArch: "",
  hostOs: "",
  repository: "https://github.com/setjet/starex",
});

// ---------------------------------------------------------------------------------------------
// Navigation and dialogs

const SELECTED_KEY = "starex.selected";

function readSelected(): string | null {
  try {
    return localStorage.getItem(SELECTED_KEY);
  } catch {
    return null;
  }
}

const [selectedId, setSelectedIdRaw] = createSignal<string | null>(readSelected());
export { selectedId };

export function select(id: string | null) {
  setSelectedIdRaw(id);
  try {
    if (id) localStorage.setItem(SELECTED_KEY, id);
    else localStorage.removeItem(SELECTED_KEY);
  } catch {
    // Storage is a convenience only.
  }
}

export const [query, setQuery] = createSignal("");

export type Dialog =
  | { type: "add"; path?: string }
  | { type: "settings"; section?: SettingsSection }
  | { type: "properties"; id: string; tab?: PropertiesTab }
  | { type: "remove"; id: string }
  | { type: "saves"; id: string }
  | { type: "threats"; id: string }
  | { type: "update" };

export type PropertiesTab = "general" | "launch" | "saves" | "artwork";

/** Bumped whenever a game's save backups may have changed, so open views reload them. */
export const [savesVersion, setSavesVersion] = createSignal(0);

/** The latest full scan report per game, for this run of the app. */
const [scanReports, setScanReports] = createStore<Record<string, ScanReport | undefined>>({});
export { scanReports };

export type SettingsSection = "general" | "library" | "artwork" | "updates" | "about";

export const [dialog, setDialog] = createSignal<Dialog | null>(null);

export const selectedGame = createMemo(() => store.games.find((g) => g.id === selectedId()) ?? null);

export const visibleGames = createMemo(() => {
  const q = query().trim().toLowerCase();
  const list = store.games.filter((g) => !q || g.name.toLowerCase().includes(q));
  const sort = store.settings.librarySort;
  return [...list].sort((a, b) => {
    if (a.favorite !== b.favorite) return a.favorite ? -1 : 1;
    if (sort === "name") return a.name.localeCompare(b.name, undefined, { sensitivity: "base" });
    if (sort === "playtime") return b.playtimeSecs - a.playtimeSecs || a.name.localeCompare(b.name);
    return (b.lastPlayed ?? b.addedAt) - (a.lastPlayed ?? a.addedAt);
  });
});

// ---------------------------------------------------------------------------------------------
// Startup

export async function init() {
  const [boot] = await Promise.all([api.bootstrap(), loadVersion()]);
  batch(() => {
    setStore({
      ready: true,
      games: boot.games,
      settings: boot.settings,
      running: Object.fromEntries(boot.running.map((r) => [r.gameId, r.startedAt])),
      dataDir: boot.dataDir,
      hostArch: boot.hostArch,
      hostOs: boot.hostOs,
      repository: boot.repository,
    });
    if (selectedId() && !boot.games.some((g) => g.id === selectedId())) select(null);
  });

  await Promise.all([
    events.gameUpdated(upsert),
    events.taskProgress((p) => setStore("tasks", p.gameId, p.done ? undefined : p)),
    events.scanFinished(onScanFinished),
    events.session((e) => {
      if (e.state === "running") {
        setStore("running", e.gameId, e.startedAt);
        setStore("launching", e.gameId, false);
        return;
      }
      setStore(
        "running",
        produce((r) => {
          delete r[e.gameId];
        }),
      );
      if (e.game) upsert(e.game);
      setSavesVersion((v) => v + 1);
      const name = e.game?.name ?? "The game";
      if (e.exitHint) {
        toast(`${name} stopped with an error`, { tone: "warning", detail: e.exitHint, duration: 9000 });
      } else if (e.seconds < 10 && e.exitCode !== null && e.exitCode !== 0) {
        toast(`${name} closed right away`, {
          tone: "warning",
          detail: `It exited with code ${e.exitCode}. Verify its files or check its launch options.`,
        });
      } else if (e.seconds >= 60) {
        toast(`Session ended · ${playtime(e.seconds)}`, { detail: name });
      }
    }),
  ]);

  if (store.settings.checkForUpdates && !import.meta.env.DEV) {
    setTimeout(() => void checkForUpdates(false), 3000);
  }
}

function upsert(game: Game) {
  const index = store.games.findIndex((g) => g.id === game.id);
  if (index === -1) setStore("games", (list) => [...list, game]);
  else setStore("games", index, reconcile(game));
}

// ---------------------------------------------------------------------------------------------
// Actions. Each reports its own failure, so callers can fire and forget.

/** Starts a game. A game with threats found in its last virus scan asks first. */
export async function launch(id: string, confirmed = false) {
  if (store.launching[id] || store.running[id]) return;
  const game = store.games.find((g) => g.id === id);
  if (!confirmed && game?.lastScan?.status === "threats") {
    setDialog({ type: "threats", id });
    return;
  }
  setStore("launching", id, true);
  try {
    await api.launch(id);
  } catch (e) {
    toast("Could not start the game", { tone: "danger", detail: errorText(e) });
  } finally {
    setStore("launching", id, false);
  }
}

export async function stop(id: string) {
  try {
    await api.stop(id);
  } catch (e) {
    toast("Could not stop the game", { tone: "danger", detail: errorText(e) });
  }
}

export async function verify(id: string, mode: VerifyMode) {
  setStore("tasks", id, { gameId: id, task: "verify", progress: { doneFiles: 0, totalFiles: 0, doneBytes: 0, totalBytes: 0 } });
  try {
    const report = await api.verify(id, mode);
    setStore("reports", id, report);
    if (report.cancelled) toast("Verification cancelled");
    else if (report.status === "fail") toast("Problems found", { tone: "danger", detail: firstProblem(report) });
    else if (report.status === "warn") toast("Verified with warnings", { tone: "warning", detail: firstProblem(report) });
    else {
      const what = report.filesChecked > 0 ? count(report.filesChecked, "file") : "Executable";
      toast("Everything checks out", { tone: "success", detail: `${what} verified in ${seconds(report.durationMs)}.` });
    }
    return report;
  } catch (e) {
    toast("Verification failed", { tone: "danger", detail: errorText(e) });
    return null;
  } finally {
    setStore("tasks", id, undefined);
  }
}

export async function recordBaseline(id: string) {
  setStore("tasks", id, { gameId: id, task: "baseline", progress: { doneFiles: 0, totalFiles: 0, doneBytes: 0, totalBytes: 0 } });
  try {
    const info = await api.createBaseline(id);
    if (info) toast("Baseline recorded", { tone: "success", detail: `${count(info.files, "file")} will be checked from now on.` });
    else toast("Baseline cancelled");
    return info;
  } catch (e) {
    toast("Could not record a baseline", { tone: "danger", detail: errorText(e) });
    return null;
  } finally {
    setStore("tasks", id, undefined);
  }
}

export async function scan(id: string) {
  setStore("tasks", id, { gameId: id, task: "scan", progress: { doneFiles: 0, totalFiles: 0, doneBytes: 0, totalBytes: 0 } });
  try {
    await api.scanGame(id);
  } catch (e) {
    toast("Virus scan failed", { tone: "danger", detail: errorText(e) });
  } finally {
    setStore("tasks", id, undefined);
  }
}

function onScanFinished(report: ScanReport) {
  setScanReports(report.gameId, report);
  const name = store.games.find((g) => g.id === report.gameId)?.name ?? "This game";
  if (report.status === "threats") {
    toast(`Threats found in ${name}`, {
      tone: "danger",
      detail: `${count(report.threats.length, "file")} flagged by ${report.engine}. Open the game to see which.`,
      duration: 10000,
    });
  } else if (report.status === "clean") {
    toast("No threats found", { tone: "success", detail: `${name}: ${count(report.filesScanned, "program file")} scanned.` });
  } else if (report.status === "unavailable") {
    toast("Virus scan unavailable", { tone: "warning", detail: report.message ?? undefined });
  }
}

export async function backupSaves(id: string) {
  try {
    const backup = await api.backupSaves(id);
    setSavesVersion((v) => v + 1);
    if (backup) toast("Saves backed up", { tone: "success", detail: `${count(backup.files, "file")} copied.` });
    else toast("Nothing to back up", { detail: "The save folder is empty." });
  } catch (e) {
    toast("Could not back up saves", { tone: "danger", detail: errorText(e) });
  }
}

export async function restoreSaves(id: string, backupId: string) {
  try {
    await api.restoreSaves(id, backupId);
    setSavesVersion((v) => v + 1);
    toast("Saves restored", { tone: "success", detail: "Your saves from before the restore were backed up too." });
    return true;
  } catch (e) {
    toast("Could not restore saves", { tone: "danger", detail: errorText(e) });
    return false;
  }
}

export async function cancelTask(id: string) {
  await api.cancelTask(id).catch(() => undefined);
}

export async function patchGame(id: string, patch: Parameters<typeof api.updateGame>[1]) {
  try {
    const game = await api.updateGame(id, patch);
    upsert(game);
    return game;
  } catch (e) {
    toast("Could not save", { tone: "danger", detail: errorText(e) });
    return null;
  }
}

export async function addGame(...args: Parameters<typeof api.addGame>) {
  const game = await api.addGame(...args);
  upsert(game);
  select(game.id);
  return game;
}

export async function removeGame(id: string) {
  const game = store.games.find((g) => g.id === id);
  try {
    await api.removeGame(id);
    batch(() => {
      setStore("games", (list) => list.filter((g) => g.id !== id));
      if (selectedId() === id) select(null);
    });
    toast(`${game?.name ?? "Game"} removed from your library`, { detail: "Its files on disk were not touched." });
  } catch (e) {
    toast("Could not remove the game", { tone: "danger", detail: errorText(e) });
  }
}

export async function setArtwork(id: string, kind: Parameters<typeof api.setArtwork>[1], source: string | null) {
  try {
    upsert(await api.setArtwork(id, kind, source));
  } catch (e) {
    toast("Could not change the artwork", { tone: "danger", detail: errorText(e) });
  }
}

export async function fetchArtwork(id: string, overwrite: boolean) {
  try {
    upsert(await api.fetchArtwork(id, overwrite));
    toast("Artwork updated", { tone: "success" });
  } catch (e) {
    toast("No artwork found", { tone: "warning", detail: errorText(e) });
  }
}

export async function saveSettings(patch: Partial<Settings>) {
  const previous = store.settings;
  const next = { ...previous, ...patch };
  setStore("settings", next);
  try {
    setStore("settings", await api.saveSettings(next));
  } catch (e) {
    setStore("settings", previous);
    toast("Could not save settings", { tone: "danger", detail: errorText(e) });
  }
}

function firstProblem(report: VerifyReport): string | undefined {
  return report.checks.find((c) => c.status === "fail")?.detail ?? report.checks.find((c) => c.status === "warn")?.detail;
}

function seconds(ms: number): string {
  return ms < 1000 ? `${ms} ms` : `${(ms / 1000).toFixed(1)} s`;
}
