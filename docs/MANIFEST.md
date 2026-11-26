# File manifests

A manifest lists the files a game needs. Starex uses it to tell a complete game from a broken one.

There are two kinds:

- **A shipped manifest**: `starex.manifest.json` in the game's folder, written by whoever
  distributes the files. It takes precedence.
- **A baseline**: recorded by Starex from your own copy (game page > Record baseline) while the
  game works. Kept in `%APPDATA%\Starex\baselines`.

## Format

```json
{
  "format": 1,
  "name": "Example Game",
  "executable": "bin/Example.exe",
  "generatedAt": 1764158400,
  "generator": "Starex 0.1.0",
  "files": [
    { "path": "bin/Example.exe", "size": 48211968, "blake3": "4f1c...e9" },
    { "path": "data/pack0.pak", "size": 2147483648, "blake3": "a0b2...77" },
    { "path": "movies/intro.bk2", "size": 31457280, "optional": true }
  ]
}
```

| Field | Meaning |
| --- | --- |
| `format` | Always `1` for now. Starex refuses newer formats instead of guessing. |
| `executable` | Optional. The file to start, relative to the game folder. |
| `files[].path` | Relative to the game folder, with `/` separators. Paths that leave the folder (`..`, absolute paths) are ignored. |
| `files[].size` | Size in bytes. Checked in quick and full mode. |
| `files[].blake3` | Optional lower-case hex BLAKE3 hash. Checked in full mode. |
| `files[].optional` | A missing optional file is a warning, not a failure. |

## Results

| Finding | Status |
| --- | --- |
| Every file present with the right size (and hash, in full mode) | Pass |
| A file's size or hash differs, an optional file is missing, or a file can't be read | Warning: updates and settings files can cause this |
| A required file is missing | Fail |

To create a manifest for a game you distribute, add the game to Starex, record a baseline, and
copy the file from `%APPDATA%\Starex\baselines\<game-id>.json` to `starex.manifest.json` in the
game folder.
