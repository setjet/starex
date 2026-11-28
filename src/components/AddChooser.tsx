import { For, type JSX } from "solid-js";
import epicLogo from "../assets/epic-logo.webp";
import steamLogo from "../assets/steam-logo.webp";
import { type Dialog as DialogState, setDialog } from "../lib/store";
import { ChevronRight, FolderInput } from "./icons";
import { Dialog } from "./ui/Dialog";

interface Choice {
  title: string;
  text: string;
  mark: JSX.Element;
  /** The mark is the launcher's own logo, shown on its own without a tile behind it. */
  logo?: boolean;
  open: DialogState;
}

const choices: Choice[] = [
  {
    title: "Steam",
    text: "Every game Steam has installed on this PC.",
    mark: <img class="add-choice-logo" src={steamLogo} alt="" draggable={false} />,
    logo: true,
    open: { type: "import", store: "steam" },
  },
  {
    title: "Epic Games",
    text: "Every game the Epic Games Launcher has installed.",
    mark: <img class="add-choice-logo" src={epicLogo} alt="" draggable={false} data-dark />,
    logo: true,
    open: { type: "import", store: "epic" },
  },
  {
    title: "Game folder",
    text: "Any other game, from its files on your PC.",
    mark: <FolderInput />,
    open: { type: "addFolder" },
  },
];

/** "Add game": import from a launcher, or add any game from its folder. */
export function AddChooser() {
  return (
    <Dialog
      title="Import your games"
      description="Bring in the games you already have, or add one from its files."
      onClose={() => setDialog(null)}
      size="md"
    >
      <div class="add-choices">
        <For each={choices}>
          {(choice, i) => (
            <button
              type="button"
              class="add-choice"
              onClick={() => setDialog(choice.open)}
              data-autofocus={i() === 0 || undefined}
            >
              <span class="add-choice-mark" data-logo={choice.logo || undefined} aria-hidden="true">
                {choice.mark}
              </span>
              <span class="add-choice-copy">
                <span class="add-choice-title">{choice.title}</span>
                <span class="add-choice-text">{choice.text}</span>
              </span>
              <ChevronRight class="add-choice-arrow" aria-hidden="true" />
            </button>
          )}
        </For>
      </div>
    </Dialog>
  );
}
