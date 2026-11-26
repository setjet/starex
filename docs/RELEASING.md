# Releasing

Releases build and publish themselves (see [CONTRIBUTING.md](../CONTRIBUTING.md#releases)). This
page is the one-time setup for maintainers.

## 1. Create the repository

Create `setjet/starex` on GitHub and push `main`. In **Settings > General > Pull Requests**:

- allow **squash merging** only, with the default commit message set to
  **Pull request title** (this is what produces `feat: something (#12)` on `main`);
- turn on **Automatically delete head branches**.

In **Settings > Actions > General > Workflow permissions**, choose **Read and write permissions**
and tick **Allow GitHub Actions to create and approve pull requests** (release-please needs both).

## 2. Add the updater signing key

Updates are signed. The key pair was generated with:

```powershell
bunx tauri signer generate -w $HOME\.tauri\starex.key
```

The public key is already in `src-tauri/tauri.conf.json`. Add the **private** key as repository
secrets (**Settings > Secrets and variables > Actions**):

| Secret | Value |
| --- | --- |
| `TAURI_SIGNING_PRIVATE_KEY` | The contents of `%USERPROFILE%\.tauri\starex.key` |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | The key's password (empty if it has none) |

Back the private key up somewhere safe. If it is lost, installed copies can no longer verify
updates, and users must reinstall by hand once with a new key.

## 3. Release

Merge pull requests with Conventional Commit titles. release-please keeps a `chore: release x.y.z`
pull request open; merge it when you want to ship. The release workflow then builds Windows (x64
NSIS installer), macOS (universal `.dmg` and `.app`) and Linux (x64 AppImage and `.deb`) and
publishes them with `latest.json`.

### Optional: operating-system code signing

Updates are always signed with the updater key above. Code signing by the operating system is
separate and optional: without it, Windows SmartScreen shows "unknown publisher" on first install,
and macOS asks users to right-click > Open the first time. To sign, add a Windows code-signing
certificate or an Apple Developer ID; see Tauri's guides for
[Windows](https://v2.tauri.app/distribute/sign/windows/) and
[macOS](https://v2.tauri.app/distribute/sign/macos/).

## Building a release locally

```powershell
$env:TAURI_SIGNING_PRIVATE_KEY = Get-Content -Raw $HOME\.tauri\starex.key
$env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = ""
bun run build
```

The installer and its `.sig` file are in `target/release/bundle/nsis` (Windows),
`target/release/bundle/dmg` (macOS) or `target/release/bundle/appimage` (Linux).
