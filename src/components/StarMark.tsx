/** The Starex mark: a four-point star. Inherits the current text colour. */
export function StarMark(props: { size?: number; class?: string }) {
  return (
    <svg
      class={props.class}
      width={props.size ?? 16}
      height={props.size ?? 16}
      viewBox="0 0 24 24"
      fill="currentColor"
      aria-hidden="true"
    >
      <path d="M12 1.5c.55 5.1 2.4 8.4 10.5 10.1v.8c-8.1 1.7-9.95 5-10.5 10.1-.55-5.1-2.4-8.4-10.5-10.1v-.8C9.6 9.9 11.45 6.6 12 1.5Z" />
    </svg>
  );
}
