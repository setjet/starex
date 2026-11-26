export function Spinner(props: { size?: number; class?: string }) {
  const size = () => props.size ?? 16;
  return (
    <svg class={`spinner ${props.class ?? ""}`} width={size()} height={size()} viewBox="0 0 24 24" aria-hidden="true">
      <circle cx="12" cy="12" r="9" fill="none" stroke="currentColor" stroke-opacity="0.2" stroke-width="2.5" />
      <path d="M21 12a9 9 0 0 0-9-9" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" />
    </svg>
  );
}

export function Progress(props: { value: number | null; class?: string }) {
  return (
    <div
      class={`progress ${props.value === null ? "progress-indeterminate" : ""} ${props.class ?? ""}`}
      role="progressbar"
      aria-valuemin={0}
      aria-valuemax={100}
      aria-valuenow={props.value === null ? undefined : Math.round(props.value * 100)}
    >
      <div class="progress-fill" style={{ transform: `scaleX(${props.value ?? 0.3})` }} />
    </div>
  );
}
