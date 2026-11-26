// Mirrors of the Rust types the commands return (serde, camelCase).

export type Arch = "x86" | "x64" | "arm64" | "arm" | "universal" | "unknown";
export type HostSupport = "native" | "emulated" | "unsupported" | "compat";
export type CheckStatus = "info" | "pass" | "warn" | "fail";
export type VerifyMode = "quick" | "full";
export type ArtworkKind = "banner" | "cover" | "logo" | "icon";
export type LaunchBehavior = "keepOpen" | "minimize" | "hide";
export type LibrarySort = "recent" | "name" | "playtime";

export interface Artwork {
  banner: string | null;
  cover: string | null;
  logo: string | null;
  icon: string | null;
}

export interface EnvVar {
  key: string;
  value: string;
}

export interface VerificationSummary {
  at: number;
  mode: VerifyMode;
  status: CheckStatus;
  filesChecked: number;
  missing: number;
  modified: number;
  durationMs: number;
}

export interface Game {
  id: string;
  name: string;
  installDir: string;
  executable: string;
  launchArgs: string;
  workingDir: string | null;
  env: EnvVar[];
  artwork: Artwork;
  favorite: boolean;
  addedAt: number;
  lastPlayed: number | null;
  playtimeSecs: number;
  sessions: number;
  lastVerification: VerificationSummary | null;
  saveDir: string | null;
  backupSaves: boolean;
  lastScan: ScanSummary | null;
}

export type ScanStatus = "clean" | "threats" | "unavailable" | "cancelled";

export interface ScanSummary {
  at: number;
  status: ScanStatus;
  threats: number;
  filesScanned: number;
}

export interface ScanReport {
  gameId: string;
  status: ScanStatus;
  engine: string;
  filesScanned: number;
  bytesScanned: number;
  skipped: number;
  threats: { path: string; name: string | null }[];
  message: string | null;
  durationMs: number;
  at: number;
}

export type BackupReason = "beforeLaunch" | "afterSession" | "manual" | "beforeRestore";

export interface SaveBackup {
  id: string;
  at: number;
  reason: BackupReason;
  files: number;
  bytes: number;
}

export interface SaveLocation {
  path: string;
  files: number;
  modified: number | null;
}

export interface GamePatch {
  name?: string;
  executable?: string;
  launchArgs?: string;
  workingDir?: string | null;
  env?: EnvVar[];
  favorite?: boolean;
  saveDir?: string | null;
  backupSaves?: boolean;
}

export interface Settings {
  launchBehavior: LaunchBehavior;
  restoreOnExit: boolean;
  verifyBeforeLaunch: boolean;
  checkForUpdates: boolean;
  librarySort: LibrarySort;
  autoArtwork: boolean;
  steamgriddbKey: string;
  reduceMotion: boolean;
  scanNewGames: boolean;
  winePath: string;
}

export interface RunningGame {
  gameId: string;
  startedAt: number;
}

export interface Bootstrap {
  games: Game[];
  settings: Settings;
  running: RunningGame[];
  dataDir: string;
  hostArch: string;
  hostOs: string;
  repository: string;
}

export type NoteLevel = "info" | "warn" | "error";

export interface ExecutableCandidate {
  path: string;
  relative: string;
  size: number;
  arch: Arch | null;
  subsystem: "gui" | "console" | "other" | null;
  problem: "unreadable" | "notExecutable" | null;
  host: HostSupport | null;
  runnable: boolean;
  score: number;
}

export interface FolderInspection {
  root: string;
  suggestedName: string;
  candidates: ExecutableCandidate[];
  package: "microsoftGdk" | "msix" | "xboxContainer" | "steam" | null;
  artwork: Artwork;
  hasManifest: boolean;
  fileCount: number;
  totalBytes: number;
  notes: { level: NoteLevel; text: string }[];
}

export interface Check {
  id: string;
  title: string;
  detail: string;
  status: CheckStatus;
}

export interface FileIssue {
  path: string;
  kind: "missing" | "sizeMismatch" | "hashMismatch" | "unreadable";
  optional: boolean;
}

export interface VerifyReport {
  gameId: string;
  mode: VerifyMode;
  status: CheckStatus;
  checks: Check[];
  issues: FileIssue[];
  manifestSource: "gameFolder" | "baseline" | null;
  filesChecked: number;
  bytesChecked: number;
  missing: number;
  modified: number;
  durationMs: number;
  cancelled: boolean;
  at: number;
}

export interface BaselineInfo {
  files: number;
  totalBytes: number;
  generatedAt: number | null;
  shipped: boolean;
}

export interface Progress {
  doneFiles: number;
  totalFiles: number;
  doneBytes: number;
  totalBytes: number;
}

export interface TaskProgress {
  gameId: string;
  task: "verify" | "baseline" | "scan";
  progress: Progress;
  /** The task ended; its progress bar goes away. */
  done?: boolean;
}

export type SessionEvent =
  | { state: "running"; gameId: string; startedAt: number }
  | {
      state: "exited";
      gameId: string;
      seconds: number;
      exitCode: number | null;
      exitHint: string | null;
      game: Game | null;
    };
