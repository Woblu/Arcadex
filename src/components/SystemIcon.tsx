/**
 * A glyph per system, drawn from the hardware.
 *
 * These used to be grouped by media - one cartridge, one disc, one handheld -
 * which meant a NES, an N64 and a Mega Drive were all the same picture.
 * Sorting a library by system is exactly the moment you want to tell them
 * apart, so each one now gets its own.
 *
 * They are drawings of the machines and their controllers, not reproductions
 * of anybody's logo or wordmark. A console's shape is what people actually
 * recognise, and three dozen near-identical trademarked marks would read worse
 * at this size anyway. All of it is our own line art on a 24x24 grid, kept to
 * a few strokes so it still holds up at 15px in a sidebar.
 */

type Glyph = React.ReactNode;

/** Systems with no drawing of their own fall back to one of these. */
type Family = "cartridge" | "disc" | "handheld" | "computer" | "chip";

const FAMILY: Record<string, Family> = {
  segacd: "disc",
  threedo: "disc",
  msx: "computer",
  scummvm: "computer",
};

// ------------------------------------------------------------- Nintendo

const nes: Glyph = (
  <>
    <rect x="2.5" y="7.5" width="19" height="9" rx="1.2" />
    <path d="M5.2 12h3.2M6.8 10.4v3.2" />
    <circle cx="15.4" cy="12" r="1.15" />
    <circle cx="18.6" cy="12" r="1.15" />
  </>
);

const snes: Glyph = (
  <>
    <path d="M4.2 8.4h15.6a3.1 3.1 0 0 1 0 6.2H4.2a3.1 3.1 0 0 1 0-6.2Z" />
    <path d="M5.6 11.5h2.8M7 10.1v2.8" />
    <circle cx="15.4" cy="10.3" r=".85" />
    <circle cx="17.7" cy="11.6" r=".85" />
    <circle cx="15.4" cy="12.9" r=".85" />
    <circle cx="13.1" cy="11.6" r=".85" />
  </>
);

const n64: Glyph = (
  <>
    <path d="M2.5 8.5h19v2.8a2.7 2.7 0 0 1-2.7 2.7h-1.4v5h-3.5v-5h-3.8v5H6.6v-5H5.2a2.7 2.7 0 0 1-2.7-2.7Z" />
    <circle cx="12" cy="11" r="1.5" />
  </>
);

const gamecube: Glyph = (
  <>
    <path d="M12 2.8 20.6 7.4v9.2L12 21.2 3.4 16.6V7.4Z" />
    <path d="M3.4 7.4 12 12l8.6-4.6M12 12v9.2" />
  </>
);

const wii: Glyph = (
  <>
    <rect x="8.8" y="2.4" width="6.4" height="19.2" rx="1.8" />
    <path d="M10.7 6.1h2.6M12 4.8v2.6" />
    <circle cx="12" cy="11.2" r="1.05" />
    <path d="M11 15.2h2M11 17.6h2" />
  </>
);

/** The Wii U is its tablet controller, screen in the middle. */
const wiiu: Glyph = (
  <>
    <rect x="1.8" y="6.2" width="20.4" height="11.6" rx="3.2" />
    <rect x="7.4" y="8.2" width="9.2" height="7.6" rx=".7" />
    <path d="M3.6 11h2.2M4.7 9.9v2.2" />
    <circle cx="19.4" cy="11" r=".8" />
  </>
);

const nswitch: Glyph = (
  <>
    <rect x="2.4" y="6.4" width="19.2" height="11.2" rx="2.6" />
    <path d="M8.2 6.4v11.2M15.8 6.4v11.2" />
    <path d="M4.3 10.6h2M5.3 9.6v2" />
    <circle cx="18.8" cy="13.4" r=".95" />
  </>
);

const gb: Glyph = (
  <>
    <rect x="6" y="2.4" width="12" height="19.2" rx="2" />
    <rect x="8.2" y="4.8" width="7.6" height="6.2" rx=".8" />
    <path d="M8.6 15.2h2.4M9.8 14v2.4" />
    <circle cx="15.2" cy="15.4" r=".95" />
  </>
);

const gbc: Glyph = (
  <>
    <rect x="6" y="2.4" width="12" height="19.2" rx="2.8" />
    <rect x="8.2" y="4.7" width="7.6" height="6.2" rx=".8" />
    <path d="M8.6 15.4h2.4M9.8 14.2v2.4" />
    <circle cx="14.4" cy="16.4" r=".85" />
    <circle cx="16.5" cy="14.6" r=".85" />
  </>
);

const gba: Glyph = (
  <>
    <rect x="1.6" y="6.2" width="20.8" height="11.6" rx="3.4" />
    <rect x="8.2" y="8.6" width="7.6" height="6.8" rx=".8" />
    <path d="M4.2 12h2.4M5.4 10.8v2.4" />
    <circle cx="18" cy="11" r=".85" />
    <circle cx="19.8" cy="13" r=".85" />
  </>
);

const nds: Glyph = (
  <>
    <rect x="4.2" y="2.6" width="15.6" height="8.2" rx="1.3" />
    <rect x="4.2" y="13.2" width="15.6" height="8.2" rx="1.3" />
    <path d="M4.2 12h15.6" />
  </>
);

const n3ds: Glyph = (
  <>
    <rect x="2.8" y="2.6" width="18.4" height="8.2" rx="1.3" />
    <rect x="5.4" y="13.2" width="13.2" height="8.2" rx="1.3" />
    <path d="M2.8 12h18.4" />
    <circle cx="7.8" cy="17.3" r=".85" />
  </>
);

const virtualboy: Glyph = (
  <>
    <rect x="2.4" y="6.6" width="19.2" height="7.4" rx="2.2" />
    <circle cx="7.8" cy="10.3" r="2.1" />
    <circle cx="16.2" cy="10.3" r="2.1" />
    <path d="M12 14v4.2M8.6 21h6.8" />
  </>
);

// ----------------------------------------------------------------- Sega

const genesis: Glyph = (
  <>
    <path d="M4 8.6h13.5a3.2 3.2 0 0 1 0 6.4H4a3.2 3.2 0 0 1 0-6.4Z" />
    <path d="M5.6 11.8h2.8M7 10.4v2.8" />
    <circle cx="13" cy="13" r=".85" />
    <circle cx="15.5" cy="12.2" r=".85" />
    <circle cx="18" cy="11.4" r=".85" />
  </>
);

const sms: Glyph = (
  <>
    <rect x="5.5" y="8.6" width="13" height="6.8" rx="1.1" />
    <path d="M7.6 12h2.6M8.9 10.7v2.6" />
    <circle cx="14.6" cy="12" r=".95" />
    <circle cx="17" cy="12" r=".95" />
  </>
);

const gamegear: Glyph = (
  <>
    <rect x="1.5" y="7" width="21" height="10" rx="3.2" />
    <rect x="8.4" y="9" width="7.2" height="6" rx=".7" />
    <path d="M4.2 12h2.4M5.4 10.8v2.4" />
    <circle cx="18.6" cy="12" r=".95" />
  </>
);

const sega32x: Glyph = (
  <>
    <rect x="5.5" y="9.5" width="13" height="11.5" rx="1.3" />
    <path d="M8.8 9.5V5.2h6.4v4.3" />
    <circle cx="12" cy="15.2" r="2.4" />
  </>
);

const saturn: Glyph = (
  <>
    <rect x="1.6" y="8.4" width="20.8" height="7.2" rx="2.4" />
    <path d="M4.2 12h2.6M5.5 10.7v2.6" />
    <circle cx="12.8" cy="10.8" r=".7" />
    <circle cx="15.2" cy="10.8" r=".7" />
    <circle cx="17.6" cy="10.8" r=".7" />
    <circle cx="12.8" cy="13.4" r=".7" />
    <circle cx="15.2" cy="13.4" r=".7" />
    <circle cx="17.6" cy="13.4" r=".7" />
  </>
);

const dreamcast: Glyph = (
  <>
    <path d="M4.4 7.8h15.2a2 2 0 0 1 2 2v2.4a4.2 4.2 0 0 1-4.2 4.2H6.6a4.2 4.2 0 0 1-4.2-4.2V9.8a2 2 0 0 1 2-2Z" />
    <rect x="9.6" y="10.6" width="4.8" height="4" rx=".6" />
    <path d="M5.2 11h2.4M6.4 9.8v2.4" />
    <circle cx="17.6" cy="11" r=".95" />
  </>
);

// ------------------------------------------------------------ PlayStation

const ps1: Glyph = (
  <>
    <path d="M7.6 8.2h8.8a5 5 0 0 1 4.9 5.9l-.3 1.6a2.4 2.4 0 0 1-4.4.7L15.2 14H8.8l-1.4 2.4a2.4 2.4 0 0 1-4.4-.7l-.3-1.6a5 5 0 0 1 4.9-5.9Z" />
    <path d="M5.6 11.4h2.4M6.8 10.2v2.4" />
    <circle cx="16.2" cy="10.4" r=".75" />
    <circle cx="18" cy="11.6" r=".75" />
    <circle cx="16.2" cy="12.8" r=".75" />
    <circle cx="14.4" cy="11.6" r=".75" />
  </>
);

const ps2: Glyph = (
  <>
    <rect x="7" y="2.5" width="10" height="19" rx="1.5" />
    <path d="M9.4 6.6h5.2M9.4 10.4h5.2" />
    <circle cx="12" cy="17" r="1.1" />
  </>
);

/** The first PlayStation 3: a slab with a rounded crown and a disc slot. */
const ps3: Glyph = (
  <>
    <path d="M2.5 16h19v-2.4c0-2.5-4.3-4.6-9.5-4.6s-9.5 2.1-9.5 4.6Z" />
    <path d="M7.5 13.2h9" />
  </>
);

/** A flat slab whose ends lean, split by a lit seam. */
const ps4: Glyph = (
  <>
    <path d="M5.4 7.5h16.1l-2.9 9H2.5Z" />
    <path d="M4 12h16" />
  </>
);

/** Stood upright: a core between two panels that flare at either end. */
const ps5: Glyph = (
  <>
    <rect x="9.8" y="4" width="4.4" height="16" rx=".8" />
    <path d="M9.8 4.4 6.2 3c1.6 2.8 2.4 5.8 2.4 9s-.8 6.2-2.4 9l3.6-1.4M14.2 4.4l3.6-1.4c-1.6 2.8-2.4 5.8-2.4 9s.8 6.2 2.4 9l-3.6-1.4" />
  </>
);

const psp: Glyph = (
  <>
    <rect x="1.5" y="7" width="21" height="10" rx="3.6" />
    <rect x="7.6" y="8.8" width="8.8" height="6.4" rx=".7" />
    <path d="M4 11h2.2M5.1 9.9v2.2" />
    <circle cx="19" cy="11" r=".8" />
    <circle cx="4.7" cy="14.4" r=".7" />
  </>
);

// ------------------------------------------------------------ Microsoft

/** A big box with a round jewel on the lid. */
const xbox: Glyph = (
  <>
    <rect x="2.5" y="7" width="19" height="10" rx="1.6" />
    <circle cx="12" cy="12" r="2.6" />
    <circle cx="12" cy="12" r=".7" />
  </>
);

/** Waisted at the sides, with the ring of lights by the power button. */
const xbox360: Glyph = (
  <>
    <path d="M3 7.5h18c-.8 1.4-1.2 2.9-1.2 4.5s.4 3.1 1.2 4.5H3c.8-1.4 1.2-2.9 1.2-4.5S3.8 8.9 3 7.5Z" />
    <circle cx="16.4" cy="12" r="1.5" />
    <path d="M6.8 10.5v3" />
  </>
);

// ---------------------------------------------------------------- Atari

const atari2600: Glyph = (
  <>
    <rect x="5" y="14.4" width="14" height="6.2" rx="1.5" />
    <path d="M12 14.4V6.6" />
    <circle cx="12" cy="4.8" r="1.9" />
    <circle cx="8.2" cy="17.5" r="1" />
  </>
);

const atari7800: Glyph = (
  <>
    <rect x="5" y="14.4" width="14" height="6.2" rx="1.5" />
    <path d="M12 14.4V6.6" />
    <circle cx="12" cy="4.8" r="1.9" />
    <circle cx="8.2" cy="17.5" r="1" />
    <circle cx="15.8" cy="17.5" r="1" />
  </>
);

const lynx: Glyph = (
  <>
    <rect x="1.5" y="6.6" width="21" height="10.8" rx="2.4" />
    <rect x="8.8" y="8.6" width="8" height="6.8" rx=".7" />
    <path d="M4.2 12h2.4M5.4 10.8v2.4" />
    <circle cx="19" cy="10.8" r=".8" />
    <circle cx="20.4" cy="13" r=".8" />
  </>
);

const jaguar: Glyph = (
  <>
    <rect x="6" y="2.6" width="12" height="18.8" rx="1.8" />
    <path d="M8.4 6.4h2.4M9.6 5.2v2.4" />
    <circle cx="15" cy="6.4" r=".9" />
    <circle cx="9.4" cy="11" r=".7" />
    <circle cx="12" cy="11" r=".7" />
    <circle cx="14.6" cy="11" r=".7" />
    <circle cx="9.4" cy="14.2" r=".7" />
    <circle cx="12" cy="14.2" r=".7" />
    <circle cx="14.6" cy="14.2" r=".7" />
    <circle cx="9.4" cy="17.4" r=".7" />
    <circle cx="12" cy="17.4" r=".7" />
    <circle cx="14.6" cy="17.4" r=".7" />
  </>
);

// ------------------------------------------------------------------ misc

const pcengine: Glyph = (
  <>
    <rect x="3.5" y="9.2" width="17" height="6.6" rx="1.2" />
    <path d="M5.8 12.5h2.6M7.1 11.2v2.6" />
    <circle cx="14.6" cy="12.5" r=".95" />
    <circle cx="17.4" cy="12.5" r=".95" />
  </>
);

const neogeo: Glyph = (
  <>
    <rect x="2.4" y="10.8" width="19.2" height="8.8" rx="1.6" />
    <path d="M7.6 10.8V7.4" />
    <circle cx="7.6" cy="5.8" r="2" />
    <circle cx="13.6" cy="14" r=".9" />
    <circle cx="16.6" cy="14" r=".9" />
    <circle cx="13.6" cy="16.8" r=".9" />
    <circle cx="16.6" cy="16.8" r=".9" />
  </>
);

const ngp: Glyph = (
  <>
    <rect x="5" y="3.4" width="14" height="17.2" rx="2" />
    <rect x="7.4" y="5.8" width="9.2" height="6.4" rx=".7" />
    <circle cx="9.6" cy="16.2" r="1.5" />
    <circle cx="15.2" cy="15.4" r=".85" />
    <circle cx="15.2" cy="17.6" r=".85" />
  </>
);

const wonderswan: Glyph = (
  <>
    <rect x="5.4" y="2.5" width="13.2" height="19" rx="2" />
    <rect x="7.6" y="6.6" width="8.8" height="7.2" rx=".7" />
    <path d="M8.2 17.6h2M9.2 16.6v2" />
    <path d="M13.8 17.6h2M14.8 16.6v2" />
  </>
);

const colecovision: Glyph = (
  <>
    <rect x="7" y="2.5" width="10" height="19" rx="1.8" />
    <circle cx="12" cy="6.4" r="1.7" />
    <circle cx="9.6" cy="11.4" r=".7" />
    <circle cx="12" cy="11.4" r=".7" />
    <circle cx="14.4" cy="11.4" r=".7" />
    <circle cx="9.6" cy="14.4" r=".7" />
    <circle cx="12" cy="14.4" r=".7" />
    <circle cx="14.4" cy="14.4" r=".7" />
    <circle cx="9.6" cy="17.4" r=".7" />
    <circle cx="12" cy="17.4" r=".7" />
    <circle cx="14.4" cy="17.4" r=".7" />
  </>
);

const intellivision: Glyph = (
  <>
    <rect x="7" y="2.5" width="10" height="19" rx="1.8" />
    <circle cx="9.6" cy="6" r=".7" />
    <circle cx="12" cy="6" r=".7" />
    <circle cx="14.4" cy="6" r=".7" />
    <circle cx="9.6" cy="9" r=".7" />
    <circle cx="12" cy="9" r=".7" />
    <circle cx="14.4" cy="9" r=".7" />
    <circle cx="12" cy="16.2" r="3.2" />
    <path d="M12 13v6.4M8.8 16.2h6.4" />
  </>
);

const c64: Glyph = (
  <>
    <path d="M2.4 18.6h19.2l-1.8-8.2H4.2Z" />
    <path d="M5.6 13.2h12.8M6.2 15.9h11.6" />
  </>
);

const amiga: Glyph = (
  <>
    <rect x="1.8" y="7.4" width="14.4" height="9.2" rx="1.4" />
    <path d="M4.4 10.4h9.2M4.4 13.2h6" />
    <rect x="18" y="11.2" width="4.2" height="6.4" rx="2.1" />
    <path d="M20.1 11.2v2.2" />
  </>
);

const dos: Glyph = (
  <>
    <path d="M4 5.5A1.5 1.5 0 0 1 5.5 4h10l4.5 4.5v10a1.5 1.5 0 0 1-1.5 1.5h-13A1.5 1.5 0 0 1 4 18.5Z" />
    <path d="M8 4v5h6V4" />
    <rect x="7.5" y="13" width="9" height="7" rx=".6" />
  </>
);

const arcade: Glyph = (
  <>
    <path d="M6 21V6a3 3 0 0 1 3-3h6a3 3 0 0 1 3 3v15" />
    <rect x="8.5" y="6" width="7" height="5" rx=".6" />
    <path d="M8.5 14.5h7M6 21h12" />
    <circle cx="10.2" cy="17.6" r=".8" />
    <circle cx="13.8" cy="17.6" r=".8" />
  </>
);

// ----------------------------------------------------------- fall-backs

const FAMILY_GLYPH: Record<Family, Glyph> = {
  cartridge: (
    <>
      <path d="M6 21V5a2 2 0 0 1 2-2h5.6a2 2 0 0 1 1.4.6l2.4 2.4a2 2 0 0 1 .6 1.4V21" />
      <rect x="9" y="6" width="6" height="4" rx=".6" />
      <path d="M9 21v-2.5M12 21v-2.5M15 21v-2.5" />
    </>
  ),
  disc: (
    <>
      <circle cx="12" cy="12" r="8.5" />
      <circle cx="12" cy="12" r="2.4" />
      <path d="M12 3.5a8.5 8.5 0 0 1 7.4 4.3" />
    </>
  ),
  handheld: (
    <>
      <rect x="6" y="2.5" width="12" height="19" rx="2.2" />
      <rect x="8.5" y="5" width="7" height="6" rx=".6" />
      <path d="M9 15.5h2.4M10.2 14.3v2.4" />
      <circle cx="15" cy="15.5" r=".9" />
    </>
  ),
  computer: (
    <>
      <rect x="3" y="4" width="18" height="11" rx="1.6" />
      <path d="M9 19h6M12 15v4" />
      <path d="M6.5 7.5h5" />
    </>
  ),
  chip: (
    <>
      <rect x="6.5" y="6.5" width="11" height="11" rx="1.6" />
      <path d="M10 3.5v3M14 3.5v3M10 17.5v3M14 17.5v3M3.5 10h3M3.5 14h3M17.5 10h3M17.5 14h3" />
    </>
  ),
};

/** Sidebar entries that are not systems at all. */
const PSEUDO: Record<string, Glyph> = {
  all: (
    <>
      <rect x="3.5" y="3.5" width="7" height="7" rx="1.2" />
      <rect x="13.5" y="3.5" width="7" height="7" rx="1.2" />
      <rect x="3.5" y="13.5" width="7" height="7" rx="1.2" />
      <rect x="13.5" y="13.5" width="7" height="7" rx="1.2" />
    </>
  ),
  star: (
    <path d="m12 3.6 2.6 5.3 5.8.85-4.2 4.1 1 5.8-5.2-2.73L6.8 19.65l1-5.8-4.2-4.1 5.8-.85Z" />
  ),
  missing: (
    <>
      <rect x="3.5" y="5" width="17" height="14" rx="2" />
      <path d="M8.6 12h6.8" />
    </>
  ),
};

const GLYPHS: Record<string, Glyph> = {
  nes,
  snes,
  n64,
  gamecube,
  wii,
  wiiu,
  switch: nswitch,
  gb,
  gbc,
  gba,
  nds,
  n3ds,
  virtualboy,
  genesis,
  sms,
  gamegear,
  sega32x,
  saturn,
  dreamcast,
  ps1,
  ps2,
  ps3,
  ps4,
  ps5,
  psp,
  xbox,
  xbox360,
  atari2600,
  atari7800,
  lynx,
  jaguar,
  pcengine,
  neogeo,
  ngp,
  wonderswan,
  colecovision,
  intellivision,
  c64,
  amiga,
  dos,
  arcade,
};

export function glyphFor(platform: string): Glyph {
  return (
    PSEUDO[platform] ??
    GLYPHS[platform] ??
    FAMILY_GLYPH[FAMILY[platform] ?? "chip"]
  );
}

interface Props {
  /** A platform slug, or one of the sidebar pseudo-icons. */
  platform: string;
  size?: number;
  className?: string;
}

export default function SystemIcon({ platform, size = 16, className }: Props) {
  return (
    <svg
      className={className}
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={1.5}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
      focusable="false"
    >
      {glyphFor(platform)}
    </svg>
  );
}
