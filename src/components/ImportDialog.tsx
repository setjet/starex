import { createSignal, For, Match, onMount, Show, Switch } from "solid-js";
import { api, errorText, fileUrl } from "../lib/api";
import { generatedArt, monogram } from "../lib/art";
import { count } from "../lib/format";
import { importGame, refreshInstallStatus, select, setDialog } from "../lib/store";
import type { FoundGame, Store } from "../lib/types";
import { FadeImage } from "./GameArt";
import { Check, CircleX } from "./icons";
import { Button } from "./ui/Button";
import { Dialog } from "./ui/Dialog";
import { Progress, Spinner } from "./ui/Spinner";

export const storeLabels: Record<Store, string> = {
  steam: "Steam",
  epic: "Epic Games",
  gog: "GOG",
  xbox: "Xbox",
};

type RowState = "waiting" | "adding" | "added" | "failed";

/**
 * Imports every game one launcher installed: it looks, then adds them one by one while the list
 * shows each game's progress. Games already in the library are left as they are.
 */
export function ImportDialog(props: { store: Store }) {
  const label = storeLabels[props.store];
  const [phase, setPhase] = createSignal<"scanning" | "importing" | "done" | "error">("scanning");
  const [error, setError] = createSignal("");
  const [games, setGames] = createSignal<FoundGame[]>([]);
  const [known, setKnown] = createSignal(0);
  const [states, setStates] = createSignal<Record<number, RowState>>({});
  const [reasons, setReasons] = createSignal<Record<number, string>>({});
  const [firstAdded, setFirstAdded] = createSignal<string | null>(null);
  const rows: HTMLElement[] = [];

  const added = () => Object.values(states()).filter((s) => s === "added").length;
  const failed = () => Object.values(states()).filter((s) => s === "failed").length;
  const finished = () => added() + failed();

  onMount(async () => {
    let found: FoundGame[];
    try {
      found = await api.scanStores(props.store);
    } catch (e) {
      setError(errorText(e));
      setPhase("error");
      return;
    }
    const fresh = found.filter((g) => !g.inLibrary);
    setKnown(found.length - fresh.length);
    setGames(fresh);
    if (fresh.length === 0) {
      setPhase("done");
      return;
    }
    setPhase("importing");
    for (const [i, game] of fresh.entries()) {
      setStates((s) => ({ ...s, [i]: "adding" }));
      rows[i]?.scrollIntoView({ block: "nearest", behavior: "smooth" });
      try {
        const g = await importGame(game);
        if (!firstAdded()) setFirstAdded(g.id);
        setStates((s) => ({ ...s, [i]: "added" }));
      } catch (e) {
        setReasons((r) => ({ ...r, [i]: errorText(e) }));
        setStates((s) => ({ ...s, [i]: "failed" }));
      }
    }
    void refreshInstallStatus();
    setPhase("done");
  });

  const close = () => setDialog(null);
  const title = () => {
    if (phase() === "scanning") return `Looking for ${label} games`;
    if (phase() === "importing") return `Importing from ${label}`;
    if (phase() === "done" && added() > 0) return `${count(added(), "game")} imported`;
    if (phase() === "done" && failed() > 0) return "Nothing was imported";
    return `Import from ${label}`;
  };
  const description = () => {
    if (phase() === "importing") return `Adding ${Math.min(finished() + 1, games().length)} of ${games().length}`;
    if (phase() === "done" && games().length > 0) {
      return failed() > 0 ? `${count(failed(), "game")} could not be added.` : `Your ${label} games are in your library.`;
    }
    return undefined;
  };

  return (
    <Dialog
      title={title()}
      description={description()}
      onClose={close}
      busy={phase() === "importing"}
      size="md"
      class="import-dialog"
      footer={
        <Show when={phase() === "done" || phase() === "error"}>
          <div class="dialog-footer-spacer" />
          <Show
            when={firstAdded()}
            fallback={
              <Button variant="primary" onClick={close}>
                Done
              </Button>
            }
          >
            {(id) => (
              <Button
                variant="primary"
                onClick={() => {
                  select(id());
                  close();
                }}
              >
                Done
              </Button>
            )}
          </Show>
        </Show>
      }
    >
      <Switch>
        <Match when={phase() === "scanning"}>
          <div class="scanning" role="status">
            <Spinner size={22} />
            <span>Looking for installed games…</span>
          </div>
        </Match>

        <Match when={phase() === "error"}>
          <div class="import-empty" role="status">
            <p class="import-empty-title">Nothing to import</p>
            <p class="import-empty-text">{error()}</p>
          </div>
        </Match>

        <Match when={games().length === 0}>
          <div class="import-empty">
            <p class="import-empty-title">
              {known() > 0 ? `All your ${label} games are already here` : `No ${label} games found`}
            </p>
            <p class="import-empty-text">
              {known() > 0
                ? `${count(known(), "game")} from ${label} ${known() === 1 ? "is" : "are"} already in your library.`
                : `Starex found no games installed with ${label} on this PC.`}
            </p>
          </div>
        </Match>

        <Match when={games().length > 0}>
          <Progress value={games().length > 0 ? finished() / games().length : 0} />
          <div class="import-list" role="list" aria-label={`Games from ${label}`}>
            <For each={games()}>
              {(game, i) => (
                <div
                  ref={(el) => (rows[i()] = el)}
                  class="import-row"
                  role="listitem"
                  data-state={states()[i()] ?? "waiting"}
                >
                  <span class="import-art" style={{ background: generatedArt(game.name, 1.1) }} aria-hidden="true">
                    <FadeImage
                      src={fileUrl(game.artwork.icon ?? game.artwork.cover ?? game.artwork.banner)}
                      fallback={<span class="game-icon-monogram">{monogram(game.name)}</span>}
                    />
                  </span>
                  <span class="import-copy">
                    <span class="import-name">{game.name}</span>
                    <Show when={reasons()[i()]}>{(reason) => <span class="import-reason">{reason()}</span>}</Show>
                  </span>
                  <span class="import-status" aria-label={states()[i()] ?? "waiting"}>
                    <Switch>
                      <Match when={states()[i()] === "adding"}>
                        <Spinner size={14} />
                      </Match>
                      <Match when={states()[i()] === "added"}>
                        <Check />
                      </Match>
                      <Match when={states()[i()] === "failed"}>
                        <CircleX />
                      </Match>
                    </Switch>
                  </span>
                </div>
              )}
            </For>
          </div>
        </Match>
      </Switch>
    </Dialog>
  );
}
