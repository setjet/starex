import { ArrowUpRight, Blocks, Bug, Eye, EyeOff, FolderOpen, Gamepad2, Heart, Info, Palette, RefreshCw, ScrollText, Settings2, X } from "./icons";
import { createSignal, For, type JSX, Match, Show, Switch as SolidSwitch } from "solid-js";
import { open } from "@tauri-apps/plugin-dialog";
import { api, fileUrl } from "../lib/api";
import { timeAgo } from "../lib/format";
import { type SettingsSection, saveSettings, setBackdropImage, setDialog, store } from "../lib/store";
import { ACCENT_ORDER, accentColor } from "../lib/theme";
import type { Accent } from "../lib/types";
import { checkForUpdates, updater } from "../lib/updater";
import { StarMark } from "./StarMark";
import { Button, IconButton } from "./ui/Button";
import { Dialog } from "./ui/Dialog";
import { Segmented, Switch } from "./ui/Switch";

const sections: { id: SettingsSection; label: string; icon: () => JSX.Element }[] = [
  { id: "general", label: "General", icon: () => <Settings2 /> },
  { id: "appearance", label: "Appearance", icon: () => <Palette /> },
  { id: "games", label: "Games", icon: () => <Gamepad2 /> },
  { id: "integrations", label: "Integrations", icon: () => <Blocks /> },
  { id: "updates", label: "Updates", icon: () => <RefreshCw /> },
  { id: "about", label: "About", icon: () => <Info /> },
];

export function SettingsDialog(props: { section?: SettingsSection }) {
  const [section, setSection] = createSignal<SettingsSection>(props.section ?? "general");
  const s = () => store.settings;

  return (
    <Dialog title="Settings" onClose={() => setDialog(null)} size="xl" bare class="settings">
      <IconButton label="Close" size="sm" class="settings-close" onClick={() => setDialog(null)}>
        <X />
      </IconButton>
      <nav class="settings-nav" aria-label="Settings sections">
        <h2 class="settings-nav-title">Settings</h2>
        <For each={sections}>
          {(item) => (
            <button
              type="button"
              class="settings-nav-item"
              data-active={section() === item.id || undefined}
              aria-current={section() === item.id ? "page" : undefined}
              onClick={() => setSection(item.id)}
            >
              {item.icon()}
              <span>{item.label}</span>
              <Show when={item.id === "updates" && updater.available()}>
                <span class="settings-nav-dot" />
              </Show>
            </button>
          )}
        </For>
      </nav>

      <div class="settings-panel">
        <div class="settings-panel-scroll">
          <SolidSwitch>
            <Match when={section() === "general"}>
              <SettingsHeader title="General" />
              <Group title="When a game starts">
                <Row label="Starex window" description="What the launcher does while you play.">
                  <Segmented
                    label="Starex window"
                    value={s().launchBehavior}
                    onChange={(v) => void saveSettings({ launchBehavior: v })}
                    options={[
                      { value: "keepOpen", label: "Stay open" },
                      { value: "minimize", label: "Minimize" },
                      { value: "hide", label: "Hide" },
                    ]}
                  />
                </Row>
                <Switch
                  label="Come back when the game closes"
                  description="Restore the Starex window after a session ends."
                  checked={s().restoreOnExit}
                  disabled={s().launchBehavior === "keepOpen"}
                  onChange={(v) => void saveSettings({ restoreOnExit: v })}
                />
              </Group>
              <SystemSettings />
            </Match>

            <Match when={section() === "appearance"}>
              <SettingsHeader title="Appearance" />
              <AppearanceSettings />
            </Match>

            <Match when={section() === "games"}>
              <SettingsHeader title="Games" />
              <Group title="Safety">
                <Switch
                  label="Scan new games for viruses"
                  description="Your antivirus checks a game's programs and scripts before it is added. Takes seconds."
                  checked={s().scanNewGames}
                  onChange={(v) => void saveSettings({ scanNewGames: v })}
                />
                <Switch
                  label="Check files before launching"
                  description="Runs a quick check first and stops if required files are missing."
                  checked={s().verifyBeforeLaunch}
                  onChange={(v) => void saveSettings({ verifyBeforeLaunch: v })}
                />
              </Group>
              <SaveSettings />
              <Group title="Library">
                <Row label="Sort games by" description="Favorites always come first.">
                  <Segmented
                    label="Sort games by"
                    value={s().librarySort}
                    onChange={(v) => void saveSettings({ librarySort: v })}
                    options={[
                      { value: "recent", label: "Recent" },
                      { value: "name", label: "Name" },
                      { value: "playtime", label: "Play time" },
                    ]}
                  />
                </Row>
              </Group>
              <Show when={store.hostOs && store.hostOs !== "windows"}>
                <WineSetting />
              </Show>
              <Group title="Storage">
                <Row label="Data folder" description={store.dataDir}>
                  <Button icon={<FolderOpen />} onClick={() => void api.openDataDir()}>
                    Open
                  </Button>
                </Row>
              </Group>
            </Match>

            <Match when={section() === "integrations"}>
              <SettingsHeader title="Integrations" />
              <DiscordSettings />
              <ArtworkSettings />
            </Match>

            <Match when={section() === "updates"}>
              <SettingsHeader title="Updates" />
              <UpdateSettings />
            </Match>

            <Match when={section() === "about"}>
              <About />
            </Match>
          </SolidSwitch>
        </div>
      </div>
    </Dialog>
  );
}

/** Backdrop, accent colour and motion. */
function AppearanceSettings() {
  const s = () => store.settings;
  const chooseImage = async () => {
    const picked = await open({
      title: "Choose a backdrop image",
      filters: [{ name: "Images", extensions: ["png", "jpg", "jpeg", "webp", "gif", "avif"] }],
    });
    if (typeof picked === "string") await setBackdropImage(picked);
  };
  const accentNames: Record<Accent, string> = {
    blue: "Blue",
    violet: "Violet",
    pink: "Pink",
    green: "Green",
    orange: "Orange",
    white: "White",
  };

  return (
    <>
      <Group title="Backdrop">
        <Row label="Behind your library" description="Fills the window behind the library and game pages.">
          <Segmented
            label="Backdrop"
            value={s().backdrop}
            onChange={(v) => {
              if (v === "custom" && !s().backdropImage) void chooseImage();
              else void saveSettings({ backdrop: v });
            }}
            options={[
              { value: "none", label: "None" },
              { value: "aurora", label: "Aurora" },
              { value: "game", label: "Game art" },
              { value: "custom", label: "Image" },
            ]}
          />
        </Row>
        <Show when={s().backdrop === "custom"}>
          <Row label="Image" description={s().backdropImage ? "Shown behind Starex, softly dimmed." : "No image chosen yet."}>
            <div class="input-row">
              <Show when={fileUrl(s().backdropImage)}>{(src) => <img class="backdrop-thumb" src={src()} alt="" />}</Show>
              <Button onClick={() => void chooseImage()}>{s().backdropImage ? "Change…" : "Choose…"}</Button>
            </div>
          </Row>
        </Show>
        <Row label="Strength" description="How strongly the backdrop shows through.">
          <input
            class="slider"
            type="range"
            min="15"
            max="100"
            step="5"
            value={s().backdropStrength}
            disabled={s().backdrop === "none"}
            aria-label="Backdrop strength"
            onChange={(e) => void saveSettings({ backdropStrength: Number(e.currentTarget.value) })}
          />
        </Row>
      </Group>

      <Group title="Accent color">
        <Row label="Play and highlights" description={accentNames[s().accent]}>
          <div class="swatches" role="radiogroup" aria-label="Accent color">
            <For each={ACCENT_ORDER}>
              {(accent) => (
                <button
                  type="button"
                  role="radio"
                  class="swatch"
                  style={{ "--swatch": accentColor(accent) }}
                  aria-label={accentNames[accent]}
                  aria-checked={s().accent === accent}
                  data-active={s().accent === accent || undefined}
                  onClick={() => void saveSettings({ accent })}
                />
              )}
            </For>
          </div>
        </Row>
      </Group>

      <Group title="Motion">
        <Switch
          label="Launch animation"
          description="The star animation when Starex opens."
          checked={s().launchAnimation}
          onChange={(v) => void saveSettings({ launchAnimation: v })}
        />
        <Switch
          label="Reduce motion"
          description="Turns off transitions and animated effects."
          checked={s().reduceMotion}
          onChange={(v) => void saveSettings({ reduceMotion: v })}
        />
      </Group>
    </>
  );
}

function SettingsHeader(props: { title: string }) {
  return <h2 class="settings-title">{props.title}</h2>;
}

function Group(props: { title: string; children: JSX.Element }) {
  return (
    <section class="settings-group">
      <h3 class="settings-group-title">{props.title}</h3>
      <div class="setting-card">{props.children}</div>
    </section>
  );
}

function Row(props: { label: string; description?: JSX.Element; children: JSX.Element }) {
  return (
    <div class="setting-row">
      <div class="switch-copy">
        <span class="switch-label">{props.label}</span>
        <Show when={props.description}>
          <span class="switch-description">{props.description}</span>
        </Show>
      </div>
      {props.children}
    </div>
  );
}

function SaveSettings() {
  const chooseCloud = async () => {
    const picked = await open({ directory: true, title: "Choose a synced folder for save backups" });
    if (typeof picked === "string") void saveSettings({ cloudBackupDir: picked });
  };
  return (
    <Group title="Saves">
      <Switch
        label="Find save folders automatically"
        description="Uses the free Ludusavi database of save locations for 10,000+ games. Starex downloads it from GitHub and refreshes it weekly."
        checked={store.settings.saveDatabase}
        onChange={(v) => void saveSettings({ saveDatabase: v })}
      />
      <Row
        label="Cloud backup folder"
        description={
          store.settings.cloudBackupDir ||
          "Choose a folder that OneDrive, Dropbox, Google Drive or Syncthing syncs. Starex copies every save backup there."
        }
      >
        <div class="input-row">
          <Show when={store.settings.cloudBackupDir}>
            <Button variant="ghost" onClick={() => void saveSettings({ cloudBackupDir: "" })}>
              Stop
            </Button>
          </Show>
          <Button icon={<FolderOpen />} onClick={() => void chooseCloud()}>
            {store.settings.cloudBackupDir ? "Change" : "Choose"}
          </Button>
        </div>
      </Row>
    </Group>
  );
}

/** Linux and macOS: where Wine is, for running Windows games. */
function WineSetting() {
  const [path, setPath] = createSignal(store.settings.winePath);
  const dirty = () => path().trim() !== store.settings.winePath;
  return (
    <Group title="Windows games">
      <div class="setting-stack">
        <div class="switch-copy">
          <span class="switch-label">Wine</span>
          <span class="switch-description">
            Starex runs Windows games through Wine. Leave this empty to use the Wine it finds, or enter the full path to
            a specific one.
          </span>
        </div>
        <div class="input-row">
          <input
            class="input mono"
            placeholder="Find automatically"
            value={path()}
            spellcheck={false}
            onInput={(e) => setPath(e.currentTarget.value)}
          />
          <Button variant="primary" disabled={!dirty()} onClick={() => void saveSettings({ winePath: path().trim() })}>
            Save
          </Button>
        </div>
      </div>
    </Group>
  );
}

function DiscordSettings() {
  const [appId, setAppId] = createSignal(store.settings.discordAppId);
  return (
    <Group title="Discord">
      <Switch
        label="Show what you're playing"
        description="Your Discord profile shows the game while you play, when Discord is open. You can turn it off per game in its Properties."
        checked={store.settings.discordPresence}
        onChange={(v) => void saveSettings({ discordPresence: v })}
      />
      <div class="setting-stack">
        <div class="switch-copy">
          <span class="switch-label">Application ID</span>
          <span class="switch-description">
            <Show
              when={store.discordBuiltin}
              fallback="Discord needs an application ID. Create a free application named Starex in Discord's developer portal and paste its ID here."
            >
              Starex uses its own application. Paste another ID only if you want your status to show a different name.
            </Show>
          </span>
        </div>
        <div class="input-row">
          <input
            class="input mono"
            placeholder={store.discordBuiltin ? "Built in" : "e.g. 1234567890123456789"}
            value={appId()}
            spellcheck={false}
            onInput={(e) => setAppId(e.currentTarget.value.replace(/[^0-9]/g, ""))}
          />
          <Button
            variant="primary"
            disabled={appId().trim() === store.settings.discordAppId}
            onClick={() => void saveSettings({ discordAppId: appId().trim() })}
          >
            Save
          </Button>
        </div>
        <button type="button" class="text-link" onClick={() => void api.openLink("https://discord.com/developers/applications")}>
          Open the Discord developer portal <ArrowUpRight />
        </button>
      </div>
    </Group>
  );
}

function SystemSettings() {
  const startLabel = () =>
    store.hostOs === "macos" ? "Open at login" : store.hostOs === "linux" ? "Start when you sign in" : "Start with Windows";
  return (
    <Group title="Starting Starex">
      <Switch
        label="Keep Starex in the tray"
        description="Closing the window leaves Starex running in the system tray, ready to start a game."
        checked={store.settings.closeToTray}
        onChange={(v) => void saveSettings({ closeToTray: v })}
      />
      <Switch
        label={startLabel()}
        description="Starex opens quietly in the tray when you sign in."
        checked={store.settings.startWithSystem}
        onChange={(v) => void saveSettings({ startWithSystem: v })}
      />
      <Row label="Shortcut to open Starex" description="Works from anywhere, even when Starex is in the tray.">
        <ShortcutRecorder value={store.settings.globalShortcut} onChange={(v) => void saveSettings({ globalShortcut: v })} />
      </Row>
    </Group>
  );
}

/** Records a key combination: click, then press the keys. Backspace turns it off. */
function ShortcutRecorder(props: { value: string; onChange: (value: string) => void }) {
  const [recording, setRecording] = createSignal(false);
  const keyName = (e: KeyboardEvent): string | null => {
    if (/^Key[A-Z]$/.test(e.code)) return e.code.slice(3);
    if (/^Digit[0-9]$/.test(e.code)) return e.code.slice(5);
    if (/^F([1-9]|1[0-2])$/.test(e.code)) return e.code;
    if (e.code === "Space") return "Space";
    return null;
  };
  const onKey = (e: KeyboardEvent) => {
    if (!recording()) return;
    e.preventDefault();
    e.stopPropagation();
    if (e.key === "Escape") return setRecording(false);
    if (e.key === "Backspace" || e.key === "Delete") {
      setRecording(false);
      return props.onChange("");
    }
    const key = keyName(e);
    const mods = [e.ctrlKey && "Ctrl", e.altKey && "Alt", e.shiftKey && "Shift", e.metaKey && "Super"].filter(Boolean);
    if (!key || mods.length === 0) return;
    setRecording(false);
    props.onChange([...mods, key].join("+"));
  };
  return (
    <button
      type="button"
      class="shortcut-recorder"
      data-recording={recording() || undefined}
      onClick={() => setRecording((v) => !v)}
      onKeyDown={onKey}
      onBlur={() => setRecording(false)}
      aria-label={recording() ? "Press the new key combination" : `Shortcut: ${props.value || "off"}. Click to change.`}
    >
      <Show when={!recording()} fallback={<span class="shortcut-hint">Press keys…</span>}>
        <Show when={props.value} fallback={<span class="shortcut-hint">Off</span>}>
          <For each={props.value.split("+")}>{(part) => <kbd>{part}</kbd>}</For>
        </Show>
      </Show>
    </button>
  );
}

function ArtworkSettings() {
  const [key, setKey] = createSignal(store.settings.steamgriddbKey);
  const [reveal, setReveal] = createSignal(false);
  const dirty = () => key().trim() !== store.settings.steamgriddbKey;

  return (
    <>
      <IgdbSettings />
      <Group title="Artwork · SteamGridDB">
        <div class="setting-stack">
          <div class="switch-copy">
            <span class="switch-label">API key</span>
            <span class="switch-description">
              Starex sends a game's name to SteamGridDB only when it looks for that game's artwork. The key stays in your
              data folder.
            </span>
          </div>
          <div class="input-row">
            <input
              class="input mono"
              type={reveal() ? "text" : "password"}
              placeholder="Paste your key"
              value={key()}
              spellcheck={false}
              autocomplete="off"
              onInput={(e) => setKey(e.currentTarget.value)}
            />
            <Button variant="secondary" onClick={() => setReveal((v) => !v)} icon={reveal() ? <EyeOff /> : <Eye />} aria-label={reveal() ? "Hide key" : "Show key"} />
            <Button variant="primary" disabled={!dirty()} onClick={() => void saveSettings({ steamgriddbKey: key().trim() })}>
              Save
            </Button>
          </div>
          <button type="button" class="text-link" onClick={() => void api.openLink("https://www.steamgriddb.com/profile/preferences/api")}>
            Get a free key at steamgriddb.com <ArrowUpRight />
          </button>
        </div>
        <Switch
          label="Find artwork for new games"
          description="Fills in banner, cover, logo and icon when you add a game."
          checked={store.settings.autoArtwork}
          disabled={!store.settings.steamgriddbKey}
          onChange={(v) => void saveSettings({ autoArtwork: v })}
        />
      </Group>
    </>
  );
}

function UpdateSettings() {
  const status = () => updater.status();
  const line = () => {
    switch (status()) {
      case "checking":
        return "Checking for updates…";
      case "upToDate":
        return "Starex is up to date.";
      case "available":
        return `Starex ${updater.available()?.version} is available.`;
      case "downloading":
        return `Downloading… ${Math.round(updater.progress() * 100)}%`;
      case "installing":
        return "Installing…";
      case "error":
        return updater.error() || "Could not check for updates.";
      default:
        return updater.lastChecked() ? `Last checked ${timeAgo(updater.lastChecked()! / 1000)}.` : "Not checked yet.";
    }
  };

  return (
    <>
      <div class="update-hero">
        <div class="update-hero-copy">
          <span class="update-version">Starex {updater.appVersion()}</span>
          <span class="update-status" data-status={status()}>
            {line()}
          </span>
        </div>
        <Show
          when={status() === "available"}
          fallback={
            <Button icon={<RefreshCw />} loading={status() === "checking"} onClick={() => void checkForUpdates(true)}>
              Check now
            </Button>
          }
        >
          <Button variant="primary" onClick={() => setDialog({ type: "update" })}>
            View update
          </Button>
        </Show>
      </div>
      <Group title="Automatic updates">
        <Switch
          label="Check for updates when Starex starts"
          description="Starex asks its GitHub releases for a newer version. Updates are signed and install only when you choose."
          checked={store.settings.checkForUpdates}
          onChange={(v) => void saveSettings({ checkForUpdates: v })}
        />
      </Group>
      <Group title="Release notes">
        <Row label="Changelog" description="Everything that changed in each version.">
          <Button icon={<ScrollText />} onClick={() => setDialog({ type: "changelog" })}>
            Open
          </Button>
        </Row>
      </Group>
    </>
  );
}

const osNames: Record<string, string> = { windows: "Windows", macos: "macOS", linux: "Linux" };
const archNames: Record<string, string> = { x86_64: "x64", aarch64: "ARM64", x86: "x86" };

function About() {
  const platform = () =>
    [osNames[store.hostOs] ?? store.hostOs, archNames[store.hostArch] ?? store.hostArch].filter(Boolean).join(" · ");

  return (
    <div class="about">
      <header class="about-header">
        <div class="about-mark">
          <StarMark size={64} />
        </div>
        <div class="about-identity">
          <h2 class="about-name">Starex</h2>
          <p class="about-tagline">A clean, fast launcher for the games on your drive.</p>
        </div>
      </header>

      <dl class="about-details setting-card">
        <div class="about-row">
          <dt>Version</dt>
          <dd>{updater.appVersion()}</dd>
        </div>
        <div class="about-row">
          <dt>Platform</dt>
          <dd>{platform()}</dd>
        </div>
        <div class="about-row">
          <dt>License</dt>
          <dd>MIT, free and open source</dd>
        </div>
      </dl>

      <div class="about-actions">
        <Button icon={<ArrowUpRight />} onClick={() => void api.openLink(store.repository)}>
          Source code
        </Button>
        <Button icon={<Bug />} onClick={() => void api.openLink(`${store.repository}/issues/new/choose`)}>
          Report an issue
        </Button>
      </div>

      <CreatorCredit />
    </div>
  );
}

const CREATOR = "inthecayenne";

/** "Made with ♥ by" and a pill with the creator's X profile. */
function CreatorCredit() {
  const [avatarFailed, setAvatarFailed] = createSignal(false);
  return (
    <div class="about-credit">
      <span class="about-credit-text">
        Made with <Heart class="about-credit-heart" aria-label="love" /> by
      </span>
      <button
        type="button"
        class="creator-pill"
        title={`@${CREATOR} on X`}
        onClick={() => void api.openLink(`https://x.com/${CREATOR}`)}
      >
        <Show when={!avatarFailed()} fallback={<span class="creator-avatar creator-avatar-fallback">{CREATOR[0]?.toUpperCase()}</span>}>
          <img
            class="creator-avatar"
            src={`https://unavatar.io/x/${CREATOR}`}
            alt=""
            referrerPolicy="no-referrer"
            onError={() => setAvatarFailed(true)}
          />
        </Show>
        <span class="creator-handle">@{CREATOR}</span>
      </button>
    </div>
  );
}

/** Twitch developer credentials, for game details from IGDB. */
function IgdbSettings() {
  const [id, setId] = createSignal(store.settings.igdbClientId);
  const [secret, setSecret] = createSignal(store.settings.igdbClientSecret);
  const [reveal, setReveal] = createSignal(false);
  const dirty = () => id().trim() !== store.settings.igdbClientId || secret().trim() !== store.settings.igdbClientSecret;
  return (
    <Group title="Game details · IGDB">
      <div class="setting-stack">
        <div class="switch-copy">
          <span class="switch-label">Client ID and secret</span>
          <span class="switch-description">
            Adds a description, genres, release date, developer and time to beat to each game. IGDB is free with a Twitch
            developer application. Starex sends a game's name to IGDB only when it looks up that game.
          </span>
        </div>
        <input
          class="input mono"
          placeholder="Client ID"
          value={id()}
          spellcheck={false}
          autocomplete="off"
          onInput={(e) => setId(e.currentTarget.value)}
        />
        <div class="input-row">
          <input
            class="input mono"
            type={reveal() ? "text" : "password"}
            placeholder="Client secret"
            value={secret()}
            spellcheck={false}
            autocomplete="off"
            onInput={(e) => setSecret(e.currentTarget.value)}
          />
          <Button variant="secondary" onClick={() => setReveal((v) => !v)} icon={reveal() ? <EyeOff /> : <Eye />} aria-label={reveal() ? "Hide secret" : "Show secret"} />
          <Button
            variant="primary"
            disabled={!dirty()}
            onClick={() => void saveSettings({ igdbClientId: id().trim(), igdbClientSecret: secret().trim() })}
          >
            Save
          </Button>
        </div>
        <button type="button" class="text-link" onClick={() => void api.openLink("https://dev.twitch.tv/console/apps")}>
          Create a Twitch application <ArrowUpRight />
        </button>
      </div>
    </Group>
  );
}
