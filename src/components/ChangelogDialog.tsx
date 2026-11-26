import { ArrowUpRight, X } from "./icons";
import { createResource, For, type JSX, Match, Show, Switch } from "solid-js";
import { api, errorText } from "../lib/api";
import { setDialog, store } from "../lib/store";
import type { Release } from "../lib/types";
import { updater } from "../lib/updater";
import { Button, IconButton } from "./ui/Button";
import { Dialog } from "./ui/Dialog";
import { Spinner } from "./ui/Spinner";

/** Release notes are fetched once per run; reopening the changelog is instant. */
let cached: Release[] | undefined;

type Block = { kind: "heading"; text: string } | { kind: "item"; text: string } | { kind: "text"; text: string };

/** Starex's release notes from its GitHub releases, newest first. */
export function ChangelogDialog() {
  const [releases, { refetch }] = createResource<Release[]>(async (_, info) => {
    if (cached && !info.refetching) return cached;
    cached = await api.fetchChangelog();
    return cached;
  });
  const back = () => setDialog({ type: "settings", section: "updates" });

  return (
    <Dialog title="Changelog" onClose={back} size="lg" bare class="changelog">
      <header class="changelog-header">
        <div class="changelog-heading">
          <h2 class="changelog-title">What's new</h2>
          <p class="changelog-subtitle">Release notes for every version of Starex.</p>
        </div>
        <IconButton label="Close" size="sm" onClick={back}>
          <X />
        </IconButton>
      </header>

      <div class="changelog-scroll">
        <Switch>
          <Match when={releases.loading}>
            <div class="changelog-state">
              <Spinner size={18} />
              <span>Loading release notes…</span>
            </div>
          </Match>
          <Match when={releases.error}>
            <div class="changelog-state">
              <span class="changelog-state-title">Couldn't load the changelog</span>
              <span>{errorText(releases.error)}</span>
              <Button size="sm" onClick={() => void refetch()}>
                Try again
              </Button>
            </div>
          </Match>
          <Match when={releases()?.length === 0}>
            <div class="changelog-state">
              <span class="changelog-state-title">No releases yet</span>
              <span>Release notes appear here once the first version is published.</span>
            </div>
          </Match>
          <Match when={releases()}>
            <ol class="changelog-list">
              <For each={releases()}>
                {(release, i) => <ReleaseEntry release={release} latest={i() === 0} index={i()} />}
              </For>
            </ol>
          </Match>
        </Switch>
      </div>

      <footer class="dialog-footer changelog-footer">
        <Button variant="ghost" icon={<ArrowUpRight />} onClick={() => void api.openLink(`${store.repository}/releases`)}>
          View on GitHub
        </Button>
        <div class="dialog-footer-spacer" />
        <Button variant="primary" onClick={back} data-autofocus>
          Done
        </Button>
      </footer>
    </Dialog>
  );
}

function ReleaseEntry(props: { release: Release; latest: boolean; index: number }) {
  const blocks = () => parseNotes(props.release.notes, props.release.version);
  const installed = () => props.release.version === updater.appVersion();
  const date = () =>
    props.release.publishedAt
      ? new Date(props.release.publishedAt).toLocaleDateString(undefined, { month: "short", day: "numeric", year: "numeric" })
      : "";

  return (
    <li class="release" style={{ "animation-delay": `${Math.min(props.index, 6) * 40}ms` }}>
      <div class="release-meta">
        <span class="release-version">{props.release.version}</span>
        <Show when={date()}>
          <time class="release-date" dateTime={props.release.publishedAt ?? undefined}>
            {date()}
          </time>
        </Show>
        <div class="release-tags">
          <Show when={props.latest}>
            <span class="release-tag" data-tone="accent">
              Latest
            </span>
          </Show>
          <Show when={installed()}>
            <span class="release-tag">Installed</span>
          </Show>
          <Show when={props.release.prerelease}>
            <span class="release-tag">Pre-release</span>
          </Show>
        </div>
      </div>

      <div class="release-body">
        <Show when={blocks().length > 0} fallback={<p class="release-text release-empty">No notes for this version.</p>}>
          <For each={groups(blocks())}>
            {(group) => (
              <section class="release-section">
                <Show when={group.heading}>
                  <h3 class="release-section-title">{group.heading}</h3>
                </Show>
                <For each={group.blocks}>
                  {(block) =>
                    block.kind === "item" ? (
                      <div class="release-item">
                        <span class="release-dot" aria-hidden="true" />
                        <span>{inline(block.text)}</span>
                      </div>
                    ) : (
                      <p class="release-text">{inline(block.text)}</p>
                    )
                  }
                </For>
              </section>
            )}
          </For>
        </Show>
      </div>
    </li>
  );
}

/** Turns a release's Markdown into headings, list items and paragraphs, minus GitHub's noise. */
function parseNotes(markdown: string, version: string): Block[] {
  const blocks: Block[] = [];
  for (const raw of markdown.replace(/\r/g, "").split("\n")) {
    const line = raw.trim();
    if (!line || /^(-{3,}|\*{3,}|<!--.*-->)$/.test(line)) continue;
    // GitHub's own footer line.
    if (/^\*\*Full Changelog\*\*/i.test(line)) continue;
    const heading = /^#{1,6}\s+(.*)$/.exec(line);
    if (heading) {
      const text = clean(heading[1] ?? "");
      // release-please opens with a heading that repeats the version and date, shown beside it already.
      if (!text || text.replace(/^v/, "").startsWith(version) || text.includes(`[${version}]`)) continue;
      blocks.push({ kind: "heading", text });
      continue;
    }
    const item = /^[*+-]\s+(.*)$/.exec(line);
    if (item) {
      const text = clean(item[1] ?? "");
      if (text) blocks.push({ kind: "item", text });
      continue;
    }
    const text = clean(line);
    if (text) blocks.push({ kind: "text", text });
  }
  return blocks;
}

/** Drops commit hashes, author credits and conventional-commit prefixes. */
function clean(text: string): string {
  return text
    .replace(/\s*\(\[[0-9a-f]{7,40}\]\([^)]*\)\)/gi, "")
    .replace(/,?\s*closes\s+\[#\d+\]\([^)]*\)/gi, "")
    .replace(/\s+by\s+@[\w-]+(\s+in\s+\S+)?$/i, "")
    .replace(/\s+in\s+https:\/\/github\.com\/\S+$/i, "")
    .replace(/^(feat|fix|chore|docs|refactor|perf|style|test|build|ci|revert)(\([^)]*\))?!?:\s*/i, "")
    .trim()
    .replace(/^./, (c) => c.toUpperCase());
}

function groups(blocks: Block[]) {
  const out: { heading: string; blocks: Block[] }[] = [];
  for (const block of blocks) {
    if (block.kind === "heading") out.push({ heading: block.text, blocks: [] });
    else {
      if (out.length === 0) out.push({ heading: "", blocks: [] });
      out[out.length - 1]?.blocks.push(block);
    }
  }
  return out.filter((g) => g.blocks.length > 0);
}

/** Bold, code and links, rendered as elements (never as raw HTML). */
function inline(text: string): JSX.Element[] {
  const parts: JSX.Element[] = [];
  const pattern = /\*\*([^*]+)\*\*|`([^`]+)`|\[([^\]]+)\]\(([^)\s]+)\)/g;
  let last = 0;
  for (const m of text.matchAll(pattern)) {
    const at = m.index ?? 0;
    if (at > last) parts.push(text.slice(last, at));
    if (m[1] !== undefined) parts.push(<strong>{m[1]}</strong>);
    else if (m[2] !== undefined) parts.push(<code>{m[2]}</code>);
    else {
      const url = m[4] ?? "";
      parts.push(
        <a
          href={url}
          class="release-link"
          onClick={(e) => {
            e.preventDefault();
            void api.openLink(url).catch(() => undefined);
          }}
        >
          {m[3]}
        </a>,
      );
    }
    last = at + m[0].length;
  }
  if (last < text.length) parts.push(text.slice(last));
  return parts;
}
