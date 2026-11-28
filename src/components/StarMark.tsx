import outlined from "../assets/starex-logo-outline.webp";
import mark from "../assets/starex-logo.webp";
import { activeTheme } from "../lib/theme";

/**
 * The Starex mark: the app icon's S. On a light interface it carries a thin dark outline, so the
 * white S stays visible. `onDark` keeps the plain S for places that are always dark (Big Picture).
 */
export function StarMark(props: { size?: number; class?: string; onDark?: boolean }) {
  return (
    <img
      class={props.class}
      src={!props.onDark && activeTheme() === "light" ? outlined : mark}
      width={props.size ?? 16}
      height={props.size ?? 16}
      alt=""
      aria-hidden="true"
      draggable={false}
    />
  );
}
