import { ArrowUpRight, Bug, Star, X } from "./icons";
import { api } from "../lib/api";
import { saveSettings, setDialog, store } from "../lib/store";
import { StarMark } from "./StarMark";
import { Button, IconButton } from "./ui/Button";
import { Dialog } from "./ui/Dialog";

/** Shown once, the first time Starex opens. */
export function WelcomeDialog() {
  const close = () => {
    setDialog(null);
    void saveSettings({ welcomeSeen: true });
  };

  return (
    <Dialog title="Welcome to Starex" onClose={close} size="md" bare class="welcome">
      <IconButton label="Close" size="sm" class="welcome-close" onClick={close}>
        <X />
      </IconButton>

      <div class="welcome-body">
        <div class="welcome-mark" aria-hidden="true">
          <StarMark size={64} />
        </div>
        <h2 class="welcome-title">
          Welcome to Starex
          <span class="welcome-beta">Beta</span>
        </h2>
        <p class="welcome-text">
          Thank you for installing Starex. It is still in beta, so you may run into the occasional bug.
        </p>
      </div>

      <div class="welcome-links">
        <button
          type="button"
          class="welcome-link"
          onClick={() => void api.openLink(`${store.repository}/issues/new/choose`)}
        >
          <Bug />
          <span class="welcome-link-copy">
            <span class="welcome-link-title">Report a bug</span>
            <span class="welcome-link-text">Tell us on GitHub and we'll fix it.</span>
          </span>
          <ArrowUpRight class="welcome-link-arrow" />
        </button>
        <button type="button" class="welcome-link" onClick={() => void api.openLink(store.repository)}>
          <Star />
          <span class="welcome-link-copy">
            <span class="welcome-link-title">Star on GitHub</span>
            <span class="welcome-link-text">It helps others find Starex.</span>
          </span>
          <ArrowUpRight class="welcome-link-arrow" />
        </button>
      </div>

      <div class="welcome-footer">
        <Button variant="primary" size="lg" onClick={close}>
          Get started
        </Button>
      </div>
    </Dialog>
  );
}
