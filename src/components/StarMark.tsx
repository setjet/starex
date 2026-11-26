import mark from "../assets/starex-mark.webp";

/** The Starex mark: the app icon's star. */
export function StarMark(props: { size?: number; class?: string }) {
  return (
    <img
      class={props.class}
      src={mark}
      width={props.size ?? 16}
      height={props.size ?? 16}
      alt=""
      aria-hidden="true"
      draggable={false}
    />
  );
}
