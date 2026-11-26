import { batch, createMemo, createSignal } from "solid-js";
import { createStore, produce, reconcile } from "solid-js/store";
import { api, errorText, events } from "./api";
import { count, playtime } from "./format";
import { toast } from "./toast";
import type { Game, Runtime, ScanReport, Settings, TaskProgress, VerifyMode, VerifyReport } from "./types";
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
  saveDatabase: true,
  cloudBackupDir: "",
  discordPresence: true,
  discordAppId: "",
  closeToTray: false,
  startWithSystem: false,
  globalShortcut: "Alt+Shift+S",
  igdbClientId: "",
  igdbClientSecret: "",
  backdrop: "none",
  backdropImage: "",
  backdropStrength: 60,
  accent: "blue",
  launchAnimation: true,
  welcomeSeen: false,
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
  discordBuiltin: boolean;
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
  discordBuiltin: false,
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
  | { type: "runtimes"; id: string; runtimes: Runtime[] }
  | { type: "update" }
  | { type: "changelog" }
  | { type: "welcome" };

export type PropertiesTab = "general" | "launch" | "saves" | "artwork";

/** Bumped whenever a game's save backups may have changed, so open views reload them. */
export const [savesVersion, setSavesVersion] = createSignal(0);

/** The latest full scan report per game, for this run of the app. */
const [scanReports, setScanReports] = createStore<Record<string, ScanReport | undefined>>({});
export { scanReports };

export type SettingsSection = "general" | "appearance" | "games" | "integrations" | "updates" | "about";

export const [dialog, setDialog] = createSignal<Dialog | null>(null);

/** The game whose right-click menu is open, and where. */
export const [contextMenu, setContextMenu] = createSignal<{ gameId: string; x: number; y: number } | null>(null);

export function openContextMenu(gameId: string, e: MouseEvent) {
  e.preventDefault();
  setContextMenu({ gameId, x: e.clientX, y: e.clientY });
}

export const selectedGame = createMemo(() => store.games.find((g) => g.id === selectedId()) ?? null);

const HIDDEN_KEY = "starex.showHidden";

function readShowHidden(): boolean {
  try {
    return localStorage.getItem(HIDDEN_KEY) === "1";
  } catch {
    return false;
  }
}

/** Hidden games appear in the library only while this is on (sidebar sort menu). */
const [showHidden, setShowHiddenRaw] = createSignal(readShowHidden());
export { showHidden };

export function setShowHidden(show: boolean) {
  setShowHiddenRaw(show);
  try {
    localStorage.setItem(HIDDEN_KEY, show ? "1" : "0");
  } catch {
    // A convenience only.
  }
}

export const visibleGames = createMemo(() => {
  const q = query().trim().toLowerCase();
  const list = store.games.filter((g) => (showHidden() || !g.hidden) && (!q || g.name.toLowerCase().includes(q)));
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

const [startHidden, setStartHidden] = createSignal(false);
export { startHidden };

export async function init() {
  const [boot] = await Promise.all([api.bootstrap(), loadVersion()]);
  setStartHidden(boot.startHidden);
  batch(() => {
    setStore({
      ready: true,
      games: boot.games,
      settings: boot.settings,
      running: Object.fromEntries(boot.running.map((r) => [r.gameId, r.startedAt])),
      dataDir: boot.dataDir,
      hostArch: boot.hostArch,
      hostOs: boot.hostOs,
      discordBuiltin: boot.discordBuiltin,
      repository: boot.repository,
    });
    if (selectedId() && !boot.games.some((g) => g.id === selectedId())) select(null);
  });

  await Promise.all([
    events.gameUpdated(upsert),
    events.taskProgress((p) => setStore("tasks", p.gameId, p.done ? undefined : p)),
    events.scanFinished(onScanFinished),
    events.playRequest(playFromShortcut),
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

  // Starex was opened from a game shortcut or a starex:// link.
  const requested = await api.takeLaunchRequest().catch(() => null);
  if (requested) playFromShortcut(requested);

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

/** Runs `work`, keeping its loader visible for at least `ms` so fast tasks do not flash. */
async function atLeast<T>(work: Promise<T>, ms = 600): Promise<T> {
  const [result] = await Promise.all([work, new Promise((r) => setTimeout(r, ms))]);
  return result;
}

let lastPlayRequest = { id: "", at: 0 };

/** A shortcut or starex:// link asked for a game: show it and start it. */
function playFromShortcut(id: string) {
  const now = Date.now();
  // A link can arrive twice (as an argument and as a link event); start the game once.
  if (lastPlayRequest.id === id && now - lastPlayRequest.at < 3000) return;
  lastPlayRequest = { id, at: now };
  if (!store.games.some((g) => g.id === id)) {
    toast("That game is no longer in your library", { tone: "warning" });
    return;
  }
  select(id);
  void launch(id);
}

/**
 * Starts a game. Asks first when its last virus scan found threats, and when a system runtime
 * it needs (Visual C++, DirectX, .NET) is missing. `skip` lists the questions already answered.
 */
export async function launch(id: string, skip: { threats?: boolean; runtimes?: boolean } = {}) {
  if (store.launching[id] || store.running[id]) return;
  const game = store.games.find((g) => g.id === id);
  if (!skip.threats && game?.lastScan?.status === "threats") {
    setDialog({ type: "threats", id });
    return;
  }
  if (!skip.runtimes && store.hostOs === "windows") {
    const missing = await missingRuntimes(id);
    if (missing.length > 0) {
      setDialog({ type: "runtimes", id, runtimes: missing });
      return;
    }
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

/** Runtimes a game needs that this computer lacks. A failed check never blocks playing. */
export async function missingRuntimes(id: string): Promise<Runtime[]> {
  try {
    return (await api.checkRuntimes(id)).filter((r) => !r.installed);
  } catch {
    return [];
  }
}

export async function createShortcut(id: string, place: "desktop" | "startMenu") {
  try {
    await api.createShortcut(id, place);
    toast(place === "desktop" ? "Desktop shortcut created" : "Added to the Start menu", {
      tone: "success",
      detail: "It starts the game through Starex, so play time and save backups still work.",
    });
  } catch (e) {
    toast("Could not create the shortcut", { tone: "danger", detail: errorText(e) });
  }
}

export async function fetchDetails(id: string) {
  try {
    upsert(await api.fetchDetails(id));
    toast("Game details updated", { tone: "success" });
  } catch (e) {
    toast("No details found", { tone: "warning", detail: errorText(e) });
  }
}

export async function setOffline(id: string, offline: boolean) {
  try {
    upsert(await api.setOffline(id, offline));
    toast(offline ? "Blocked from going online" : "Online access allowed again", {
      tone: "success",
      detail: offline ? "Windows Firewall now stops the game's programs from reaching the internet." : undefined,
    });
  } catch (e) {
    toast("Could not change online access", { tone: "danger", detail: errorText(e) });
  }
}

export async function copyLaunchLink(id: string) {
  try {
    const link = await api.launchLink(id);
    await navigator.clipboard.writeText(link);
    toast("Launch link copied", { tone: "success", detail: link });
  } catch (e) {
    toast("Could not copy the link", { tone: "danger", detail: errorText(e) });
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
    const report = await atLeast(api.verify(id, mode));
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
    const info = await atLeast(api.createBaseline(id));
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
    await atLeast(api.scanGame(id));
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

/** Uses an image file as the window's backdrop. */
export async function setBackdropImage(source: string) {
  try {
    setStore("settings", await api.setBackdropImage(source));
  } catch (e) {
    toast("Could not use that image", { tone: "danger", detail: errorText(e) });
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
