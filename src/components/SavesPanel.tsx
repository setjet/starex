import { createResource, Show } from "solid-js";
import { api } from "../lib/api";
import { count, timeAgo } from "../lib/format";
import { backupSaves, savesVersion, setDialog } from "../lib/store";
import type { Game } from "../lib/types";
import { Button } from "./ui/Button";

/** Where the game saves, and its latest backup. */
export function SavesPanel(props: { game: Game }) {
  const [backups] = createResource(
    () => [props.game.id, props.game.saveDir, savesVersion()] as const,
    ([id]) => api.listSaveBackups(id).catch(() => []),
  );
  const latest = () => backups()?.[0];

  return (
    <section class="panel" aria-labelledby="saves-title">
      <header class="panel-header-inline">
        <h2 id="saves-title" class="panel-eyebrow">
          Saves
        </h2>
        <Button variant="ghost" size="sm" onClick={() => setDialog({ type: "properties", id: props.game.id, tab: "saves" })}>
          {props.game.saveDir ? "Change" : "Set up"}
        </Button>
      </header>

      <Show
        when={props.game.saveDir}
        fallback={
          <p class="panel-note">
            Tell Starex where this game saves, and it backs your progress up after every session.
          </p>
        }
      >
        {(dir) => (
          <>
            <dl class="detail-list">
              <div>
                <dt>Folder</dt>
                <dd>
                  <button type="button" class="link-path" title={dir()} onClick={() => void api.reveal(props.game.id, "saves")}>
                    {dir()}
                  </button>
                </dd>
              </div>
              <div>
                <dt>Latest backup</dt>
                <dd>
                  <Show when={latest()} fallback={<span class="muted">None yet</span>}>
                    {(b) => (
                      <>
                        {timeAgo(b().at)} · {count(b().files, "file")}
                      </>
                    )}
                  </Show>
                </dd>
              </div>
              <Show when={!props.game.backupSaves}>
                <div>
                  <dt>Automatic backups</dt>
                  <dd class="muted">Off</dd>
                </div>
              </Show>
            </dl>
            <div class="panel-actions">
              <Button size="sm" onClick={() => void backupSaves(props.game.id)}>
                Back up now
              </Button>
              <Button
                size="sm"
                variant="ghost"
                disabled={!backups()?.length}
                onClick={() => setDialog({ type: "saves", id: props.game.id })}
              >
                Restore…
              </Button>
            </div>
          </>
        )}
      </Show>
    </section>
  );
}
