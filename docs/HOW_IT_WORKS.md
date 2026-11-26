# How Starex works

Starex is a launcher for game files you already have. It does not download or sell games. You
point it at a folder, and it does the rest.

## 1. Adding a game

Click **Add game** (or press Ctrl+N, or drop a folder on the window) and choose the game's folder.
Starex then reads the whole folder once:

- It **finds the program to start.** Every program in the folder gets a score: a name that matches
  the folder, a large size and a place near the top count for it; uninstallers, redistributables,
  crash reporters, servers and editors count against it. The best one is picked for you, and you
  can choose another.
- It **reads each program's header** to learn which processor and system it was built for. That
  is how it knows whether your computer can run it, needs emulation or Wine, or cannot run it at
  all, for example an encrypted console build.
- It **looks for artwork** in the folder (banners, covers, logos, icons) and, if you added a
  SteamGridDB key, fetches the rest.
- It **scans for viruses** in the background: your antivirus checks the game's programs and
  scripts. Textures, audio and video cannot run by themselves, so they are skipped and the scan
  takes seconds.

## 2. Playing

Press **Play**. Starex starts the program with your launch options, then:

- minimises itself (or stays open, or hides: your choice in Settings);
- watches the game. Some games start through a small launcher that exits right away; Starex keeps
  counting as long as *any* program from the game's folder is running;
- when the game closes, records the play time, backs up the saves, and comes back.

If the game fails to start or crashes straight away, Starex explains the usual cause, such as a
missing Visual C++ runtime, instead of a bare error code.

## 3. Checking the files

Games break when files go missing or get damaged. On the game's page:

- **Record baseline** while the game works. Starex writes down every file with its size and a
  fingerprint (a BLAKE3 hash).
- **Quick check** compares every file's presence and size against that list in seconds.
- **Full check** also recomputes every fingerprint, using all your processor cores, and finds
  even a single changed byte.

Every check also confirms the game folder exists, the program can run here, and the game can
write its saves.

## 4. Saves

Under **Properties > Saves**, choose where the game saves. Starex suggests likely folders: Saved
Games, Documents\My Games, AppData, or a saves folder inside the game. From then on it:

- backs the saves up before the first launch and after every session (only when something
  changed), keeping the latest 20 copies;
- restores any backup in one click from the game page, after first backing up the saves you have
  now, so a restore never loses anything.

Ported games often keep their saves next to their own files. If the game sits in a folder Windows
protects, such as Program Files, it cannot save there; Starex warns you so you can move it.

## 5. Updates

Starex checks this project's GitHub releases when it starts. When a new version is out, a pill
appears in the title bar. Updates are signed with the project's key, and Starex refuses any
update whose signature does not match.

## Where your data lives

Everything stays on your computer, in Starex's data folder (Settings > Library > Data folder):

| File or folder | Holds |
| --- | --- |
| `library.json` | Your games, play time and the results of the latest checks |
| `settings.json` | Your settings |
| `baselines/` | The file lists recorded for each game |
| `saves/` | Save backups, one folder per backup |
| `artwork/` | Banner, cover, logo and icon images |

Removing a game from Starex never touches the game's own files, and keeps its save backups.
