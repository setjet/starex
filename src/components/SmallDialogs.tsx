import { createResource, createSignal, For, Show } from "solid-js";
import { api, errorText } from "../lib/api";
import { bytes, count } from "../lib/format";
import { launch, missingRuntimes, removeGame, restoreSaves, savesVersion, scan, scanReports, setDialog } from "../lib/store";
import { toast } from "../lib/toast";
import type { BackupReason, Game, Runtime } from "../lib/types";
import { installUpdate, updater } from "../lib/updater";
import { versionLabel } from "../lib/version";
import { Button } from "./ui/Button";
import { Dialog } from "./ui/Dialog";
import { storeLabels } from "./ImportDialog";
import { Progress } from "./ui/Spinner";

export function RemoveDialog(props: { game: Game }) {
  const [busy, setBusy] = createSignal(false);
  const confirm = async () => {
    setBusy(true);
    await removeGame(props.game.id);
    setBusy(false);
    setDialog(null);
  };
  return (
    <Dialog
      title={`Remove ${props.game.name}?`}
      description={
        props.game.source
          ? `Starex forgets the game, its artwork and play time. It stays installed in ${storeLabels[props.game.source.store]}.`
          : "Starex forgets the game, its artwork, play time and baseline. The game's files and your save backups stay on your drive."
      }
      onClose={() => setDialog(null)}
      busy={busy()}
      size="sm"
      footer={
        <>
          <div class="dialog-footer-spacer" />
          <Button variant="secondary" onClick={() => setDialog(null)} data-autofocus>
            Cancel
          </Button>
          <Button variant="destructive" loading={busy()} onClick={() => void confirm()}>
            Remove
          </Button>
        </>
      }
    >
      {null}
    </Dialog>
  );
}

export function UpdateDialog() {
  const update = () => updater.available();
  const working = () => updater.status() === "downloading" || updater.status() === "installing";
  return (
    <Dialog
      title={`Starex ${update()?.version ?? ""}`}
      description={`You have ${versionLabel()}. Starex restarts to finish the update; running games keep running.`}
      onClose={() => setDialog(null)}
      busy={working()}
      size="md"
      footer={
        <>
          <div class="dialog-footer-spacer" />
          <Button variant="secondary" onClick={() => setDialog(null)} disabled={working()}>
            Later
          </Button>
          <Button variant="primary" loading={working()} onClick={() => void installUpdate()} data-autofocus>
            {working()
              ? updater.status() === "installing"
                ? "Installing"
                : "Downloading"
              : updater.downloaded()
                ? "Restart to update"
                : "Install and restart"}
          </Button>
        </>
      }
    >
      <Show when={working()}>
        <Progress value={updater.status() === "installing" ? null : updater.progress()} />
      </Show>
      <Show when={updater.status() === "error"}>
        <p class="callout" data-level="error">
          <span>{updater.error()}</span>
        </p>
      </Show>
      <div class="release-notes">{update()?.body?.trim() || "This release has no notes."}</div>
    </Dialog>
  );
}

const reasonLabels: Record<BackupReason, string> = {
  beforeLaunch: "Before first launch",
  afterSession: "After playing",
  manual: "Backed up by you",
  beforeRestore: "Before a restore",
};

/** Lists a game's save backups and restores one. */
export function SavesDialog(props: { game: Game }) {
  const [backups] = createResource(
    () => [props.game.id, savesVersion()] as const,
    ([id]) => api.listSaveBackups(id).catch(() => []),
  );
  const [confirming, setConfirming] = createSignal<string | null>(null);
  const [busy, setBusy] = createSignal(false);

  const restore = async (backupId: string) => {
    setBusy(true);
    const ok = await restoreSaves(props.game.id, backupId);
    setBusy(false);
    if (ok) setDialog(null);
  };

  return (
    <Dialog
      title="Restore saves"
      description={`Puts ${props.game.name}'s save folder back the way it was. Your current saves are backed up first, so nothing is lost.`}
      onClose={() => setDialog(null)}
      busy={busy()}
      size="md"
    >
      <Show when={(backups() ?? []).length > 0} fallback={<p class="panel-note">No backups yet.</p>}>
        <ul class="backup-list">
          <For each={backups()}>
            {(b) => (
              <li class="backup">
                <div class="backup-copy">
                  <span class="backup-when">{new Date(b.at * 1000).toLocaleString()}</span>
                  <span class="backup-detail">
                    {reasonLabels[b.reason]} · {count(b.files, "file")} · {bytes(b.bytes)}
                  </span>
                </div>
                <Show
                  when={confirming() === b.id}
                  fallback={
                    <Button size="sm" onClick={() => setConfirming(b.id)} disabled={busy()}>
                      Restore
                    </Button>
                  }
                >
                  <Button size="sm" variant="ghost" onClick={() => setConfirming(null)} disabled={busy()}>
                    Cancel
                  </Button>
                  <Button size="sm" variant="primary" loading={busy()} onClick={() => void restore(b.id)}>
                    Confirm
                  </Button>
                </Show>
              </li>
            )}
          </For>
        </ul>
      </Show>
    </Dialog>
  );
}

/** Asks before starting a game whose last virus scan found threats. */
export function ThreatsDialog(props: { game: Game }) {
  const report = () => scanReports[props.game.id];
  return (
    <Dialog
      title="Threats were found in this game"
      description={`Your antivirus flagged ${count(props.game.lastScan?.threats ?? 0, "file")} in ${props.game.name}. Starting it could harm your computer.`}
      onClose={() => setDialog(null)}
      size="sm"
      footer={
        <>
          <Button variant="ghost" class="btn-danger-ghost" onClick={() => {
            setDialog(null);
            void launch(props.game.id, { threats: true });
          }}>
            Play anyway
          </Button>
          <div class="dialog-footer-spacer" />
          <Button variant="secondary" onClick={() => {
            setDialog(null);
            void scan(props.game.id);
          }}>
            Scan again
          </Button>
          <Button variant="primary" onClick={() => setDialog(null)} data-autofocus>
            Don't play
          </Button>
        </>
      }
    >
      <Show when={report()?.threats.length}>
        <ul class="issue-list">
          <For each={report()?.threats}>
            {(t) => (
              <li class="issue">
                <span class="issue-kind" data-kind="missing">
                  Threat
                </span>
                <span class="issue-path">{t.path}</span>
              </li>
            )}
          </For>
        </ul>
      </Show>
    </Dialog>
  );
}

/** Before Play: runtimes the game needs that this computer is missing. */
export function RuntimesDialog(props: { game: Game; runtimes: Runtime[] }) {
  const [list, setList] = createSignal(props.runtimes);
  const [checking, setChecking] = createSignal(false);
  const [started, setStarted] = createSignal<Record<string, boolean>>({});

  const install = async (runtime: Runtime) => {
    try {
      await api.installRuntime(props.game.id, runtime.id);
      setStarted({ ...started(), [runtime.id]: true });
    } catch (e) {
      toast("Could not start the installer", { tone: "danger", detail: errorText(e) });
    }
  };

  const checkAgain = async () => {
    setChecking(true);
    const missing = await missingRuntimes(props.game.id);
    setChecking(false);
    if (missing.length === 0) {
      setDialog(null);
      toast("Everything the game needs is installed", { tone: "success" });
      void launch(props.game.id, { threats: true, runtimes: true });
    } else {
      setList(missing);
    }
  };

  return (
    <Dialog
      title="Install what this game needs"
      description={`${props.game.name} uses ${list().length === 1 ? "a runtime" : "runtimes"} this computer does not have yet. Without ${list().length === 1 ? "it" : "them"}, the game will most likely not start.`}
      onClose={() => setDialog(null)}
      size="md"
      footer={
        <>
          <Button
            variant="ghost"
            onClick={() => {
              setDialog(null);
              void launch(props.game.id, { threats: true, runtimes: true });
            }}
          >
            Play anyway
          </Button>
          <div class="dialog-footer-spacer" />
          <Button variant="primary" loading={checking()} onClick={() => void checkAgain()} data-autofocus>
            Check again
          </Button>
        </>
      }
    >
      <ul class="backup-list">
        <For each={list()}>
          {(runtime) => (
            <li class="backup">
              <div class="backup-copy">
                <span class="backup-when">{runtime.name}</span>
                <span class="backup-detail">
                  {started()[runtime.id]
                    ? "Finish the installer, then choose Check again."
                    : runtime.installer
                      ? "Installer included with the game"
                      : "Download from Microsoft"}
                </span>
              </div>
              <Button size="sm" variant={started()[runtime.id] ? "ghost" : "secondary"} onClick={() => void install(runtime)}>
                {runtime.installer ? "Install" : "Download"}
              </Button>
            </li>
          )}
        </For>
      </ul>
    </Dialog>
  );
}
