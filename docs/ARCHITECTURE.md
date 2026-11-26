# Architecture

Starex is a Rust workspace with a web interface rendered by the system webview (WebView2 on
Windows, WebKit on macOS and Linux) through [Tauri 2](https://tauri.app). For a plain-language
tour, read [How Starex works](HOW_IT_WORKS.md) first.

```
crates/starex-core   Game logic. Plain Rust, no UI, fully unit-tested.
src-tauri            The desktop app: Tauri commands, play sessions, artwork, updates.
src                  The interface: SolidJS components and CSS on shared design tokens.
```

## starex-core

| Module | Job |
| --- | --- |
| `library` | The game list in `library.json`. Every change is written atomically (temp file + rename). An unreadable file is moved aside, never overwritten. |
| `inspect` | Walks a folder once: counts files, ranks executables, detects GDK/MSIX/Xbox/Steam layouts, finds artwork. |
| `pe` | Reads a Windows executable's header: processor architecture, GUI or console. |
| `binary` | Recognises programs on every platform (PE, ELF, Mach-O, `.app` bundles, `#!` scripts) and decides whether this computer runs them natively, through emulation (Windows on ARM, Rosetta), through Wine, or not at all. |
| `antivirus` | Scans a game's code files with the computer's antivirus: AMSI on Windows (Microsoft Defender or any registered antivirus), ClamAV on macOS and Linux. Data files are skipped, so scans take seconds. |
| `saves` | Suggests save folders, backs them up (only when something changed, keeping 20), restores a backup after first backing up the current saves, and tests whether a folder is writable. |
| `manifest` | The file list format (`starex.manifest.json` and baselines) and BLAKE3 hashing, parallel across files with Rayon and across chunks of large files. |
| `verify` | Checks the folder, the executable and every file in the manifest. Quick mode compares sizes; full mode compares hashes. Reports progress at most every 80 ms. |
| `launch` | Splits launch options like a shell and builds the process command, through Wine for Windows games on macOS and Linux and through the inner program for `.app` bundles. |
| `settings` | Preferences in `settings.json`. Unknown fields are ignored and missing ones take defaults, so versions can share the file. |

## src-tauri

- `commands.rs` — everything the interface can call. Slow work runs on blocking threads, and
  long tasks are cancellable.
- `sessions.rs` — starts a game and follows it. A session lasts while the started process *or
  any process running from the game folder* is alive, so play time stays right when a launcher
  hands over to the real game.
- Before the first launch and after every session, `sessions.rs` backs up the game's save folder.
  A game added to the library is virus-scanned in the background (`commands.rs`).
- `artwork.rs` — copies images into `%APPDATA%\Starex\artwork\<game>` (a new file name per
  version, so the webview never shows a stale image) and fetches from SteamGridDB.

The window may load images only from the artwork folder (asset protocol scope), and can call only
the commands listed in `main.rs` plus the plugin permissions in `capabilities/default.json`.

## Interface

- `src/lib/store.ts` holds all state in one Solid store and wraps each command in an action that
  reports its own errors as toasts.
- `src/lib/api.ts` is the typed bridge to the Rust commands; `src/lib/types.ts` mirrors the Rust
  types.
- `src/styles/tokens.css` defines every colour, size, radius and duration. It follows the design
  language of Burn: neutral near-black surfaces, hairline borders, a light primary action and one
  blue accent.

## Errors

- Every command returns a sentence the user can act on; the interface shows it in a toast.
- Failed starts and well-known crash codes are translated into causes (a missing Visual C++
  runtime, a blocked file, a wrong-architecture DLL) in `sessions.rs`.
- A Rust panic is written to the log before the process ends. If the data folder cannot be opened
  at start, a native error dialog says why.
- In the interface, unhandled promise rejections become toasts, and an error boundary keeps a
  broken view from blanking the window.

## Data folder

`%APPDATA%\Starex` on Windows, `~/Library/Application Support/Starex` on macOS and
`~/.local/share/Starex` on Linux:

```
library.json
settings.json
baselines/<game-id>.json
saves/<game-id>/<backup-id>/files/...     + backup.json
artwork/<game-id>/banner-<stamp>.jpg ...
```

Logs go to the system's log folder for `com.setjet.starex` (on Windows,
`%LOCALAPPDATA%\com.setjet.starex\logs`).
