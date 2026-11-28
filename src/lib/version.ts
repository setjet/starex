import { store } from "./store";
import { updater } from "./updater";

/**
 * The version shown in the interface: "0.1.0", or "Test build 003" for a private test build,
 * which shows its build number instead of a version.
 */
export const versionLabel = () => (store.testBuild ? `Test build ${store.testBuild}` : updater.appVersion());
