import {
  CircleCheck,
  CircleX,
  Ellipsis,
  Fingerprint,
  FolderOpen,
  Image,
  Info,
  Play,
  ScanSearch,
  ShieldAlert,
  ShieldCheck,
  ShieldX,
  SlidersHorizontal,
  Sparkles,
  Square,
  Star,
  Trash2,
  TriangleAlert,
} from "./icons";
import { createMemo, createResource, createSignal, For, Match, onCleanup, Show, Switch } from "solid-js";
import { api, fileUrl } from "../lib/api";
import { generatedArt } from "../lib/art";
import { bytes, clock, count, playtime, relativeTo, timeAgo } from "../lib/format";
import {
  cancelTask,
  fetchArtwork,
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
import type { CheckStatus, Game } from "../lib/types";
import { FadeImage } from "./GameArt";
import { SavesPanel } from "./SavesPanel";
import { Button, IconButton } from "./ui/Button";
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
        <FilesPanel game={game()} />
        <SavesPanel game={game()} />
        <LaunchPanel game={game()} />
      </div>
    </div>
  );
}

function Hero(props: { game: Game; now: number }) {
  const g = () => props.game;
  const startedAt = () => store.running[g().id];
  const verification = () => g().lastVerification;

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
          <Show when={g().lastScan?.status === "threats"}>
            <span class="chip" data-tone="fail">
              <ShieldX />
              Threats found
            </span>
          </Show>
          <Show when={verification()} fallback={<span class="chip">Not verified</span>}>
            {(v) => <StatusChip status={v().status} at={v().at} />}
          </Show>
          <Show when={g().lastPlayed}>{(at) => <span class="hero-meta-item">Played {timeAgo(at(), props.now)}</span>}</Show>
          <Show when={g().playtimeSecs > 0}>
            <span class="hero-meta-item">{playtime(g().playtimeSecs)} total</span>
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
            <Match when={true}>
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
            </Match>
          </Switch>

          <IconButton
            label="Quick check"
            variant="glass"
            size="lg"
            disabled={!!store.tasks[g().id]}
            onClick={() => void verify(g().id, "quick")}
          >
            <ShieldCheck />
          </IconButton>
          <IconButton label="Open game folder" variant="glass" size="lg" onClick={() => void api.reveal(g().id, "folder")}>
            <FolderOpen />
          </IconButton>
          <IconButton
            label="Properties"
            variant="glass"
            size="lg"
            onClick={() => setDialog({ type: "properties", id: g().id })}
          >
            <SlidersHorizontal />
          </IconButton>
          <Menu
            items={() => [
              {
                label: g().favorite ? "Remove from favorites" : "Add to favorites",
                icon: <Star />,
                onSelect: () => void patchGame(g().id, { favorite: !g().favorite }),
              },
              {
                label: "Change artwork…",
                icon: <Image />,
                onSelect: () => setDialog({ type: "properties", id: g().id, tab: "artwork" }),
              },
              {
                label: "Find artwork online",
                icon: <Sparkles />,
                disabled: !store.settings.steamgriddbKey,
                onSelect: () => void fetchArtwork(g().id, false),
              },
              {
                label: "Show executable",
                icon: <ScanSearch />,
                onSelect: () => void api.reveal(g().id, "executable"),
              },
              {
                label: "Remove from library…",
                icon: <Trash2 />,
                danger: true,
                separatorBefore: true,
                onSelect: () => setDialog({ type: "remove", id: g().id }),
              },
            ]}
            trigger={(p) => (
              <IconButton label="More" variant="glass" size="lg" {...p}>
                <Ellipsis />
              </IconButton>
            )}
          />
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
  const [showIssues, setShowIssues] = createSignal(false);

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
        <Show when={props.game.lastVerification}>{(v) => <StatusChip status={v().status} at={v().at} />}</Show>
      </header>

      <Show when={task()}>
        {(t) => (
          <div class="task">
            <div class="task-row">
              <span class="task-label">
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

      <Show when={!task() && report()}>
        {(r) => (
          <ul class="checks">
            <For each={r().checks}>
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
            <Show when={r().issues.length > 0}>
              <li class="issues">
                <button type="button" class="issues-toggle" onClick={() => setShowIssues((v) => !v)} aria-expanded={showIssues()}>
                  {showIssues() ? "Hide" : "Show"} {count(r().missing + r().modified, "affected file")}
                </button>
                <Show when={showIssues()}>
                  <ul class="issue-list">
                    <For each={r().issues}>
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
        )}
      </Show>

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
        </h2>
        <p class="stat-big">{playtime(props.game.playtimeSecs)}</p>
      </div>
      <dl class="stat-grid">
        <div>
          <dt>Sessions</dt>
          <dd>{props.game.sessions.toLocaleString()}</dd>
        </div>
        <div>
          <dt>Last played</dt>
          <dd>{props.game.lastPlayed ? timeAgo(props.game.lastPlayed) : "Never"}</dd>
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
      </dl>
    </section>
  );
}
