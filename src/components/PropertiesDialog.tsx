import { open } from "@tauri-apps/plugin-dialog";
import { Plus, Sparkles, Trash2, X } from "./icons";
import { createResource, createSignal, For, Match, Show, Switch as SolidSwitch } from "solid-js";
import { api, fileUrl } from "../lib/api";
import { count, timeAgo } from "../lib/format";
import { generatedArt } from "../lib/art";
import { fetchArtwork, patchGame, type PropertiesTab, setArtwork, setDialog, store } from "../lib/store";
import type { ArtworkKind, EnvVar, Game, SaveLocation } from "../lib/types";
import { FadeImage } from "./GameArt";
import { Button, IconButton } from "./ui/Button";
import { Dialog } from "./ui/Dialog";
import { Switch } from "./ui/Switch";

type Tab = PropertiesTab;

const artSlots: { kind: ArtworkKind; label: string; hint: string }[] = [
  { kind: "banner", label: "Banner", hint: "Wide hero image, 1920×620 or larger" },
  { kind: "cover", label: "Cover", hint: "Portrait, 600×900" },
  { kind: "logo", label: "Logo", hint: "Transparent PNG, shown over the banner" },
  { kind: "icon", label: "Icon", hint: "Square, shown in the sidebar" },
];

export function PropertiesDialog(props: { game: Game; tab?: Tab }) {
  const [tab, setTab] = createSignal<Tab>(props.tab ?? "general");
  const [name, setName] = createSignal(props.game.name);
  const [favorite, setFavorite] = createSignal(props.game.favorite);
  const [executable, setExecutable] = createSignal(props.game.executable);
  const [args, setArgs] = createSignal(props.game.launchArgs);
  const [workingDir, setWorkingDir] = createSignal<string | null>(props.game.workingDir);
  const [env, setEnv] = createSignal<EnvVar[]>(props.game.env.map((v) => ({ ...v })));
  const [saveDir, setSaveDir] = createSignal<string | null>(props.game.saveDir);
  const [backupSaves, setBackupSaves] = createSignal(props.game.backupSaves);
  const [suggestions] = createResource(
    () => (tab() === "saves" ? props.game.id : null),
    (id) => api.suggestSaveLocations(id).catch((): SaveLocation[] => []),
  );
  const [saving, setSaving] = createSignal(false);
  const [fetching, setFetching] = createSignal(false);

  const close = () => setDialog(null);

  const save = async (e: Event) => {
    e.preventDefault();
    setSaving(true);
    const saved = await patchGame(props.game.id, {
      name: name(),
      favorite: favorite(),
      executable: executable(),
      launchArgs: args(),
      workingDir: workingDir(),
      env: env().filter((v) => v.key.trim()),
      saveDir: saveDir(),
      backupSaves: backupSaves(),
    });
    setSaving(false);
    if (saved) close();
  };

  const pickExe = async () => {
    const picked = await open({
      title: "Choose the game's executable",
      defaultPath: props.game.installDir,
      filters: store.hostOs === "windows" ? [{ name: "Programs", extensions: ["exe"] }] : undefined,
    });
    if (typeof picked === "string") setExecutable(picked);
  };

  const pickDir = async () => {
    const picked = await open({ directory: true, title: "Choose the working folder", defaultPath: props.game.installDir });
    if (typeof picked === "string") setWorkingDir(picked);
  };

  const pickSaveDir = async () => {
    const picked = await open({ directory: true, title: "Choose the folder where the game saves" });
    if (typeof picked === "string") setSaveDir(picked);
  };

  const pickArt = async (kind: ArtworkKind) => {
    const picked = await open({
      title: "Choose an image",
      filters: [{ name: "Images", extensions: ["png", "jpg", "jpeg", "webp", "gif", "avif"] }],
    });
    if (typeof picked === "string") await setArtwork(props.game.id, kind, picked);
  };

  const findArt = async () => {
    setFetching(true);
    await fetchArtwork(props.game.id, true);
    setFetching(false);
  };

  // Artwork changes apply at once, so read them from the store rather than the snapshot.
  const live = () => store.games.find((g) => g.id === props.game.id) ?? props.game;

  return (
    <Dialog
      title="Properties"
      description={props.game.name}
      onClose={close}
      busy={saving()}
      size="lg"
      footer={
        <>
          <Button variant="ghost" icon={<Trash2 />} class="btn-danger-ghost" onClick={() => setDialog({ type: "remove", id: props.game.id })}>
            Remove from library
          </Button>
          <div class="dialog-footer-spacer" />
          <Button variant="secondary" onClick={close}>
            Cancel
          </Button>
          <Button variant="primary" type="submit" form="properties-form" loading={saving()} disabled={!name().trim()}>
            Save
          </Button>
        </>
      }
    >
      <div class="tabs" role="tablist">
        <For each={["general", "launch", "saves", "artwork"] as Tab[]}>
          {(t) => (
            <button
              type="button"
              role="tab"
              class="tab"
              aria-selected={tab() === t}
              data-active={tab() === t || undefined}
              onClick={() => setTab(t)}
            >
              {{ general: "General", launch: "Launch", saves: "Saves", artwork: "Artwork" }[t]}
            </button>
          )}
        </For>
      </div>

      <form id="properties-form" class="form" onSubmit={save}>
        <SolidSwitch>
          <Match when={tab() === "general"}>
            <label class="field">
              <span class="field-label">Name</span>
              <input class="input" value={name()} onInput={(e) => setName(e.currentTarget.value)} data-autofocus />
            </label>
            <div class="setting-card">
              <Switch
                label="Favorite"
                description="Favorites stay at the top of your library."
                checked={favorite()}
                onChange={setFavorite}
              />
            </div>
          </Match>

          <Match when={tab() === "launch"}>
            <div class="field">
              <span class="field-label">Executable</span>
              <div class="input-row">
                <input class="input mono" value={executable()} onInput={(e) => setExecutable(e.currentTarget.value)} spellcheck={false} />
                <Button onClick={() => void pickExe()}>Browse…</Button>
              </div>
            </div>
            <label class="field">
              <span class="field-label">Launch options</span>
              <input
                class="input mono"
                value={args()}
                placeholder="e.g. -windowed -dx12"
                spellcheck={false}
                onInput={(e) => setArgs(e.currentTarget.value)}
              />
              <span class="field-description">Separate options with spaces. Use quotes for a value with spaces.</span>
            </label>
            <div class="field">
              <span class="field-label">Working folder</span>
              <div class="input-row">
                <input
                  class="input mono"
                  value={workingDir() ?? ""}
                  placeholder="The executable's folder"
                  readOnly
                />
                <Button onClick={() => void pickDir()}>Browse…</Button>
                <Show when={workingDir()}>
                  <IconButton label="Use the executable's folder" variant="secondary" onClick={() => setWorkingDir(null)}>
                    <X />
                  </IconButton>
                </Show>
              </div>
            </div>
            <div class="field">
              <div class="field-label-row">
                <span class="field-label">Environment variables</span>
                <Button variant="ghost" size="sm" icon={<Plus />} onClick={() => setEnv([...env(), { key: "", value: "" }])}>
                  Add
                </Button>
              </div>
              <Show when={env().length > 0} fallback={<span class="field-description">None. Some games and mods read settings from these.</span>}>
                <div class="env-list">
                  <For each={env()}>
                    {(v, i) => (
                      <div class="input-row">
                        <input
                          class="input mono env-key"
                          placeholder="NAME"
                          value={v.key}
                          spellcheck={false}
                          onInput={(e) => setEnv(env().map((x, j) => (j === i() ? { ...x, key: e.currentTarget.value } : x)))}
                        />
                        <input
                          class="input mono"
                          placeholder="value"
                          value={v.value}
                          spellcheck={false}
                          onInput={(e) => setEnv(env().map((x, j) => (j === i() ? { ...x, value: e.currentTarget.value } : x)))}
                        />
                        <IconButton label="Remove variable" variant="secondary" onClick={() => setEnv(env().filter((_, j) => j !== i()))}>
                          <X />
                        </IconButton>
                      </div>
                    )}
                  </For>
                </div>
              </Show>
            </div>
          </Match>

          <Match when={tab() === "saves"}>
            <div class="field">
              <span class="field-label">Save folder</span>
              <div class="input-row">
                <input class="input mono" value={saveDir() ?? ""} placeholder="Not set" readOnly />
                <Button onClick={() => void pickSaveDir()}>Browse…</Button>
                <Show when={saveDir()}>
                  <IconButton label="Forget the save folder" variant="secondary" onClick={() => setSaveDir(null)}>
                    <X />
                  </IconButton>
                </Show>
              </div>
              <span class="field-description">
                Where the game writes its save files. Starex backs this folder up so a crash or a bad save never costs
                you progress.
              </span>
            </div>

            <div class="field">
              <span class="field-label">Found on this computer</span>
              <Show
                when={!suggestions.loading}
                fallback={<span class="field-description">Looking for save folders…</span>}
              >
                <Show
                  when={(suggestions() ?? []).length > 0}
                  fallback={
                    <span class="field-description">
                      No likely save folders yet. Play once and save, then look again, or browse to the folder yourself.
                    </span>
                  }
                >
                  <div class="candidates" role="radiogroup" aria-label="Suggested save folders">
                    <For each={suggestions()}>
                      {(location) => (
                        <button
                          type="button"
                          role="radio"
                          class="candidate"
                          aria-checked={saveDir() === location.path}
                          data-active={saveDir() === location.path || undefined}
                          onClick={() => setSaveDir(location.path)}
                        >
                          <span class="candidate-radio" aria-hidden="true" />
                          <span class="candidate-path" title={location.path}>
                            {location.path}
                          </span>
                          <span class="candidate-size">
                            {count(location.files, "file")}
                            <Show when={location.modified}>{(at) => <> · {timeAgo(at())}</>}</Show>
                          </span>
                        </button>
                      )}
                    </For>
                  </div>
                </Show>
              </Show>
            </div>

            <div class="setting-card">
              <Switch
                label="Back up saves automatically"
                description="Before the first launch and after every session. Starex keeps the latest 20 backups."
                checked={backupSaves()}
                onChange={setBackupSaves}
                disabled={!saveDir()}
              />
            </div>
          </Match>

          <Match when={tab() === "artwork"}>
            <div class="art-grid">
              <For each={artSlots}>
                {(slot) => (
                  <div class="art-slot" data-kind={slot.kind}>
                    <div class="art-preview" style={{ background: generatedArt(live().name, 0.8) }}>
                      <FadeImage
                        src={fileUrl(live().artwork[slot.kind])}
                        fallback={<span class="art-empty">No {slot.label.toLowerCase()}</span>}
                      />
                    </div>
                    <div class="art-slot-copy">
                      <span class="art-slot-label">{slot.label}</span>
                      <span class="art-slot-hint">{slot.hint}</span>
                    </div>
                    <div class="art-slot-actions">
                      <Button size="sm" onClick={() => void pickArt(slot.kind)}>
                        Choose…
                      </Button>
                      <Show when={live().artwork[slot.kind]}>
                        <Button size="sm" variant="ghost" onClick={() => void setArtwork(props.game.id, slot.kind, null)}>
                          Remove
                        </Button>
                      </Show>
                    </div>
                  </div>
                )}
              </For>
            </div>
            <div class="art-online">
              <div>
                <span class="art-slot-label">SteamGridDB</span>
                <span class="art-slot-hint">
                  <Show when={store.settings.steamgriddbKey} fallback="Add an API key in Settings > Artwork to find artwork online.">
                    Replaces all four images with the best match for "{live().name}".
                  </Show>
                </span>
              </div>
              <Button icon={<Sparkles />} loading={fetching()} disabled={!store.settings.steamgriddbKey} onClick={() => void findArt()}>
                Find artwork
              </Button>
            </div>
          </Match>
        </SolidSwitch>
      </form>
    </Dialog>
  );
}
