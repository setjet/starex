# Contributing to Starex

Thanks for helping. Bug reports, ideas, documentation and code are all welcome. This page covers
how to get set up, how a change lands, and how versions are released.

## Ways to help

- **Report a bug** with the [bug report form](https://github.com/setjet/starex/issues/new/choose).
  Include your Starex version (Settings > About) and the log lines around the problem.
- **Suggest a feature** with the feature request form. Describe the problem first; the best
  solution often comes out of the discussion.
- **Fix something.** Issues labelled `good first issue` are small and well described. Comment on
  the issue so nobody else starts the same work.

## Setup

1. Install [Rust](https://rustup.rs) (stable) and [Bun](https://bun.sh) 1.4.
2. Install your system's build dependencies:
   - **Windows:** nothing more (WebView2 is built into Windows 11).
   - **macOS:** `xcode-select --install`
   - **Linux (Debian/Ubuntu):**
     `sudo apt install libwebkit2gtk-4.1-dev libappindicator3-dev librsvg2-dev patchelf libssl-dev libxdo-dev`
3. Fork the repository, clone your fork, and run `bun install`.
4. `bun run dev` starts the app. Interface changes reload instantly; Rust changes rebuild and
   restart it.

To work on the interface alone, `bun run dev:web` serves it in a browser with a demo library, no
Rust build needed.

Set `STAREX_DATA_DIR` to a scratch folder to develop against an empty library without touching
your own:

```sh
STAREX_DATA_DIR=/tmp/starex-dev bun run dev                     # macOS and Linux
$env:STAREX_DATA_DIR = "$env:TEMP\starex-dev"; bun run dev      # Windows PowerShell
```

## Where things are

| Path | What lives there |
| --- | --- |
| `crates/starex-core/src` | All game logic, one module per job: `library`, `inspect`, `binary`, `verify`, `manifest`, `antivirus`, `saves`, `launch`, `settings`. Every module has its tests at the bottom. |
| `src-tauri/src` | The desktop app: `commands.rs` (what the interface can call), `sessions.rs` (running games), `artwork.rs`. |
| `src` | The interface. `lib/store.ts` holds the state and actions; components are in `components/`; every colour and size is in `styles/tokens.css`. |
| `docs` | [How Starex works](docs/HOW_IT_WORKS.md), [architecture](docs/ARCHITECTURE.md), the [manifest format](docs/MANIFEST.md) and [releasing](docs/RELEASING.md). |

## Checks

Run these before you push. CI runs the same ones on Windows, macOS and Linux.

```sh
cargo fmt --all
cargo test --workspace
bun run lint          # rustfmt check, clippy with warnings as errors, TypeScript
```

With an antivirus running, `cargo test -p starex-core -- --ignored` also checks that virus
scanning really detects the harmless EICAR test signature.

## How a change lands

1. Create a branch on your fork, make the change, and add or update tests for it.
2. Open a pull request against `main` and fill in the template.
3. **Title it as a [Conventional Commit](https://www.conventionalcommits.org).** The title is
   checked automatically:

   | Prefix | Use it for | Effect on the version |
   | --- | --- | --- |
   | `feat:` | Something new for users | Minor (0.**2**.0) |
   | `fix:` | A bug fix | Patch (0.1.**1**) |
   | `perf:` | Faster, same behaviour | Patch |
   | `refactor:` `docs:` `test:` `build:` `ci:` `chore:` `style:` | Everything else | None |
   | `feat!:` or a `BREAKING CHANGE:` footer | An incompatible change | Major (minor before 1.0) |

4. A maintainer reviews it. When CI is green and the review is done, the pull request is
   **squash-merged**: GitHub writes one commit on `main` with the pull request's title and number,
   for example:

   ```
   feat: back up saves after every session (#42)
   ```

   The `(#42)` is the pull request number, a link back to the discussion, review and checks
   behind the commit. It is why the history on `main` reads as one tidy line per change.

## Releases

Releases are automatic. Nobody edits a version number by hand.

1. On every push to `main`, [release-please](https://github.com/googleapis/release-please) opens or
   updates a pull request titled `chore: release x.y.z`. It picks the next version from the
   commit prefixes above, writes the [changelog](CHANGELOG.md), and bumps the version in
   `Cargo.toml` and `package.json`.
2. Merging that pull request tags `vx.y.z` and creates the GitHub release.
3. The release workflow builds Windows (x64), macOS (universal) and Linux (x64), signs the builds
   with the updater key and uploads them with `latest.json`.
4. Installed copies of Starex find the new version on their next start and offer to update.

Maintainers: see [docs/RELEASING.md](docs/RELEASING.md) for the one-time setup.

## Style

- Rust: `rustfmt` and `clippy` clean, no `unwrap` outside tests. Errors are sentences a user can
  act on.
- Interface: colours, sizes, radii and durations come from `src/styles/tokens.css`. Hover styles
  sit inside `@media (hover: hover)`, and animations respect reduced motion.
- Copy: short, plain and specific. Say what happened and what to do next.

Everyone taking part follows the [code of conduct](CODE_OF_CONDUCT.md).
