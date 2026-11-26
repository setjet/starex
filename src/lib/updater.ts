import { getVersion } from "@tauri-apps/api/app";
import { relaunch } from "@tauri-apps/plugin-process";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { createSignal } from "solid-js";
import { errorText } from "./api";

export type UpdateStatus = "idle" | "checking" | "upToDate" | "available" | "downloading" | "installing" | "error";

const [appVersion, setAppVersion] = createSignal("");
const [status, setStatus] = createSignal<UpdateStatus>("idle");
const [available, setAvailable] = createSignal<Update | null>(null);
const [progress, setProgress] = createSignal(0);
const [error, setError] = createSignal("");
const [lastChecked, setLastChecked] = createSignal<number | null>(null);

export const updater = { appVersion, status, available, progress, error, lastChecked };

export async function loadVersion() {
  try {
    setAppVersion(await getVersion());
  } catch {
    setAppVersion("dev");
  }
}

/**
 * Asks Starex's GitHub releases whether a newer version exists. The automatic check is silent;
 * a manual one reports "up to date" and errors.
 */
export async function checkForUpdates(manual = false) {
  if (status() === "checking" || status() === "downloading" || status() === "installing") return;
  setStatus("checking");
  setError("");
  try {
    const update = await check();
    setLastChecked(Date.now());
    setAvailable(update);
    setStatus(update ? "available" : "upToDate");
  } catch (e) {
    setStatus(manual ? "error" : "idle");
    setError(errorText(e));
  }
}

/** Downloads the update, installs it and restarts Starex. */
export async function installUpdate() {
  const update = available();
  if (!update) return;
  setStatus("downloading");
  setProgress(0);
  let total = 0;
  let received = 0;
  try {
    await update.downloadAndInstall((event) => {
      if (event.event === "Started") total = event.data.contentLength ?? 0;
      if (event.event === "Progress") {
        received += event.data.chunkLength;
        if (total > 0) setProgress(Math.min(1, received / total));
      }
      if (event.event === "Finished") setStatus("installing");
    });
    await relaunch();
  } catch (e) {
    setStatus("error");
    setError(errorText(e));
  }
}
