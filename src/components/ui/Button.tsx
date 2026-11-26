import { type JSX, Show, splitProps } from "solid-js";
import { Spinner } from "./Spinner";

export type ButtonVariant = "primary" | "secondary" | "ghost" | "destructive" | "glass";
export type ButtonSize = "sm" | "md" | "lg";

type Props = JSX.ButtonHTMLAttributes<HTMLButtonElement> & {
  variant?: ButtonVariant;
  size?: ButtonSize;
  icon?: JSX.Element;
  loading?: boolean;
  full?: boolean;
};

/** An action with a label. `primary` is reserved for the single most important action in a view. */
export function Button(props: Props) {
  const [local, rest] = splitProps(props, ["variant", "size", "icon", "loading", "full", "children", "class", "disabled"]);
  return (
    <button
      type="button"
      {...rest}
      class={`btn btn-${local.variant ?? "secondary"} btn-${local.size ?? "md"} ${local.full ? "btn-full" : ""} ${local.class ?? ""}`}
      disabled={local.disabled || local.loading}
      aria-busy={local.loading || undefined}
    >
      <Show when={local.loading} fallback={local.icon}>
        <Spinner />
      </Show>
      <Show when={local.children}>
        <span class="btn-label">{local.children}</span>
      </Show>
    </button>
  );
}

type IconProps = JSX.ButtonHTMLAttributes<HTMLButtonElement> & {
  label: string;
  variant?: "ghost" | "glass" | "secondary";
  size?: "sm" | "md" | "lg";
  active?: boolean;
};

/** A standalone icon action. `label` names it for screen readers and the tooltip. */
export function IconButton(props: IconProps) {
  const [local, rest] = splitProps(props, ["label", "variant", "size", "active", "children", "class"]);
  return (
    <button
      type="button"
      {...rest}
      class={`icon-btn icon-btn-${local.variant ?? "ghost"} icon-btn-${local.size ?? "md"} ${local.class ?? ""}`}
      aria-label={local.label}
      title={local.label}
      data-active={local.active || undefined}
    >
      {local.children}
    </button>
  );
}
