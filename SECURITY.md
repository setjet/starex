# Security

Please report security problems privately through
[GitHub security advisories](https://github.com/setjet/starex/security/advisories/new), not in a
public issue. You should hear back within a week.

## How Starex protects you

- **Signed updates.** Every update is signed with the project's private key, and Starex refuses
  an update whose signature does not match the public key built into it.
- **A narrow window.** The interface can call only Starex's own commands. It cannot read or run
  arbitrary files; it can display images only from Starex's artwork folder.
- **Manifests are data.** A `starex.manifest.json` in a game folder is only ever read. Paths that
  try to leave the game folder are ignored.
- **Starex runs what you choose.** It starts the executable you picked, with your options, and
  nothing else. Only add games from sources you trust.
