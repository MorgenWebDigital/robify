import type { SVGProps } from "react";

// the coins as their own symbols.
//
// deliberately apart from `Icons.tsx`: everything there is drawn in one
// stroke on `currentColor` and takes the colour of its surroundings. a coin
// lives on its colour instead — orange is bitcoin, green is tether, and drawn
// in grey none of them is recognised any more.
//
// the shapes are our own, simplified for a circle of six and twenty pixels.
// no logo file is copied along, and nothing is fetched from the net.

type MuenzProps = SVGProps<SVGSVGElement> & { size?: number };

/** brand colour and glyph of one coin. */
interface Muenzbild {
  farbe: string;
  glyphe: React.ReactNode;
}

// white on the coloured disc, in one stroke. the round caps keep the letters
// legible where the whole coin is thirteen pixels wide, as it is for a token
// beside the name.
const strich = {
  stroke: "#fff",
  strokeWidth: 1.9,
  strokeLinecap: "round" as const,
  strokeLinejoin: "round" as const,
  fill: "none",
};

// the b with the two upright strokes through it, bitcoin and bitcoin cash
// alike: the two carry the same sign and differ only in colour.
const zeichenB = (
  <g {...strich}>
    <path d="M9.7 5.9v12.2" />
    <path d="M9.7 8.2h3.4c1.3 0 2.3.9 2.3 2s-1 2-2.3 2H9.7" />
    <path d="M9.7 12.2h3.9c1.4 0 2.5.9 2.5 2s-1.1 2-2.5 2H9.7" />
    <path d="M11.9 4.4v1.6M14.2 4.8v3.4M11.9 18v1.6M14.2 16.2v3.2" />
  </g>
);

const MUENZEN: Record<string, Muenzbild> = {
  Bitcoin: { farbe: "#f7931a", glyphe: zeichenB },
  "Bitcoin Cash": { farbe: "#0ac18e", glyphe: zeichenB },

  // the octahedron seen from the side: the upper body light, the lower one
  // darker, as the sign has always been drawn
  Ethereum: {
    farbe: "#627eea",
    glyphe: (
      <g fill="#fff">
        <path d="M12 3.2 6.4 12.1 12 15.3l5.6-3.2z" fillOpacity="0.85" />
        <path d="M12 16.5 6.4 13.3 12 20.8l5.6-7.5z" fillOpacity="0.55" />
      </g>
    ),
  },

  // the m of two upright bars with the valley between them
  Monero: {
    farbe: "#ff6600",
    glyphe: (
      <g {...strich} strokeWidth={2.1}>
        <path d="M4.6 16.4V7.8L12 15.2l7.4-7.4v8.6" />
      </g>
    ),
  },

  // three bars, the middle one leaning the other way
  Solana: {
    farbe: "#1b1b28",
    glyphe: (
      <g fill="url(#muenze-solana)">
        <path d="M7 5.6h12.4l-2.4 2.5H4.6z" />
        <path d="M4.6 10.8H17l2.4 2.5H7z" />
        <path d="M7 16h12.4L17 18.5H4.6z" />
        <defs>
          <linearGradient id="muenze-solana" x1="4" y1="19" x2="20" y2="5">
            <stop offset="0" stopColor="#9945ff" />
            <stop offset="1" stopColor="#14f195" />
          </linearGradient>
        </defs>
      </g>
    ),
  },

  // the l with the stroke through it
  Litecoin: {
    farbe: "#345d9d",
    glyphe: (
      <g {...strich} strokeWidth={2.1}>
        <path d="M13.9 5.2 11.2 15.4h6.6" />
        <path d="m7.6 12.9 5.4-1.9" />
      </g>
    ),
  },

  // the folded triangle
  Tron: {
    farbe: "#ff060a",
    glyphe: (
      <g {...strich} strokeWidth={1.7}>
        <path d="M4.6 6.5 19.4 9.6 10.9 19.6z" />
        <path d="M4.6 6.5 13.6 11.4 10.9 19.6" />
      </g>
    ),
  },

  // the t with the bar and the ring around its stem
  USDT: {
    farbe: "#26a17b",
    glyphe: (
      <g {...strich} strokeWidth={1.8}>
        <path d="M6.4 7.2h11.2" />
        <path d="M12 7.4v10.4" />
        <ellipse cx="12" cy="10.8" rx="5.3" ry="2.1" />
      </g>
    ),
  },

  // the dollar sign in its ring
  USDC: {
    farbe: "#2775ca",
    glyphe: (
      <g {...strich} strokeWidth={1.8}>
        <circle cx="12" cy="12" r="7.6" strokeWidth={1.5} />
        <path d="M14.6 9.6c0-1.2-1.2-2-2.6-2s-2.6.8-2.6 2 1.1 1.8 2.6 2.2 2.6.9 2.6 2.2-1.2 2-2.6 2-2.6-.8-2.6-2" />
        <path d="M12 6.2v11.6" />
      </g>
    ),
  },
};

/**
 * the symbol of one coin, by the name it carries in the list.
 *
 * an unknown name does not break the row: what appears then is a grey disc
 * with the first letter on it, and a further currency can be entered without
 * a drawing having to exist for it first.
 */
export function Muenze({
  name,
  size = 26,
  ...props
}: MuenzProps & { name: string }) {
  const bild = MUENZEN[name];
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      role="img"
      aria-label={name}
      {...props}
    >
      <circle cx="12" cy="12" r="12" fill={bild ? bild.farbe : "#5a5a63"} />
      {bild ? (
        bild.glyphe
      ) : (
        <text
          x="12"
          y="16.5"
          textAnchor="middle"
          fontSize="12"
          fontWeight="600"
          fill="#fff"
        >
          {name.slice(0, 1)}
        </text>
      )}
    </svg>
  );
}
