import type { SkinName } from "../../skins/shell";

/**
 * A wireframe of each layout, so the choice can be made by eye rather than by
 * reading six descriptions and guessing. Drawn rather than screenshotted — a
 * thumbnail stays right when the skin changes.
 */
export default function SkinPreview({ skin }: { skin: SkinName }) {
  const box = (x: number, y: number, w: number, h: number, fill: string, rx = 1.5) => (
    <rect key={`${x}-${y}-${w}-${h}`} x={x} y={y} width={w} height={h} rx={rx} fill={fill} />
  );

  return (
    <svg className="skin-thumb" viewBox="0 0 100 60" role="img" aria-hidden="true">
      {skin === "launchbox" && (
        <>
          {box(0, 0, 100, 60, "#151920", 3)}
          {box(4, 4, 22, 52, "#1b2028", 2)}
          {box(30, 4, 66, 7, "#1b2028")}
          {[0, 1, 2, 3].map((i) => box(30 + (i % 4) * 17, 15, 14, 16, "#2b3441"))}
          {[0, 1, 2, 3].map((i) => box(30 + (i % 4) * 17, 34, 14, 16, "#2b3441"))}
        </>
      )}

      {skin === "switch" && (
        <>
          {box(0, 0, 100, 60, "#2a2a2e", 3)}
          {box(4, 4, 92, 6, "#3a3a40")}
          {box(6, 16, 12, 3, "#a0a0a8", 1)}
          {[0, 1, 2, 3].map((i) => box(6 + i * 22, 24, 18, 18, "#3a3a40", 2))}
          <rect
            x={27}
            y={22}
            width={22}
            height={22}
            rx={2.5}
            fill="#3a3a40"
            stroke="#00c3e3"
            strokeWidth={2}
          />
          {box(4, 50, 92, 6, "#3a3a40")}
        </>
      )}

      {skin === "steam" && (
        <>
          {box(0, 0, 100, 60, "#10161f", 3)}
          {box(0, 0, 24, 60, "#0a0f16", 3)}
          {box(24, 0, 76, 26, "#22405f")}
          {box(29, 17, 20, 5, "#4c9be8", 1.5)}
          {[0, 1, 2, 3].map((i) => box(29 + i * 17, 32, 13, 20, "#182231"))}
          {box(24, 55, 76, 5, "#0d131b", 0)}
        </>
      )}

      {/* One column of slabs, the selected one stepped toward the light. */}
      {skin === "arc" && (
        <>
          {box(0, 0, 100, 60, "#06090a", 3)}
          {box(0, 0, 10, 60, "#0f1a10", 3)}
          {[0, 1, 3, 4].map((i) => box(30, 6 + i * 10, 40, 7, "#151d17"))}
          {box(26, 26, 48, 8, "#7ed321", 1.5)}
        </>
      )}

      {/* A light 4x3 page of channels, the tray along the bottom. */}
      {skin === "wii" && (
        <>
          {box(0, 0, 100, 60, "#eff3f5", 3)}
          {[0, 1, 2].map((row) =>
            [0, 1, 2, 3].map((col) => (
              <rect
                key={`${row}-${col}`}
                x={8 + col * 22}
                y={5 + row * 13}
                width={18}
                height={10}
                rx={2}
                fill="#ffffff"
                stroke={row === 0 && col === 1 ? "#00a8e0" : "#b6bfc5"}
                strokeWidth={row === 0 && col === 1 ? 1.4 : 0.6}
              />
            )),
          )}
          {box(0, 46, 100, 14, "#dfe4e7", 3)}
          {box(43, 50, 14, 5, "#c6d0d7", 2.5)}
        </>
      )}

      {/* A column of cards standing in a dark, quiet field. */}
      {skin === "browser" && (
        <>
          {box(0, 0, 100, 60, "#040a1c", 3)}
          {[12, 40, 70, 84].map((x, i) => (
            <circle key={x} cx={x} cy={8 + i * 13} r={0.8} fill="#7c8cb8" />
          ))}
          {box(38, 6, 24, 14, "#0a1533", 2)}
          <rect x={36} y={23} width={28} height={16} rx={2} fill="#0a1533" stroke="#dce6ff" strokeWidth={0.8} />
          {box(38, 42, 24, 14, "#0a1533", 2)}
        </>
      )}
    </svg>
  );
}
