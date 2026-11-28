import {
  Check,
  ChevronDown,
  CircleCheck,
  Flag,
  WifiOff,
  CircleX,
  Fingerprint,
  Info,
  Play,
  ScanSearch,
  ShieldAlert,
  ShieldCheck,
  ShieldX,
  Square,
  TriangleAlert,
} from "./icons";
import { createMemo, createResource, createSignal, For, Match, onCleanup, Show, Switch } from "solid-js";
import { api, fileUrl } from "../lib/api";
import { generatedArt } from "../lib/art";
import { bytes, clock, count, playtime, relativeTo, timeAgo } from "../lib/format";
import {
  cancelTask,
  isStoreGame,
  lastPlayedOf,
  playtimeOf,
  launch,
  patchGame,
  recordBaseline,
  scan,
  scanReports,
  setDialog,
  stop,
  store,
  verify,
} from "../lib/store";
import type { CheckStatus, Game, VerifyReport } from "../lib/types";
import { ActivityPanel } from "./ActivityPanel";
import { FadeImage } from "./GameArt";
import { LocalStatus } from "./LocalStatus";
import { SavesPanel } from "./SavesPanel";
import { SteamPanels } from "./SteamPanels";
import { Button } from "./ui/Button";
import { Menu } from "./ui/Menu";
import { Progress, Spinner } from "./ui/Spinner";

export function GameView(props: { game: Game }) {
  const game = () => props.game;
  const [now, setNow] = createSignal(Date.now() / 1000);
  const timer = setInterval(() => setNow(Date.now() / 1000), 1000);
  onCleanup(() => clearInterval(timer));

  return (
    <div class="game page-enter">
      <Hero game={game()} now={now()} />
      <div class="game-body">
        <StatsPanel game={game()} />
        <ActivityPanel game={game()} />
        <SteamPanels game={game()} />
        {/* A launcher's game keeps its files and saves to the launcher. */}
        <Show when={!isStoreGame(game())}>
          <FilesPanel game={game()} />
          <SavesPanel game={game()} />
          <LaunchPanel game={game()} />
        </Show>
      </div>
    </div>
  );
}

function Hero(props: { game: Game; now: number }) {
  const g = () => props.game;
  const startedAt = () => store.running[g().id];

  return (
    <section class="hero" aria-label={g().name}>
      <div class="hero-art" style={{ background: generatedArt(g().name, 1.35) }}>
        <FadeImage src={fileUrl(g().artwork.banner ?? g().artwork.cover)} class="hero-image" />
      </div>
      <div class="hero-scrim" />
      <div class="hero-content">
        <Show
          when={g().artwork.logo}
          fallback={<h1 class="hero-title">{g().name}</h1>}
        >
          <h1 class="hero-logo-wrap">
            <FadeImage
              src={fileUrl(g().artwork.logo)}
              alt={g().name}
              class="hero-logo"
              fallback={<span class="hero-title">{g().name}</span>}
            />
          </h1>
        </Show>

        <div class="hero-meta">
          <Show when={g().completed}>
            <span class="chip" data-tone="pass">
              <Flag />
              Finished
            </span>
          </Show>
          <Show when={g().offline}>
            <span class="chip">
              <WifiOff />
              Offline
            </span>
          </Show>
          <Show when={g().lastScan?.status === "threats"}>
            <span class="chip" data-tone="fail">
              <ShieldX />
              Threats found
            </span>
          </Show>
        </div>

        <div class="hero-actions">
          <Switch>
            <Match when={startedAt()}>
              {(at) => (
                <>
                  <div class="play-btn is-running" role="status">
                    <span class="play-pulse" aria-hidden="true" />
                    <span>Playing</span>
                    <span class="play-clock">{clock(props.now - at())}</span>
                  </div>
                  <Button variant="glass" size="lg" icon={<Square />} onClick={() => void stop(g().id)}>
                    Stop
                  </Button>
                </>
              )}
            </Match>
            <Match when={store.notInstalled[g().id]}>
              <button type="button" class="play-btn is-unavailable" disabled>
                <span>Not installed</span>
              </button>
            </Match>
            <Match when={true}>
              <div class="play-group" data-split={(!isStoreGame(g()) && g().profiles.length > 0) || undefined}>
                <button
                  type="button"
                  class="play-btn"
                  disabled={store.launching[g().id]}
                  onClick={() => void launch(g().id)}
                  data-autofocus
                >
                  <Show when={store.launching[g().id]} fallback={<Play class="play-icon" />}>
                    <Spinner size={18} />
                  </Show>
                  <span>{store.launching[g().id] ? "Starting" : "Play"}</span>
                </button>
                <Show when={!isStoreGame(g()) && g().profiles.length > 0}>
                  <Menu
                    align="start"
                    items={() => [
                      {
                        label: "Standard",
                        icon: g().activeProfile ? <span class="menu-icon-space" /> : <Check />,
                        onSelect: () => void patchGame(g().id, { activeProfile: null }),
                      },
                      ...g().profiles.map((profile) => ({
                        label: profile.name,
                        icon: g().activeProfile === profile.id ? <Check /> : <span class="menu-icon-space" />,
                        onSelect: () => void patchGame(g().id, { activeProfile: profile.id }),
                      })),
                    ]}
                    trigger={(p) => (
                      <button type="button" class="play-profile" aria-label="Launch profile" {...p}>
                        <span>{g().profiles.find((x) => x.id === g().activeProfile)?.name ?? "Standard"}</span>
                        <ChevronDown />
                      </button>
                    )}
                  />
                </Show>
              </div>
            </Match>
          </Switch>

          <LocalStatus game={g()} />
        </div>
      </div>
    </section>
  );
}

const statusText: Record<CheckStatus, string> = {
  pass: "Verified",
  info: "Verified",
  warn: "Needs attention",
  fail: "Problems found",
};

function StatusChip(props: { status: CheckStatus; at: number }) {
  return (
    <span class="chip" data-tone={props.status} title={`Checked ${timeAgo(props.at)}`}>
      <StatusIcon status={props.status} />
      {statusText[props.status]}
    </span>
  );
}

function StatusIcon(props: { status: CheckStatus }) {
  return (
    <Switch>
      <Match when={props.status === "pass"}>
        <CircleCheck class="status-icon" data-tone="pass" />
      </Match>
      <Match when={props.status === "warn"}>
        <TriangleAlert class="status-icon" data-tone="warn" />
      </Match>
      <Match when={props.status === "fail"}>
        <CircleX class="status-icon" data-tone="fail" />
      </Match>
      <Match when={props.status === "info"}>
        <Info class="status-icon" data-tone="info" />
      </Match>
    </Switch>
  );
}

function FilesPanel(props: { game: Game }) {
  const id = () => props.game.id;
  const task = () => store.tasks[id()];
  const report = () => store.reports[id()];
  const [baseline, { refetch }] = createResource(id, (gameId) => api.baselineInfo(gameId).catch(() => null));

  const progress = createMemo(() => {
    const p = task()?.progress;
    if (!p) return null;
    if (p.totalBytes > 0) return p.doneBytes / p.totalBytes;
    if (p.totalFiles > 0) return p.doneFiles / p.totalFiles;
    return null;
  });

  const summary = () => {
    const v = props.game.lastVerification;
    if (!v) return "Not checked yet. A quick check takes seconds; a full check reads every byte.";
    const kind = v.mode === "full" ? "Full check" : "Quick check";
    const files = v.filesChecked > 0 ? ` · ${count(v.filesChecked, "file")}` : "";
    return `${kind} ${timeAgo(v.at)}${files} · ${v.durationMs < 1000 ? `${v.durationMs} ms` : `${(v.durationMs / 1000).toFixed(1)} s`}`;
  };

  const record = async () => {
    const info = await recordBaseline(id());
    if (info) void refetch();
  };

  return (
    <section class="panel panel-files" aria-labelledby="files-title">
      <header class="panel-header">
        <div class="panel-heading">
          <h2 id="files-title" class="panel-title">
            Game files
          </h2>
          <p class="panel-subtitle">{summary()}</p>
        </div>
        <Show when={props.game.lastVerification} fallback={<span class="chip">Not verified</span>}>
          {(v) => <StatusChip status={v().status} at={v().at} />}
        </Show>
      </header>

      <Show when={task()}>
        {(t) => (
          <div class="task">
            <div class="task-row">
              <span class="task-label">
                <Spinner size={14} />
                {{ baseline: "Recording baseline", verify: "Checking files", scan: "Scanning for viruses" }[t().task]}
                <Show when={t().progress.totalFiles > 0}>
                  {" "}
                  · {t().progress.doneFiles.toLocaleString()} of {count(t().progress.totalFiles, "file")}
                </Show>
                <Show when={t().progress.totalBytes > 0}>
                  {" "}
                  · {bytes(t().progress.doneBytes)} of {bytes(t().progress.totalBytes)}
                </Show>
              </span>
              <Button variant="ghost" size="sm" onClick={() => void cancelTask(id())}>
                Cancel
              </Button>
            </div>
            <Progress value={progress()} />
          </div>
        )}
      </Show>

      <Show when={!task() && report()}>{(r) => <CheckResult report={r()} />}</Show>

      <div class="baseline">
        <Fingerprint class="baseline-icon" />
        <div class="baseline-copy">
          <Show
            when={baseline()}
            fallback={
              <>
                <span class="baseline-title">No baseline yet</span>
                <span class="baseline-detail">
                  Record one while the game works. Later checks then catch every missing or changed file.
                </span>
              </>
            }
          >
            {(b) => (
              <>
                <span class="baseline-title">{b().shipped ? "Manifest from the game folder" : "Baseline recorded"}</span>
                <span class="baseline-detail">
                  {count(b().files, "file")} · {bytes(b().totalBytes)}
                  <Show when={b().generatedAt}>{(at) => <> · {timeAgo(at())}</>}</Show>
                </span>
              </>
            )}
          </Show>
        </div>
      </div>

      <VirusRow game={props.game} busy={!!task()} />

      <footer class="panel-actions">
        <Button icon={<ShieldCheck />} disabled={!!task()} onClick={() => void verify(id(), "quick")}>
          Quick check
        </Button>
        <Button icon={<ScanSearch />} disabled={!!task() || !baseline()} onClick={() => void verify(id(), "full")}>
          Full check
        </Button>
        <Show when={!baseline()?.shipped}>
          <Button variant="ghost" icon={<Fingerprint />} disabled={!!task()} onClick={() => void record()}>
            {baseline() ? "Record again" : "Record baseline"}
          </Button>
        </Show>
      </footer>
    </section>
  );
}

/** The outcome of a check: one calm line when all is well, the problems when it is not. */
function CheckResult(props: { report: VerifyReport }) {
  const [open, setOpen] = createSignal(false);
  const [showIssues, setShowIssues] = createSignal(false);
  const problems = () => props.report.checks.filter((c) => c.status === "warn" || c.status === "fail");
  const shown = () => (open() ? props.report.checks : problems());
  const headline = () => {
    const n = props.report.checks.length;
    if (props.report.status === "fail") return "Problems found";
    if (props.report.status === "warn") return "Some checks need attention";
    return `All ${n} checks passed`;
  };

  return (
    <div class="check-result" data-tone={props.report.status}>
      <div class="check-summary">
        <StatusIcon status={props.report.status} />
        <span class="check-summary-title">{headline()}</span>
        <button type="button" class="check-toggle" onClick={() => setOpen((v) => !v)} aria-expanded={open()}>
          {open() ? "Hide details" : problems().length ? "Show all" : "Details"}
        </button>
      </div>
      <Show when={shown().length > 0}>
        <ul class="checks">
          <For each={shown()}>
            {(check) => (
              <li class="check">
                <StatusIcon status={check.status} />
                <div class="check-copy">
                  <span class="check-title">{check.title}</span>
                  <span class="check-detail">{check.detail}</span>
                </div>
              </li>
            )}
          </For>
          <Show when={props.report.issues.length > 0}>
            <li class="issues">
              <button
                type="button"
                class="issues-toggle"
                onClick={() => setShowIssues((v) => !v)}
                aria-expanded={showIssues()}
              >
                {showIssues() ? "Hide" : "Show"} {count(props.report.missing + props.report.modified, "affected file")}
              </button>
              <Show when={showIssues()}>
                <ul class="issue-list">
                  <For each={props.report.issues}>
                    {(issue) => (
                      <li class="issue">
                        <span class="issue-kind" data-kind={issue.kind}>
                          {issue.kind === "missing" ? "Missing" : issue.kind === "unreadable" ? "Unreadable" : "Changed"}
                        </span>
                        <span class="issue-path">{issue.path}</span>
                      </li>
                    )}
                  </For>
                </ul>
              </Show>
            </li>
          </Show>
        </ul>
      </Show>
    </div>
  );
}

function VirusRow(props: { game: Game; busy: boolean }) {
  const last = () => props.game.lastScan;
  const report = () => scanReports[props.game.id];
  const title = () => {
    switch (last()?.status) {
      case "clean":
        return "No viruses found";
      case "threats":
        return `${count(last()?.threats ?? 0, "threat")} found`;
      case "unavailable":
        return "Virus scan unavailable";
      default:
        return "Not scanned for viruses";
    }
  };
  const detail = () => {
    const l = last();
    if (!l) return "Checks the game's programs and scripts with your antivirus. Takes seconds.";
    if (l.status === "unavailable") return report()?.message ?? "No antivirus could be reached.";
    return `${count(l.filesScanned, "program file")} · ${timeAgo(l.at)}`;
  };

  return (
    <div class="baseline" data-tone={last()?.status}>
      <Switch fallback={<ShieldCheck class="baseline-icon" />}>
        <Match when={last()?.status === "threats"}>
          <ShieldX class="baseline-icon" />
        </Match>
        <Match when={last()?.status === "unavailable"}>
          <ShieldAlert class="baseline-icon" />
        </Match>
      </Switch>
      <div class="baseline-copy">
        <span class="baseline-title">{title()}</span>
        <span class="baseline-detail">{detail()}</span>
        <Show when={last()?.status === "threats" && report()?.threats.length}>
          <ul class="issue-list">
            <For each={report()?.threats}>
              {(threat) => (
                <li class="issue">
                  <span class="issue-kind" data-kind="missing">
                    Threat
                  </span>
                  <span class="issue-path" title={threat.name ?? undefined}>
                    {threat.path}
                    {threat.name ? ` · ${threat.name}` : ""}
                  </span>
                </li>
              )}
            </For>
          </ul>
        </Show>
      </div>
      <Button size="sm" variant="ghost" class="baseline-action" disabled={props.busy} onClick={() => void scan(props.game.id)}>
        {last() ? "Scan again" : "Scan"}
      </Button>
    </div>
  );
}

function StatsPanel(props: { game: Game }) {
  return (
    <section class="panel stats-strip" aria-labelledby="stats-title">
      <div class="stats-main">
        <h2 id="stats-title" class="panel-eyebrow">
          Play time
          <Show when={props.game.storePlaytime?.totalSecs != null}>
            <span class="stat-source" title="Play time from Steam">
              Steam
            </span>
          </Show>
        </h2>
        <p class="stat-big">{playtime(playtimeOf(props.game))}</p>
      </div>
      <dl class="stat-grid">
        <Show when={props.game.storePlaytime?.recentSecs != null}>
          <div>
            <dt>Last 2 weeks</dt>
            <dd>{playtime(props.game.storePlaytime?.recentSecs ?? 0)}</dd>
          </div>
        </Show>
        <div>
          <dt>Sessions</dt>
          <dd>{props.game.sessions.toLocaleString()}</dd>
        </div>
        <div>
          <dt>Last played</dt>
          <dd>{lastPlayedOf(props.game) ? timeAgo(lastPlayedOf(props.game) ?? 0) : "Never"}</dd>
        </div>
        <div>
          <dt>Added</dt>
          <dd>{timeAgo(props.game.addedAt)}</dd>
        </div>
      </dl>
    </section>
  );
}

function LaunchPanel(props: { game: Game }) {
  return (
    <section class="panel" aria-labelledby="launch-title">
      <header class="panel-header-inline">
        <h2 id="launch-title" class="panel-eyebrow">
          Launch
        </h2>
        <Button variant="ghost" size="sm" onClick={() => setDialog({ type: "properties", id: props.game.id, tab: "launch" })}>
          Edit
        </Button>
      </header>
      <dl class="detail-list">
        <div>
          <dt>Executable</dt>
          <dd class="mono" title={props.game.executable}>
            {relativeTo(props.game.installDir, props.game.executable)}
          </dd>
        </div>
        <div>
          <dt>Options</dt>
          <dd class={props.game.launchArgs ? "mono" : "muted"}>{props.game.launchArgs || "None"}</dd>
        </div>
        <div>
          <dt>Folder</dt>
          <dd>
            <button type="button" class="link-path" title={props.game.installDir} onClick={() => void api.reveal(props.game.id, "folder")}>
              {props.game.installDir}
            </button>
          </dd>
        </div>
        <Show when={props.game.env.length > 0}>
          <div>
            <dt>Environment</dt>
            <dd class="muted">{count(props.game.env.length, "variable")}</dd>
          </div>
        </Show>
        <Show when={props.game.preLaunch}>
          <div>
            <dt>Before launch</dt>
            <dd class="mono" title={props.game.preLaunch}>
              {props.game.preLaunch}
            </dd>
          </div>
        </Show>
        <Show when={props.game.runAsAdmin && store.hostOs === "windows"}>
          <div>
            <dt>Runs as</dt>
            <dd>Administrator</dd>
          </div>
        </Show>
      </dl>
    </section>
  );
}
