import logoUrl from "../assets/logo.svg";

/**
 * The Arcadex mark: original 8-bit pixel art — a play triangle with a sprite
 * outline over a brick ground, in the NES sky blue.
 *
 * Rendered from the same SVG that generates every app icon size, so the
 * taskbar and the sidebar can never drift apart. `pixelated` matters: without
 * it the browser smooths the hard edges and the pixel art turns to mush.
 */
interface Props {
  size?: number;
  className?: string;
}

export default function Logo({ size = 22, className }: Props) {
  return (
    <img
      src={logoUrl}
      width={size}
      height={size}
      alt="Arcadex"
      className={className}
      style={{ imageRendering: "pixelated" }}
    />
  );
}
