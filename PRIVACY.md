# Privacy

Starex has no account, no analytics, no telemetry and no crash reporting. Your library and save
backups stay on your computer, in Starex's data folder.

Starex connects to the internet only for these things:

| When | Where | What is sent |
| --- | --- | --- |
| Checking for updates (when Starex starts, or when you click Check now). With Automatically download updates on (Settings > Updates), a new version is also downloaded right away | `github.com/setjet/starex` releases | A normal download request. |
| Finding artwork (only after you add your own SteamGridDB API key) | `steamgriddb.com` | The game's name and your API key. |
| Showing news for a Steam game (when you open its page) | `api.steampowered.com` | The game's Steam app ID. |
| Showing achievements for a Steam game (only after you add your own Steam Web API key, when you open its page) | `api.steampowered.com`, and Steam's image servers for the achievement icons | The game's Steam app ID, your API key and your Steam ID. |
| Reading Steam's play time for your Steam games (only after you add your own Steam Web API key; when Starex starts and after you play a Steam game) | `api.steampowered.com` | Your API key and your Steam ID. |
| Finding save folders (weekly, if enabled in Settings > Games) | The public Ludusavi manifest on `raw.githubusercontent.com` | A normal download request. Your games are looked up on your computer. |
| Installing a missing runtime that the game does not ship (only when you click Download) | Microsoft's download pages | A normal download request. |
| Showing release notes (when you open them) | `api.github.com` | A normal download request. |
| Opening Settings > About | `unavatar.io` | A normal image request for the maintainer's profile picture. |

The Discord status is sent to the Discord app running on your computer, over its local
connection; Discord then shows it to your friends as usual. For a game Steam knows, the status
includes the address of the game's picture on Steam's image servers, which Discord loads. Turn the
status off in Settings > Games, or per game in Properties.

To import games from Steam and Epic Games, Starex reads the lists of installed games those
launchers keep on your computer, and Steam's own record of when you last played. This stays on
your computer. Starex starts those games by asking the launcher on your computer to start them.

Virus scans run on your computer, with the antivirus you already have. What your antivirus does
with what it scans (for example Microsoft Defender's cloud protection) follows that antivirus's
own settings.
