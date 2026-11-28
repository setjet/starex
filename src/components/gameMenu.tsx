import {
  AppWindow,
  Eye,
  EyeOff,
  Flag,
  FolderOpen,
  Globe,
  Image,
  Link,
  Monitor,
  ScanSearch,
  ShieldCheck,
  SlidersHorizontal,
  Sparkles,
  Star,
  Trash2,
  WifiOff,
} from "./icons";
import { api } from "../lib/api";
import {
  copyLaunchLink,
  createShortcut,
  fetchArtwork,
  patchGame,
  setDialog,
  isStoreGame,
  setOffline,
  store,
  verify,
} from "../lib/store";
import type { Game } from "../lib/types";
import type { MenuItem } from "./ui/Menu";

/** A game's actions, opened by right-clicking it in the sidebar or the library. */
export function gameMenuItems(g: Game): MenuItem[] {
  const own = !isStoreGame(g);
  return [
    {
      label: "Properties…",
      icon: <SlidersHorizontal />,
      onSelect: () => setDialog({ type: "properties", id: g.id }),
    },
    {
      label: "Open game folder",
      icon: <FolderOpen />,
      onSelect: () => void api.reveal(g.id, "folder"),
    },
    ...(own
      ? [
          {
            label: "Check files",
            icon: <ShieldCheck />,
            disabled: !!store.tasks[g.id],
            onSelect: () => void verify(g.id, "quick"),
          },
        ]
      : []),
    {
      separatorBefore: true,
      label: g.favorite ? "Remove from favorites" : "Add to favorites",
      icon: <Star />,
      onSelect: () => void patchGame(g.id, { favorite: !g.favorite }),
    },
    {
      label: g.completed ? "Mark as unfinished" : "Mark as finished",
      icon: <Flag />,
      onSelect: () => void patchGame(g.id, { completed: !g.completed }),
    },
    {
      label: g.hidden ? "Show in library" : "Hide from library",
      icon: g.hidden ? <Eye /> : <EyeOff />,
      onSelect: () => void patchGame(g.id, { hidden: !g.hidden }),
    },
    {
      label: "Change artwork…",
      icon: <Image />,
      onSelect: () => setDialog({ type: "properties", id: g.id, tab: "artwork" }),
    },
    {
      label: "Find artwork online",
      icon: <Sparkles />,
      disabled: !store.settings.steamgriddbKey,
      onSelect: () => void fetchArtwork(g.id, false),
    },
    ...(own
      ? [
          {
            label: "Show executable",
            icon: <ScanSearch />,
            onSelect: () => void api.reveal(g.id, "executable"),
          },
        ]
      : []),
    ...(store.hostOs === "macos"
      ? []
      : [
          {
            label: "Create desktop shortcut",
            icon: <Monitor />,
            separatorBefore: true,
            onSelect: () => void createShortcut(g.id, "desktop"),
          },
          {
            label: store.hostOs === "linux" ? "Add to applications menu" : "Add to Start menu",
            icon: <AppWindow />,
            onSelect: () => void createShortcut(g.id, "startMenu"),
          },
        ]),
    {
      label: "Copy launch link",
      icon: <Link />,
      separatorBefore: store.hostOs === "macos",
      onSelect: () => void copyLaunchLink(g.id),
    },
    ...(store.hostOs === "windows" && own
      ? [
          {
            label: g.offline ? "Allow online access" : "Block online access",
            icon: g.offline ? <Globe /> : <WifiOff />,
            separatorBefore: true,
            onSelect: () => void setOffline(g.id, !g.offline),
          },
        ]
      : []),
    {
      label: "Remove from library…",
      icon: <Trash2 />,
      danger: true,
      separatorBefore: true,
      onSelect: () => setDialog({ type: "remove", id: g.id }),
    },
  ];
}
