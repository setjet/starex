import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  ArtworkKind,
  BaselineInfo,
  Bootstrap,
  FolderInspection,
  Game,
  GamePatch,
  SessionEvent,
  Settings,
  TaskProgress,
  VerifyMode,
  VerifyReport,
  Artwork,
  SaveBackup,
  SaveLocation,
  ScanReport,
  ScanSummary,
} from "./types";

/** Typed wrappers over the Rust commands in `src-tauri/src/commands.rs`. */
export const api = {
  bootstrap: () => invoke<Bootstrap>("bootstrap"),
  inspectFolder: (path: string) => invoke<FolderInspection>("inspect_folder", { path }),
  addGame: (
    game: { name: string; installDir: string; executable: string; launchArgs: string },
    detected: Artwork | null,
    scan: ScanSummary | null,
  ) => invoke<Game>("add_game", { game, detected, scan }),
  scanFolder: (path: string, taskId: string) => invoke<ScanReport>("scan_folder", { path, taskId }),
  updateGame: (id: string, patch: GamePatch) => invoke<Game>("update_game", { id, patch }),
  removeGame: (id: string) => invoke<void>("remove_game", { id }),
  setArtwork: (id: string, kind: ArtworkKind, source: string | null) =>
    invoke<Game>("set_artwork", { id, kind, source }),
  fetchArtwork: (id: string, overwrite: boolean) => invoke<Game>("fetch_artwork", { id, overwrite }),
  launch: (id: string) => invoke<number>("launch_game", { id }),
  stop: (id: string) => invoke<void>("stop_game", { id }),
  verify: (id: string, mode: VerifyMode) => invoke<VerifyReport>("verify_game", { id, mode }),
  createBaseline: (id: string) => invoke<BaselineInfo | null>("create_baseline", { id }),
  deleteBaseline: (id: string) => invoke<void>("delete_baseline", { id }),
  baselineInfo: (id: string) => invoke<BaselineInfo | null>("baseline_info", { id }),
  cancelTask: (id: string) => invoke<void>("cancel_task", { id }),
  saveSettings: (settings: Settings) => invoke<Settings>("save_settings", { settings }),
  reveal: (id: string, target: "folder" | "executable" | "saves") => invoke<void>("reveal", { id, target }),
  scanGame: (id: string) => invoke<ScanReport>("scan_game", { id }),
  suggestSaveLocations: (id: string) => invoke<SaveLocation[]>("suggest_save_locations", { id }),
  listSaveBackups: (id: string) => invoke<SaveBackup[]>("list_save_backups", { id }),
  backupSaves: (id: string) => invoke<SaveBackup | null>("backup_saves", { id }),
  restoreSaves: (id: string, backupId: string) => invoke<void>("restore_saves", { id, backupId }),
  openDataDir: () => invoke<void>("open_data_dir"),
  openLink: (url: string) => invoke<void>("open_link", { url }),
};

export const events = {
  session: (handler: (e: SessionEvent) => void): Promise<UnlistenFn> =>
    listen<SessionEvent>("starex://session", (e) => handler(e.payload)),
  gameUpdated: (handler: (g: Game) => void): Promise<UnlistenFn> =>
    listen<Game>("starex://game-updated", (e) => handler(e.payload)),
  scanFinished: (handler: (r: ScanReport) => void): Promise<UnlistenFn> =>
    listen<ScanReport>("starex://scan-finished", (e) => handler(e.payload)),
  taskProgress: (handler: (p: TaskProgress) => void): Promise<UnlistenFn> =>
    listen<TaskProgress>("starex://task-progress", (e) => handler(e.payload)),
};

/** A URL the webview can load for a file in Starex's artwork folder. */
export function fileUrl(path: string | null | undefined): string | undefined {
  return path ? convertFileSrc(path) : undefined;
}

/** Commands reject with a plain string; anything else gets a generic sentence. */
export function errorText(error: unknown): string {
  if (typeof error === "string") return error;
  if (error instanceof Error) return error.message;
  return "Something went wrong.";
}
