import { ArrowUpRight, Bug, Eye, EyeOff, FolderOpen, Image, Info, Library, RefreshCw, Settings2 } from "./icons";
import { createSignal, For, type JSX, Match, Show, Switch as SolidSwitch } from "solid-js";
import { api } from "../lib/api";
import { timeAgo } from "../lib/format";
import { type SettingsSection, saveSettings, setDialog, store } from "../lib/store";
import { checkForUpdates, updater } from "../lib/updater";
import { StarMark } from "./StarMark";
import { Button } from "./ui/Button";
import { Dialog } from "./ui/Dialog";
import { Segmented, Switch } from "./ui/Switch";

const sections: { id: SettingsSection; label: string; icon: () => JSX.Element }[] = [
  { id: "general", label: "General", icon: () => <Settings2 /> },
  { id: "library", label: "Library", icon: () => <Library /> },
  { id: "artwork", label: "Artwork", icon: () => <Image /> },
  { id: "updates", label: "Updates", icon: () => <RefreshCw /> },
  { id: "about", label: "About", icon: () => <Info /> },
];

export function SettingsDialog(props: { section?: SettingsSection }) {
  const [section, setSection] = createSignal<SettingsSection>(props.section ?? "general");
  const s = () => store.settings;

  return (
    <Dialog title="Settings" onClose={() => setDialog(null)} size="xl" bare class="settings">
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
                <Switch
                  label="Scan new games for viruses"
                  description="Your antivirus checks a game's programs and scripts when you add it. Takes seconds."
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
              <Show when={store.hostOs && store.hostOs !== "windows"}>
                <WineSetting />
              </Show>
              <Group title="Appearance">
                <Switch
                  label="Reduce motion"
                  description="Turns off transitions and animated effects."
                  checked={s().reduceMotion}
                  onChange={(v) => void saveSettings({ reduceMotion: v })}
                />
              </Group>
            </Match>

            <Match when={section() === "library"}>
              <SettingsHeader title="Library" />
              <Group title="Order">
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
              <Group title="Data">
                <Row label="Data folder" description={store.dataDir}>
                  <Button icon={<FolderOpen />} onClick={() => void api.openDataDir()}>
                    Open
                  </Button>
                </Row>
              </Group>
            </Match>

            <Match when={section() === "artwork"}>
              <SettingsHeader title="Artwork" />
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

function ArtworkSettings() {
  const [key, setKey] = createSignal(store.settings.steamgriddbKey);
  const [reveal, setReveal] = createSignal(false);
  const dirty = () => key().trim() !== store.settings.steamgriddbKey;

  return (
    <>
      <Group title="SteamGridDB">
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
          <Button icon={<ArrowUpRight />} onClick={() => void api.openLink(`${store.repository}/releases`)}>
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
          <div class="empty-glow" />
          <StarMark size={34} />
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
    </div>
  );
}
