# Privacy

Starex has no account, no analytics, no telemetry and no crash reporting. Your library and save
backups stay on your computer, in Starex's data folder.

Starex connects to the internet only for these things:

| When | Where | What is sent |
| --- | --- | --- |
| Checking for updates (on start, if enabled in Settings > Updates, or when you click Check now) | `github.com/setjet/starex` releases | A normal download request. |
| Finding artwork (only after you add your own SteamGridDB API key) | `steamgriddb.com` | The game's name and your API key. |
| Finding save folders (weekly, if enabled in Settings > Games) | The public Ludusavi manifest on `raw.githubusercontent.com` | A normal download request. Your games are looked up on your computer. |
| Finding game details (only after you add your own IGDB client ID and secret) | `id.twitch.tv` and `api.igdb.com` | Your IGDB credentials and the game's name. |
| Installing a missing runtime that the game does not ship (only when you click Download) | Microsoft's download pages | A normal download request. |
| Showing release notes (when you open them) | `api.github.com` | A normal download request. |
| Opening Settings > About | `unavatar.io` | A normal image request for the maintainer's profile picture. |

The Discord status is sent to the Discord app running on your computer, over its local
connection; Discord then shows it to your friends as usual. Turn it off in Settings > Integrations, or
per game in Properties.

Virus scans run on your computer, with the antivirus you already have. What your antivirus does
with what it scans (for example Microsoft Defender's cloud protection) follows that antivirus's
own settings.
