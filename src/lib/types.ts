// Mirrors of the Rust types the commands return (serde, camelCase).

export type Arch = "x86" | "x64" | "arm64" | "arm" | "universal" | "unknown";
export type HostSupport = "native" | "emulated" | "unsupported" | "compat";
export type CheckStatus = "info" | "pass" | "warn" | "fail";
export type VerifyMode = "quick" | "full";
export type ArtworkKind = "banner" | "cover" | "logo" | "icon";
export type LaunchBehavior = "keepOpen" | "minimize" | "hide";
export type LibrarySort = "recent" | "name" | "playtime";
export type Backdrop = "none" | "aurora" | "game" | "custom";
export type Accent = "blue" | "violet" | "pink" | "green" | "orange" | "white";
export type Theme = "system" | "dark" | "light";

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
  /** Play time the game's launcher keeps (Steam), shown in place of Starex's own where known. */
  storePlaytime: StorePlaytime | null;
  sessions: number;
  lastVerification: VerificationSummary | null;
  saveDir: string | null;
  backupSaves: boolean;
  lastScan: ScanSummary | null;
  tags: string[];
  hidden: boolean;
  completed: boolean;
  history: PlaySession[];
  profiles: LaunchProfile[];
  activeProfile: string | null;
  showOnDiscord: boolean;
  offline: boolean;
  source: StoreRef | null;
  runAsAdmin: boolean;
  preLaunch: string;
}

export type Store = "steam" | "epic" | "gog" | "xbox";

export interface StoreRef {
  store: Store;
  id: string;
}

/** A game another launcher installed, from `scan_stores`. */
export interface FoundGame {
  source: StoreRef;
  name: string;
  installDir: string;
  executable: string;
  artwork: Artwork;
  inLibrary: boolean;
}

export interface SteamAccount {
  steamId: string;
  name: string;
}

export interface NewsItem {
  gid: string;
  title: string;
  summary: string;
  date: number;
  source: string;
}

export interface Achievement {
  name: string;
  description: string;
  icon: string;
  iconGray: string;
  achieved: boolean;
  unlockedAt: number | null;
  percent: number | null;
}

export interface Achievements {
  total: number;
  unlocked: number;
  items: Achievement[];
}

export interface PlaySession {
  start: number;
  end: number;
}

export interface LaunchProfile {
  id: string;
  name: string;
  launchArgs: string;
  executable: string | null;
  env: EnvVar[];
}

export interface StorePlaytime {
  totalSecs: number | null;
  recentSecs: number | null;
  lastPlayed: number | null;
  updatedAt: number;
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
  fromDatabase: boolean;
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
  tags?: string[];
  hidden?: boolean;
  completed?: boolean;
  profiles?: LaunchProfile[];
  activeProfile?: string | null;
  showOnDiscord?: boolean;
  runAsAdmin?: boolean;
  preLaunch?: string;
}

export interface Settings {
  launchBehavior: LaunchBehavior;
  restoreOnExit: boolean;
  verifyBeforeLaunch: boolean;
  autoDownloadUpdates: boolean;
  librarySort: LibrarySort;
  autoArtwork: boolean;
  steamgriddbKey: string;
  reduceMotion: boolean;
  scanNewGames: boolean;
  winePath: string;
  saveDatabase: boolean;
  cloudBackupDir: string;
  discordPresence: boolean;
  discordAppId: string;
  closeToTray: boolean;
  startWithSystem: boolean;
  globalShortcut: string;
  backdrop: Backdrop;
  backdropImage: string;
  backdropStrength: number;
  accent: Accent;
  theme: Theme;
  launchAnimation: boolean;
  welcomeSeen: boolean;
  steamApiKey: string;
  steamId: string;
  /** When Starex opens, add games newly installed with Steam or the Epic Games Launcher. */
  autoImportStores: boolean;
  /** Launcher games the user removed ("steam:570"); kept by the app. */
  removedStoreGames: string[];
}

export interface Runtime {
  id: string;
  name: string;
  missing: string[];
  installed: boolean;
  installer: string | null;
  download: string | null;
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
  startHidden: boolean;
  discordBuiltin: boolean;
  /** The signed-in user's name, for the launch animation's welcome. */
  userName: string;
  /** The number of a private test build; null for every normal build. */
  testBuild: string | null;
}

/** One published version of Starex, from its GitHub releases. */
export interface Release {
  version: string;
  name: string;
  notes: string;
  url: string;
  publishedAt: string | null;
  prerelease: boolean;
}

export type NoteLevel = "info" | "warn" | "error";

export interface ExecutableCandidate {
  path: string;
  relative: string;
  size: number;
  format: "pe" | "elf" | "machO" | "appBundle" | "script" | "batch" | null;
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
  looksLikeGame: boolean;
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
