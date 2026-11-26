import { createUniqueId, type JSX, Show } from "solid-js";

interface Props {
  checked: boolean;
  onChange: (checked: boolean) => void;
  label: string;
  description?: JSX.Element;
  disabled?: boolean;
}

/** A binary setting that applies immediately. */
export function Switch(props: Props) {
  const id = createUniqueId();
  return (
    <div class="switch-field" data-disabled={props.disabled || undefined}>
      <div class="switch-copy">
        <label class="switch-label" for={id}>
          {props.label}
        </label>
        <Show when={props.description}>
          <div class="switch-description">{props.description}</div>
        </Show>
      </div>
      <button
        id={id}
        type="button"
        role="switch"
        class="switch"
        aria-checked={props.checked}
        data-checked={props.checked || undefined}
        disabled={props.disabled}
        onClick={() => props.onChange(!props.checked)}
      >
        <span class="switch-thumb" />
      </button>
    </div>
  );
}

interface SegmentedProps<T extends string> {
  value: T;
  options: { value: T; label: string }[];
  onChange: (value: T) => void;
  label: string;
}

/** One choice from two to four short options. */
export function Segmented<T extends string>(props: SegmentedProps<T>) {
  return (
    <div class="segmented" role="radiogroup" aria-label={props.label}>
      {props.options.map((option) => (
        <button
          type="button"
          role="radio"
          class="segmented-item"
          aria-checked={props.value === option.value}
          data-active={props.value === option.value || undefined}
          onClick={() => props.onChange(option.value)}
        >
          {option.label}
        </button>
      ))}
    </div>
  );
}

export function Field(props: { label: string; description?: JSX.Element; children: JSX.Element; for?: string }) {
  return (
    <div class="field">
      <label class="field-label" for={props.for}>
        {props.label}
      </label>
      {props.children}
      <Show when={props.description}>
        <div class="field-description">{props.description}</div>
      </Show>
    </div>
  );
}
