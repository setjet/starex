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
    tags: [],
    hidden: false,
    completed: false,
    history: [],
    profiles: [],
    activeProfile: null,
    showOnDiscord: true,
    offline: false,
    source: null,
    storePlaytime: null,
    runAsAdmin: false,
    preLaunch: "",
    ...extra,
  };
}

/** Two weeks of believable sessions for the activity chart. */
function demoHistory() {
  const sessions: { start: number; end: number }[] = [];
  const lengths = [0, 1.6, 0.7, 0, 2.4, 1.1, 3.2, 0.5, 0, 1.9, 2.7, 0, 1.3, 0.8];
  lengths.forEach((hours, i) => {
    if (hours === 0) return;
    const start = now - (13 - i) * day - 20 * 3600 + 18 * 3600;
    sessions.push({ start, end: start + Math.round(hours * 3600) });
  });
  return sessions;
}

const games: Game[] = [
  game("nebula", "Nebula Drift", {
    favorite: true,
    source: { store: "steam", id: "4000" },
    storePlaytime: { totalSecs: 182 * 3600 + 40 * 60, recentSecs: 6 * 3600 + 15 * 60, lastPlayed: now - 2 * 3600, updatedAt: now },
    lastPlayed: now - 2 * 3600,
    playtimeSecs: 51_720,
    sessions: 23,
    launchArgs: "-dx12 -windowed",
    saveDir: "C:/Users/you/Saved Games/Nebula Drift",
    lastScan: { at: now - 3 * day, status: "clean", threats: 0, filesScanned: 38 },
    tags: ["Ported", "Sci-fi"],
    history: demoHistory(),
    profiles: [
      { id: "dx11", name: "DX11", launchArgs: "-dx11", executable: null, env: [] },
      { id: "modded", name: "Modded", launchArgs: "-dx12 -mods", executable: null, env: [] },
    ],
    activeProfile: "dx11",
    lastVerification: { at: now - 3 * 3600, mode: "full", status: "pass", filesChecked: 4_812, missing: 0, modified: 0, durationMs: 6_400 },
  }),
  game("hollow", "Hollow Meridian", { lastPlayed: now - 3 * day, playtimeSecs: 9_300, sessions: 6 }),
  game("ember", "Ember & Ash", {
    tags: ["Ported"],
    completed: true,
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
  autoDownloadUpdates: true,
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
  backdrop: "none",
  backdropImage: "",
  backdropStrength: 60,
  accent: "blue",
  theme: "dark",
  launchAnimation: true,
  welcomeSeen: false,
  steamApiKey: "",
  steamId: "",
  autoImportStores: false,
  removedStoreGames: [],
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
          return { games, settings, running: [], dataDir: "C:\\Users\\you\\AppData\\Roaming\\Starex", hostArch: "x86_64", hostOs: "windows", startHidden: false, discordBuiltin: false, repository: "https://github.com/setjet/starex", userName: "Alex", testBuild: null };
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
        case "set_offline": {
          await sleep(600);
          const g = games.find((x) => x.id === id);
          if (g) g.offline = args.offline as boolean;
          return g;
        }
        case "check_runtimes":
          // Ember & Ash shows the missing-runtime dialog.
          return id === "ember"
            ? [
                {
                  id: "vc2015-x64",
                  name: "Visual C++ 2015–2022 (x64)",
                  missing: ["vcruntime140.dll", "msvcp140.dll"],
                  installed: false,
                  installer: "D:/Games/EmberAsh/_CommonRedist/vcredist/2022/VC_redist.x64.exe",
                  download: "https://aka.ms/vs/17/release/vc_redist.x64.exe",
                },
                {
                  id: "directx",
                  name: "DirectX End-User Runtime (June 2010)",
                  missing: ["d3dx9_43.dll"],
                  installed: false,
                  installer: null,
                  download: "https://www.microsoft.com/en-us/download/details.aspx?id=35",
                },
              ]
            : [];
        case "fetch_changelog":
          await sleep(700);
          return [
            {
              version: "0.2.0",
              name: "v0.2.0",
              notes:
                "## [0.2.0](https://github.com/setjet/starex/compare/v0.1.0...v0.2.0) (2026-11-20)\n\n\n### Features\n\n* **settings:** changelog popup with release notes from GitHub ([#14](https://github.com/setjet/starex/issues/14)) ([a1b2c3d](https://github.com/setjet/starex/commit/a1b2c3d))\n* **library:** sort games by play time ([#12](https://github.com/setjet/starex/issues/12)) ([b2c3d4e](https://github.com/setjet/starex/commit/b2c3d4e))\n\n### Bug Fixes\n\n* toasts no longer stack behind open dialogs ([#13](https://github.com/setjet/starex/issues/13)) ([c3d4e5f](https://github.com/setjet/starex/commit/c3d4e5f))\n",
              url: "https://github.com/setjet/starex/releases/tag/v0.2.0",
              publishedAt: "2026-11-20T12:00:00Z",
              prerelease: false,
            },
            {
              version: "0.1.0",
              name: "v0.1.0",
              notes:
                "## 0.1.0 (2026-11-01)\n\n\n### Features\n\n* initial release of Starex ([f2918f3](https://github.com/setjet/starex/commit/f2918f3))\n",
              url: "https://github.com/setjet/starex/releases/tag/v0.1.0",
              publishedAt: "2026-11-01T12:00:00Z",
              prerelease: false,
            },
          ];
        case "scan_stores":
          await sleep(900);
          return [
            ["steam", "220", "Half Light 2", false],
            ["steam", "4000", "Nebula Drift", true],
            ["steam", "4100", "Copper Coast", false],
            ["steam", "4200", "Lantern Keep", false],
            ["steam", "4300", "Velvet Circuit", false],
            ["epic", "ns:item:quail", "Quiet Harbor", false],
            ["epic", "ns:item:moss", "Mossbound", false],
            ["gog", "1207658924", "The Long Road", false],
            ["xbox", "Starfall", "Starfall", false],
          ]
            .filter(([store]) => !args.store || store === args.store)
            .map(([store, sid, name, inLibrary]) => ({
            source: { store, id: sid },
            name,
            installDir: `D:\\${store}\\${String(name).replace(/\W+/g, "")}`,
            executable: `D:\\${store}\\${String(name).replace(/\W+/g, "")}\\game.exe`,
            artwork: { banner: null, cover: null, logo: null, icon: null },
            inLibrary,
          }));
        case "store_status":
          return [];
        case "sync_steam_playtime":
          return 0;
        case "auto_import_stores":
          await sleep(600);
          return settings.autoImportStores
            ? [game(`auto-${Date.now()}`, "Copper Coast", { source: { store: "steam", id: "4100" }, addedAt: Math.floor(Date.now() / 1000) })]
            : [];
        case "import_games": {
          await sleep(500);
          const list = args.games as { source: Game["source"]; name: string; installDir: string; executable: string }[];
          // Fresh objects, like the real backend; the store adds them to the library.
          return list.map((g, i) => game(`import-${Date.now()}-${i}`, g.name, { source: g.source, installDir: g.installDir, executable: g.executable, addedAt: Math.floor(Date.now() / 1000) }));
        }
        case "steam_account":
          return { steamId: "76561198000000000", name: "you" };
        case "steam_news":
          await sleep(500);
          if (games.find((g) => g.id === id)?.source?.store !== "steam") throw "This is not a Steam game.";
          return [
            { gid: "1", title: "Patch 1.4: smoother warp jumps", summary: "Warp jumps no longer stutter on large stations. Docking is faster, and the freighter's cargo hold remembers its layout between sessions.", date: now - 2 * day, source: "Community Announcements" },
            { gid: "2", title: "The Storm Season update is live", summary: "A new season of storms rolls through the nebula with three new stations, a story chapter and photo mode.", date: now - 16 * day, source: "Community Announcements" },
            { gid: "3", title: "Nebula Drift is 30% off this week", summary: "Celebrate the anniversary with the biggest discount yet.", date: now - 40 * day, source: "Steam" },
          ];
        case "steam_achievements":
          await sleep(500);
          if (games.find((g) => g.id === id)?.source?.store !== "steam") throw "This is not a Steam game.";
          if (!settings.steamApiKey) throw "Add a Steam Web API key in Settings > Integrations to see achievements.";
          return {
            total: 24,
            unlocked: 9,
            items: Array.from({ length: 24 }, (_, i) => ({
              name: ["First Light", "Salvager", "Storm Chaser", "Trader", "Long Haul", "Dockmaster"][i % 6] + (i >= 6 ? ` ${Math.floor(i / 6) + 1}` : ""),
              description: "Reach a milestone in the nebula.",
              icon: "",
              iconGray: "",
              achieved: i < 9,
              unlockedAt: i < 9 ? now - i * 3 * day : null,
              percent: Math.max(1, 80 - i * 3.2),
            })),
          };
        case "open_steam_news":
          return null;
        case "launch_link":
          return `starex://play/${id}`;
        case "take_launch_request":
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
