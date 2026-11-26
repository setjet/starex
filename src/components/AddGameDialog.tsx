import { open } from "@tauri-apps/plugin-dialog";
import { CircleX, FolderInput, Info, ShieldAlert, ShieldCheck, TriangleAlert } from "./icons";
import { createSignal, For, Match, onCleanup, onMount, Show, Switch } from "solid-js";
import { api, errorText } from "../lib/api";
import { archLabel, bytes, count } from "../lib/format";
import { addGame, cancelTask, setDialog, store } from "../lib/store";
import { toast } from "../lib/toast";
import type { ExecutableCandidate, FolderInspection, ScanReport } from "../lib/types";
import { Button } from "./ui/Button";
import { Dialog } from "./ui/Dialog";
import { Progress, Spinner } from "./ui/Spinner";

const packageLabels: Record<string, string> = {
  microsoftGdk: "Microsoft GDK",
  msix: "MSIX package",
  xboxContainer: "Xbox container",
  steam: "Steam",
};

/** Choose a folder, let Starex inspect it, review and confirm. */
export function AddGameDialog(props: { initialPath?: string }) {
  const [phase, setPhase] = createSignal<"choose" | "scanning" | "review">("choose");
  const [inspection, setInspection] = createSignal<FolderInspection | null>(null);
  const [name, setName] = createSignal("");
  const [exe, setExe] = createSignal<string | null>(null);
  const [args, setArgs] = createSignal("");
  const [useArt, setUseArt] = createSignal(true);
  const [error, setError] = createSignal("");
  const [saving, setSaving] = createSignal(false);
  // The virus scan runs before the game is added, as soon as the folder has been read.
  const [virus, setVirus] = createSignal<
    { state: "off" } | { state: "running"; taskId: string } | { state: "done"; report: ScanReport } | { state: "skipped" }
  >({ state: "off" });

  const stopScan = () => {
    const v = virus();
    if (v.state === "running") void cancelTask(v.taskId);
  };
  onCleanup(stopScan);

  const close = () => setDialog(null);

  const scanForViruses = async (folder: string) => {
    const taskId = `add-${Date.now()}`;
    setVirus({ state: "running", taskId });
    try {
      const report = await api.scanFolder(folder, taskId);
      const current = virus();
      if (current.state !== "running" || current.taskId !== taskId) return;
      setVirus(report.status === "cancelled" ? { state: "skipped" } : { state: "done", report });
    } catch (e) {
      setVirus({
        state: "done",
        report: {
          gameId: taskId,
          status: "unavailable",
          engine: "",
          filesScanned: 0,
          bytesScanned: 0,
          skipped: 0,
          threats: [],
          message: errorText(e),
          durationMs: 0,
          at: Math.floor(Date.now() / 1000),
        },
      });
    }
  };

  const scan = async (path: string) => {
    let folder = path;
    let preferred: string | null = null;
    if (/\.(exe|bat|cmd)$/i.test(path)) {
      preferred = path;
      folder = path.replace(/[\\/][^\\/]+$/, "");
    }
    setPhase("scanning");
    setError("");
    try {
      const result = await api.inspectFolder(folder);
      setInspection(result);
      setName(result.suggestedName);
      const first = result.candidates.find((c) => c.runnable);
      const match = preferred && result.candidates.find((c) => c.path.toLowerCase() === preferred?.toLowerCase());
      setExe(match ? match.path : (first?.path ?? preferred));
      setPhase("review");
      if (store.settings.scanNewGames) void scanForViruses(result.root);
    } catch (e) {
      setError(errorText(e));
      setPhase("choose");
    }
  };

  const choose = async () => {
    const picked = await open({ directory: true, title: "Choose the game's folder" });
    if (typeof picked === "string") await scan(picked);
  };

  const browseExe = async () => {
    const root = inspection()?.root;
    const picked = await open({
      title: "Choose the game's executable",
      defaultPath: root,
      filters: store.hostOs === "windows" ? [{ name: "Programs", extensions: ["exe", "bat", "cmd"] }] : undefined,
    });
    if (typeof picked === "string") setExe(picked);
  };

  onMount(() => {
    if (props.initialPath) void scan(props.initialPath);
  });

  const selected = (): ExecutableCandidate | undefined => inspection()?.candidates.find((c) => c.path === exe());
  const hasArt = () => {
    const a = inspection()?.artwork;
    return !!a && !!(a.banner || a.cover || a.logo || a.icon);
  };
  const scanning = () => virus().state === "running";
  const report = () => {
    const v = virus();
    return v.state === "done" ? v.report : null;
  };
  const threats = () => report()?.status === "threats";
  const canAdd = () => !!exe() && name().trim().length > 0 && (selected()?.runnable ?? true) && !scanning();
  const scanProgress = () => {
    const v = virus();
    const p = v.state === "running" ? store.tasks[v.taskId]?.progress : undefined;
    return p && p.totalFiles > 0 ? p : null;
  };

  const submit = async (e: Event) => {
    e.preventDefault();
    const info = inspection();
    const executable = exe();
    if (!info || !executable || !canAdd()) return;
    setSaving(true);
    try {
      const r = report();
      const game = await addGame(
        { name: name().trim(), installDir: info.root, executable, launchArgs: args() },
        useArt() ? info.artwork : null,
        r ? { at: r.at, status: r.status, threats: r.threats.length, filesScanned: r.filesScanned } : null,
      );
      toast(`${game.name} added`, { tone: "success", detail: "Run a quick check, or record a baseline while it works." });
      close();
    } catch (err) {
      setError(errorText(err));
    } finally {
      setSaving(false);
    }
  };

  return (
    <Dialog
      title="Add a game"
      description={phase() === "review" ? inspection()?.root : "Point Starex at the folder that holds the game's files."}
      onClose={close}
      busy={saving()}
      size="lg"
      footer={
        <Show when={phase() === "review"}>
          <Button
            variant="ghost"
            onClick={() => {
              stopScan();
              setVirus({ state: "off" });
              setPhase("choose");
            }}
            disabled={saving()}
          >
            Choose another folder
          </Button>
          <div class="dialog-footer-spacer" />
          <Button variant="secondary" onClick={close} disabled={saving()}>
            Cancel
          </Button>
          <Button
            variant={threats() ? "destructive" : "primary"}
            type="submit"
            form="add-game-form"
            disabled={!canAdd()}
            loading={saving()}
          >
            {threats() ? "Add anyway" : "Add to library"}
          </Button>
        </Show>
      }
    >
      <Switch>
        <Match when={phase() === "choose"}>
          <button type="button" class="dropzone" onClick={() => void choose()} data-autofocus>
            <FolderInput class="dropzone-icon" />
            <span class="dropzone-title">Choose a folder</span>
            <span class="dropzone-text">or drop it on the Starex window</span>
          </button>
          <Show when={error()}>
            <p class="callout" data-level="error">
              <CircleX />
              <span>{error()}</span>
            </p>
          </Show>
          <SafetyNotice />
        </Match>

        <Match when={phase() === "scanning"}>
          <div class="scanning" role="status">
            <Spinner size={22} />
            <span>Scanning the folder…</span>
          </div>
        </Match>

        <Match when={phase() === "review" && inspection()}>
          {(info) => (
            <form id="add-game-form" class="review" onSubmit={submit}>
              <div class="review-stats">
                <span>{count(info().fileCount, "file")}</span>
                <span>{bytes(info().totalBytes)}</span>
                <Show when={info().package}>{(p) => <span>{packageLabels[p()] ?? p()}</span>}</Show>
                <Show when={info().hasManifest}>
                  <span data-tone="pass">Has a file manifest</span>
                </Show>
              </div>

              <Switch>
                <Match when={virus().state === "running"}>
                  <div class="virus-check" role="status">
                    <div class="virus-check-row">
                      <Spinner size={16} />
                      <span class="virus-check-title">Scanning for viruses</span>
                      <Show when={scanProgress()}>
                        {(p) => (
                          <span class="virus-check-meta">
                            {p().doneFiles.toLocaleString()} of {count(p().totalFiles, "file")}
                          </span>
                        )}
                      </Show>
                      <button type="button" class="virus-check-cancel" onClick={stopScan}>
                        Skip
                      </button>
                    </div>
                    <Progress
                      value={scanProgress() ? (scanProgress()?.doneFiles ?? 0) / (scanProgress()?.totalFiles ?? 1) : null}
                    />
                  </div>
                </Match>
                <Match when={report()?.status === "clean" && report()}>
                  {(r) => (
                    <p class="virus-check virus-check-row" data-tone="clean">
                      <ShieldCheck />
                      <span class="virus-check-title">No threats found</span>
                      <span class="virus-check-meta">{count(r().filesScanned, "program file")} scanned</span>
                    </p>
                  )}
                </Match>
                <Match when={threats() && report()}>
                  {(r) => (
                    <div class="callout" data-level="error">
                      <ShieldAlert />
                      <div class="callout-body">
                        <span>
                          Your antivirus flagged {count(r().threats.length, "file")}. Adding this game is not
                          recommended.
                        </span>
                        <ul class="issue-list">
                          <For each={r().threats}>{(t) => <li class="issue-path">{t.path}</li>}</For>
                        </ul>
                      </div>
                    </div>
                  )}
                </Match>
                <Match when={report()?.status === "unavailable" && report()}>
                  {(r) => (
                    <p class="callout" data-level="warn">
                      <TriangleAlert />
                      <span>{r().message ?? "The virus scan could not run."}</span>
                    </p>
                  )}
                </Match>
                <Match when={virus().state === "skipped"}>
                  <p class="virus-check virus-check-row">
                    <ShieldAlert />
                    <span class="virus-check-title">Virus scan skipped</span>
                    <button type="button" class="virus-check-cancel" onClick={() => void scanForViruses(info().root)}>
                      Scan
                    </button>
                  </p>
                </Match>
              </Switch>

              <For each={info().notes}>
                {(note) => (
                  <p class="callout" data-level={note.level}>
                    <Switch>
                      <Match when={note.level === "error"}>
                        <CircleX />
                      </Match>
                      <Match when={note.level === "warn"}>
                        <TriangleAlert />
                      </Match>
                      <Match when={note.level === "info"}>
                        <Info />
                      </Match>
                    </Switch>
                    <span>{note.text}</span>
                  </p>
                )}
              </For>

              <label class="field">
                <span class="field-label">Name</span>
                <input class="input" value={name()} onInput={(e) => setName(e.currentTarget.value)} data-autofocus />
              </label>

              <div class="field">
                <div class="field-label-row">
                  <span class="field-label">Executable</span>
                  <Button variant="ghost" size="sm" onClick={() => void browseExe()}>
                    Browse…
                  </Button>
                </div>
                <Show
                  when={info().candidates.length > 0}
                  fallback={<p class="field-description">No executable found. Browse for one.</p>}
                >
                  <div class="candidates" role="radiogroup" aria-label="Executable">
                    <For each={info().candidates}>
                      {(c) => (
                        <button
                          type="button"
                          role="radio"
                          class="candidate"
                          aria-checked={exe() === c.path}
                          data-active={exe() === c.path || undefined}
                          data-invalid={!c.runnable || undefined}
                          onClick={() => setExe(c.path)}
                        >
                          <span class="candidate-radio" aria-hidden="true" />
                          <span class="candidate-path">{c.relative}</span>
                          <span class="candidate-meta">
                            <Show
                              when={c.runnable}
                              fallback={<span class="badge" data-tone="fail">{c.problem === "notExecutable" ? "Not a Windows program" : "Can't run here"}</span>}
                            >
                              <Show when={c.host === "emulated"}>
                                <span class="badge" data-tone="info">{store.hostOs === "macos" ? "Rosetta" : "Emulated"}</span>
                              </Show>
                              <Show when={c.host === "compat"}>
                                <span class="badge" data-tone="info">Wine</span>
                              </Show>
                              <span class="badge">
                                {c.format === "batch" || c.format === "script" ? "Launch script" : archLabel[c.arch ?? "unknown"]}
                              </span>
                            </Show>
                            <span class="candidate-size">{bytes(c.size)}</span>
                          </span>
                        </button>
                      )}
                    </For>
                  </div>
                </Show>
                <Show when={exe() && !selected()}>
                  <p class="field-description mono">{exe()}</p>
                </Show>
              </div>

              <label class="field">
                <span class="field-label">Launch options</span>
                <input
                  class="input mono"
                  placeholder="Optional, e.g. -windowed -dx12"
                  value={args()}
                  spellcheck={false}
                  onInput={(e) => setArgs(e.currentTarget.value)}
                />
              </label>

              <Show when={hasArt()}>
                <label class="art-found">
                  <input type="checkbox" checked={useArt()} onChange={(e) => setUseArt(e.currentTarget.checked)} />
                  <span>Use the artwork found in the folder</span>
                  <span class="art-found-kinds">
                    <For each={(["banner", "cover", "logo", "icon"] as const).filter((k) => info().artwork[k])}>
                      {(kind) => <span class="badge">{kind[0]?.toUpperCase() + kind.slice(1)}</span>}
                    </For>
                  </span>
                </label>
              </Show>

              <SafetyNotice />
            </form>
          )}
        </Match>
      </Switch>
    </Dialog>
  );
}

/** Always shown while adding a game: a virus check lowers the risk but cannot remove it. */
function SafetyNotice() {
  return (
    <p class="safety-notice">
      <ShieldAlert />
      <span>
        Only add game files from sources you trust. Starex checks programs and scripts with your installed antivirus,
        which cannot detect every threat.
      </span>
    </p>
  );
}
