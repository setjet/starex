// A demo backend for working on the interface in a plain browser (`bun run dev:web`).
// It is loaded only in development and only when Tauri is absent, so it never ships.

import { emit } from "@tauri-apps/api/event";
import { mockIPC, mockWindows } from "@tauri-apps/api/mocks";
import type { Game, Settings, VerifyReport } from "../lib/types";

const now = Math.floor(Date.now() / 1000);
const day = 86400;

function game(id: string, name: string, extra: Partial<Game>): Game {
  return {
    id,
    name,
    installDir: `D:\\Games\\${name.replace(/\W+/g, "")}`,
    executable: `D:\\Games\\${name.replace(/\W+/g, "")}\\${name.replace(/\W+/g, "")}.exe`,
    launchArgs: "",
    workingDir: null,
    env: [],
    artwork: { banner: null, cover: null, logo: null, icon: null },
    favorite: false,
    addedAt: now - 40 * day,
    lastPlayed: null,
    playtimeSecs: 0,
    sessions: 0,
    lastVerification: null,
    saveDir: null,
    backupSaves: true,
    lastScan: null,
    ...extra,
  };
}

const games: Game[] = [
  game("nebula", "Nebula Drift", {
    favorite: true,
    lastPlayed: now - 2 * 3600,
    playtimeSecs: 51_720,
    sessions: 23,
    launchArgs: "-dx12 -windowed",
    saveDir: "C:/Users/you/Saved Games/Nebula Drift",
    lastScan: { at: now - 3 * day, status: "clean", threats: 0, filesScanned: 38 },
    lastVerification: { at: now - 3 * 3600, mode: "full", status: "pass", filesChecked: 4_812, missing: 0, modified: 0, durationMs: 6_400 },
  }),
  game("hollow", "Hollow Meridian", { lastPlayed: now - 3 * day, playtimeSecs: 9_300, sessions: 6 }),
  game("ember", "Ember & Ash", {
    lastPlayed: now - 9 * day,
    playtimeSecs: 132_000,
    sessions: 71,
    lastVerification: { at: now - 9 * day, mode: "quick", status: "warn", filesChecked: 1_220, missing: 0, modified: 2, durationMs: 380 },
  }),
  game("signal", "Signal Lost", { playtimeSecs: 0 }),
  game("tidal", "Tidal Engine", { lastPlayed: now - 30 * day, playtimeSecs: 4_100, sessions: 3 }),
  game("orbit", "Orbit Kitchen", { lastPlayed: now - 65 * day, playtimeSecs: 22_000, sessions: 14 }),
  game("paper", "Paper Kingdoms", { playtimeSecs: 600, sessions: 1, lastPlayed: now - 120 * day }),
];

let settings: Settings = {
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

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

function report(id: string, mode: "quick" | "full"): VerifyReport {
  return {
    gameId: id,
    mode,
    status: "pass",
    checks: [
      { id: "folder", title: "Game folder", detail: `D:\\Games\\${id}`, status: "pass" },
      { id: "executable", title: "Executable", detail: "NebulaDrift.exe · 64-bit x64", status: "pass" },
      { id: "files", title: "Game files", detail: "All 4,812 files are present and intact, checked against your baseline.", status: "pass" },
    ],
    issues: [],
    manifestSource: "baseline",
    filesChecked: 4812,
    bytesChecked: 68_000_000_000,
    missing: 0,
    modified: 0,
    durationMs: mode === "full" ? 6400 : 310,
    cancelled: false,
    at: Math.floor(Date.now() / 1000),
  };
}

export function installDemo() {
  mockWindows("main");
  mockIPC(
    async (cmd, raw) => {
      const args = (raw ?? {}) as Record<string, unknown>;
      const id = args.id as string;
      switch (cmd) {
        case "bootstrap":
          return { games, settings, running: [], dataDir: "C:\\Users\\you\\AppData\\Roaming\\Starex", hostArch: "x86_64", hostOs: "windows", repository: "https://github.com/setjet/starex" };
        case "save_settings":
          settings = args.settings as Settings;
          return settings;
        case "update_game": {
          const g = games.find((x) => x.id === id);
          if (g) Object.assign(g, args.patch);
          return g;
        }
        case "baseline_info":
          return id === "nebula" ? { files: 4812, totalBytes: 68_000_000_000, generatedAt: now - 12 * day, shipped: false } : null;
        case "launch_game":
          await sleep(700);
          void emit("starex://session", { state: "running", gameId: id, startedAt: Math.floor(Date.now() / 1000) });
          return Math.floor(Date.now() / 1000);
        case "stop_game":
          void emit("starex://session", { state: "exited", gameId: id, seconds: 75, exitCode: 0, game: games.find((g) => g.id === id) });
          return null;
        case "verify_game": {
          for (let i = 1; i <= 10; i++) {
            await sleep(120);
            void emit("starex://task-progress", {
              gameId: id,
              task: "verify",
              progress: { doneFiles: i * 481, totalFiles: 4812, doneBytes: i * 6.8e9, totalBytes: 6.8e10 },
            });
          }
          return report(id, args.mode as "quick" | "full");
        }
        case "scan_game": {
          for (let i = 1; i <= 6; i++) {
            await sleep(150);
            void emit("starex://task-progress", {
              gameId: id,
              task: "scan",
              progress: { doneFiles: i * 6, totalFiles: 38, doneBytes: i * 4e7, totalBytes: 2.4e8 },
            });
          }
          const scanned = {
            gameId: id,
            status: "clean",
            engine: "Windows antivirus (AMSI)",
            filesScanned: 38,
            bytesScanned: 2.4e8,
            skipped: 0,
            threats: [],
            message: null,
            durationMs: 900,
            at: Math.floor(Date.now() / 1000),
          };
          const g = games.find((x) => x.id === id);
          if (g) g.lastScan = { at: scanned.at, status: "clean", threats: 0, filesScanned: 38 };
          if (g) void emit("starex://game-updated", g);
          void emit("starex://scan-finished", scanned);
          return scanned;
        }
        case "list_save_backups":
          return id === "nebula"
            ? [
                { id: "2", at: now - 2 * 3600, reason: "afterSession", files: 6, bytes: 2_400_000 },
                { id: "1", at: now - 3 * day, reason: "beforeLaunch", files: 5, bytes: 2_100_000 },
              ]
            : [];
        case "suggest_save_locations":
          return [{ path: "C:/Users/you/Saved Games/Nebula Drift", files: 6, modified: now - 2 * 3600 }];
        case "backup_saves":
          return { id: "3", at: Math.floor(Date.now() / 1000), reason: "manual", files: 6, bytes: 2_400_000 };
        case "restore_saves":
          await sleep(400);
          return null;
        case "plugin:window|is_maximized":
          return false;
        case "plugin:app|version":
          return "0.1.0";
        default:
          return null;
      }
    },
    { shouldMockEvents: true },
  );
}
