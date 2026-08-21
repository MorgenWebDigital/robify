import type { SVGProps } from "react";

type IconProps = SVGProps<SVGSVGElement> & { size?: number };

/** the shared basis: a 24 unit grid, stroke width 1.8, currentcolor */
function Icon({ size = 20, children, ...props }: IconProps) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={1.8}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
      {...props}
    >
      {children}
    </svg>
  );
}

export const PlayIcon = (p: IconProps) => (
  <Icon {...p}>
    <path d="M7 4.5v15l12-7.5z" fill="currentColor" stroke="none" />
  </Icon>
);

export const PauseIcon = (p: IconProps) => (
  <Icon {...p}>
    <rect
      x="6.5"
      y="4.5"
      width="4"
      height="15"
      rx="1.2"
      fill="currentColor"
      stroke="none"
    />
    <rect
      x="13.5"
      y="4.5"
      width="4"
      height="15"
      rx="1.2"
      fill="currentColor"
      stroke="none"
    />
  </Icon>
);

export const PrevIcon = (p: IconProps) => (
  <Icon {...p}>
    <path d="M18 5.5v13L8.5 12z" fill="currentColor" stroke="none" />
    <rect
      x="5"
      y="5.5"
      width="2.2"
      height="13"
      rx="1"
      fill="currentColor"
      stroke="none"
    />
  </Icon>
);

export const NextIcon = (p: IconProps) => (
  <Icon {...p}>
    <path d="M6 5.5v13L15.5 12z" fill="currentColor" stroke="none" />
    <rect
      x="16.8"
      y="5.5"
      width="2.2"
      height="13"
      rx="1"
      fill="currentColor"
      stroke="none"
    />
  </Icon>
);

export const ShuffleIcon = (p: IconProps) => (
  <Icon {...p}>
    <path d="M16 3h5v5" />
    <path d="M4 20 21 3" />
    <path d="M21 16v5h-5" />
    <path d="M15 15l6 6" />
    <path d="M4 4l5 5" />
  </Icon>
);

export const RepeatIcon = (p: IconProps) => (
  <Icon {...p}>
    <path d="M17 2l4 4-4 4" />
    <path d="M3 11v-1a4 4 0 0 1 4-4h14" />
    <path d="M7 22l-4-4 4-4" />
    <path d="M21 13v1a4 4 0 0 1-4 4H3" />
  </Icon>
);

export const RepeatOneIcon = (p: IconProps) => (
  <Icon {...p}>
    <path d="M17 2l4 4-4 4" />
    <path d="M3 11v-1a4 4 0 0 1 4-4h14" />
    <path d="M7 22l-4-4 4-4" />
    <path d="M21 13v1a4 4 0 0 1-4 4H3" />
    <path d="M11 15v-4l-1.4 1" />
  </Icon>
);

export const VolumeIcon = (p: IconProps) => (
  <Icon {...p}>
    <path d="M11 5 6.5 9H3v6h3.5L11 19z" />
    <path d="M15.5 8.5a5 5 0 0 1 0 7" />
    <path d="M18.5 5.5a9 9 0 0 1 0 13" />
  </Icon>
);

export const MuteIcon = (p: IconProps) => (
  <Icon {...p}>
    <path d="M11 5 6.5 9H3v6h3.5L11 19z" />
    <path d="m16 9 5 6" />
    <path d="m21 9-5 6" />
  </Icon>
);

export const MoonIcon = (p: IconProps) => (
  <Icon {...p}>
    <path d="M20 14.5A8.5 8.5 0 1 1 9.5 4a6.8 6.8 0 0 0 10.5 10.5z" />
  </Icon>
);

export const HeartIcon = ({
  filled,
  ...p
}: IconProps & { filled?: boolean }) => (
  <Icon {...p}>
    <path
      d="M12 20.5S3.5 15.4 3.5 9.6A4.6 4.6 0 0 1 12 7.2a4.6 4.6 0 0 1 8.5 2.4c0 5.8-8.5 10.9-8.5 10.9z"
      fill={filled ? "currentColor" : "none"}
    />
  </Icon>
);

export const PlusIcon = (p: IconProps) => (
  <Icon {...p}>
    <path d="M12 5v14M5 12h14" />
  </Icon>
);

export const SearchIcon = (p: IconProps) => (
  <Icon {...p}>
    <circle cx="11" cy="11" r="6.5" />
    <path d="m20 20-3.6-3.6" />
  </Icon>
);

export const HomeIcon = (p: IconProps) => (
  <Icon {...p}>
    <path d="M4 10.5 12 4l8 6.5V20a1 1 0 0 1-1 1h-4v-6H9v6H5a1 1 0 0 1-1-1z" />
  </Icon>
);

export const LibraryIcon = (p: IconProps) => (
  <Icon {...p}>
    <path d="M4 4v16M9 4v16" />
    <path d="m14.5 5 4.7 1.3-3.6 13.4-4.7-1.3z" />
  </Icon>
);

export const ArtistIcon = (p: IconProps) => (
  <Icon {...p}>
    <circle cx="12" cy="8" r="3.6" />
    <path d="M5 20a7 7 0 0 1 14 0" />
  </Icon>
);

export const PlaylistIcon = (p: IconProps) => (
  <Icon {...p}>
    <path d="M4 6h11M4 11h11M4 16h7" />
    <circle cx="17.5" cy="16.5" r="2.5" />
    <path d="M20 16.5V9l-2.5.8" />
  </Icon>
);

export const DownloadIcon = (p: IconProps) => (
  <Icon {...p}>
    <path d="M12 3v11" />
    <path d="m7.5 10 4.5 4.5L16.5 10" />
    <path d="M4.5 19.5h15" />
  </Icon>
);

export const SparkIcon = (p: IconProps) => (
  <Icon {...p}>
    <path d="M12 3.5 13.8 9l5.7 1.8-5.7 1.8L12 18.2l-1.8-5.6L4.5 10.8 10.2 9z" />
    <path d="M18.5 3.5 19.2 5.6 21.5 6.3 19.2 7 18.5 9.2 17.8 7 15.5 6.3 17.8 5.6z" />
  </Icon>
);

export const SettingsIcon = (p: IconProps) => (
  <Icon {...p}>
    <circle cx="12" cy="12" r="3.2" />
    <path d="M19.4 14a1.5 1.5 0 0 0 .3 1.7l.1.1a2 2 0 1 1-2.8 2.8l-.1-.1a1.5 1.5 0 0 0-2.5 1V20a2 2 0 1 1-4 0v-.2a1.5 1.5 0 0 0-2.5-1l-.1.1a2 2 0 1 1-2.8-2.8l.1-.1A1.5 1.5 0 0 0 4 14a2 2 0 1 1 0-4 1.5 1.5 0 0 0 1.1-2.5l-.1-.1a2 2 0 1 1 2.8-2.8l.1.1A1.5 1.5 0 0 0 10.5 4 2 2 0 1 1 14.5 4a1.5 1.5 0 0 0 2.5 1l.1-.1a2 2 0 1 1 2.8 2.8l-.1.1a1.5 1.5 0 0 0 1 2.5 2 2 0 1 1 0 4 1.5 1.5 0 0 0-1.4.7z" />
  </Icon>
);

export const QueueIcon = (p: IconProps) => (
  <Icon {...p}>
    <path d="M4 7h12M4 12h12M4 17h8" />
    <path d="M17.5 12v7l4.5-3.5z" fill="currentColor" stroke="none" />
  </Icon>
);

export const LyricsIcon = (p: IconProps) => (
  <Icon {...p}>
    <path d="M5 5h14v10H9l-4 4z" />
    <path d="M9 9h6M9 12h3" />
  </Icon>
);

export const TrashIcon = (p: IconProps) => (
  <Icon {...p}>
    <path d="M4.5 7h15" />
    <path d="M9.5 7V5.5A1.5 1.5 0 0 1 11 4h2a1.5 1.5 0 0 1 1.5 1.5V7" />
    <path d="M6.5 7 7.4 19a1.5 1.5 0 0 0 1.5 1.4h6.2a1.5 1.5 0 0 0 1.5-1.4L17.5 7" />
  </Icon>
);

export const PencilIcon = (p: IconProps) => (
  <Icon {...p}>
    <path d="M4 20.5h4L19 9.5a2.1 2.1 0 0 0-3-3L5 17.5z" />
    <path d="m14.5 5 4 4" />
  </Icon>
);

export const CloseIcon = (p: IconProps) => (
  <Icon {...p}>
    <path d="m6 6 12 12M18 6 6 18" />
  </Icon>
);

export const ChevronDownIcon = (p: IconProps) => (
  <Icon {...p}>
    <path d="m6 9 6 6 6-6" />
  </Icon>
);

export const ChevronLeftIcon = (p: IconProps) => (
  <Icon {...p}>
    <path d="m14.5 5-7 7 7 7" />
  </Icon>
);

export const ChevronRightIcon = (p: IconProps) => (
  <Icon {...p}>
    <path d="m9.5 5 7 7-7 7" />
  </Icon>
);

export const DotsIcon = (p: IconProps) => (
  <Icon {...p}>
    <circle cx="5.5" cy="12" r="1.4" fill="currentColor" stroke="none" />
    <circle cx="12" cy="12" r="1.4" fill="currentColor" stroke="none" />
    <circle cx="18.5" cy="12" r="1.4" fill="currentColor" stroke="none" />
  </Icon>
);

export const FolderIcon = (p: IconProps) => (
  <Icon {...p}>
    <path d="M3.5 6.5h5l2 2.5h10v9a1.5 1.5 0 0 1-1.5 1.5h-14A1.5 1.5 0 0 1 3.5 18z" />
  </Icon>
);

export const RefreshIcon = (p: IconProps) => (
  <Icon {...p}>
    <path d="M20 11a8 8 0 1 0-.6 4" />
    <path d="M20 5v6h-6" />
  </Icon>
);

export const CopyIcon = (p: IconProps) => (
  <Icon {...p}>
    <rect x="9" y="9" width="11" height="11" rx="2.5" />
    <path d="M15 5.5A2.5 2.5 0 0 0 12.5 3h-6A2.5 2.5 0 0 0 4 5.5v6A2.5 2.5 0 0 0 6.5 14" />
  </Icon>
);

export const CheckIcon = (p: IconProps) => (
  <Icon {...p}>
    <path d="m5 12.5 4.5 4.5L19 7.5" />
  </Icon>
);

export const DiscIcon = (p: IconProps) => (
  <Icon {...p}>
    <circle cx="12" cy="12" r="8.5" />
    <circle cx="12" cy="12" r="2.2" />
  </Icon>
);

export const GridIcon = (p: IconProps) => (
  <Icon {...p}>
    <rect x="4" y="4" width="7" height="7" rx="1.5" />
    <rect x="13" y="4" width="7" height="7" rx="1.5" />
    <rect x="4" y="13" width="7" height="7" rx="1.5" />
    <rect x="13" y="13" width="7" height="7" rx="1.5" />
  </Icon>
);

export const ListIcon = (p: IconProps) => (
  <Icon {...p}>
    <path d="M4 6.5h2M4 12h2M4 17.5h2" />
    <path d="M9.5 6.5H20M9.5 12H20M9.5 17.5H20" />
  </Icon>
);

/**
 * three squares of growing edge length. set side by side the row reads as a
 * size scale without any letters being needed. deliberately no grid as on the
 * tile icon, otherwise view and size get mixed up.
 */
export const SizeSmallIcon = (p: IconProps) => (
  <Icon {...p}>
    <rect x="8.5" y="8.5" width="7" height="7" rx="1.5" />
  </Icon>
);

export const SizeMediumIcon = (p: IconProps) => (
  <Icon {...p}>
    <rect x="6.5" y="6.5" width="11" height="11" rx="2" />
  </Icon>
);

export const SizeLargeIcon = (p: IconProps) => (
  <Icon {...p}>
    <rect x="4" y="4" width="16" height="16" rx="2.5" />
  </Icon>
);

/**
 * the mark of the app: two notes under a shared beam.
 *
 * the transform sets the drawing centred into the field and scales it up so
 * it fills it, otherwise it would stand small in a corner. it takes the text
 * colour, which in the sidebar is the accent.
 */
export const NotesMark = ({ size = 20, ...p }: IconProps) => (
  <svg
    width={size}
    height={size}
    viewBox="0 0 512 512"
    fill="currentColor"
    aria-hidden="true"
    {...p}
  >
    <g transform="translate(256 256) scale(1.45) translate(-204 -288)">
      <path d="M150 214 L338 166 v52 L150 266 z" />
      <rect x="150" y="214" width="26" height="150" />
      {/* 172.64 instead of 166: the top edge of the beam at this point. set
          higher, the top left corner of the stem looked out over the beam and
          formed a step. has to match `icons/robify-logo.svg`, the program
          icons grow out of that. */}
      <rect x="312" y="172.64" width="26" height="143.36" />
      <ellipse
        cx="120"
        cy="366"
        rx="52"
        ry="40"
        transform="rotate(-20 120 366)"
      />
      <ellipse
        cx="282"
        cy="318"
        rx="52"
        ry="40"
        transform="rotate(-20 282 318)"
      />
    </g>
  </svg>
);

/** small animated bars for the track currently running */
export function PlayingBars({ className = "" }: { className?: string }) {
  return (
    <span
      className={`flex h-3.5 items-end gap-[2px] ${className}`}
      aria-hidden="true"
    >
      {[0, 0.2, 0.4].map((delay) => (
        <span
          key={delay}
          className="w-[3px] origin-bottom rounded-full"
          style={{
            height: "100%",
            background: "var(--accent)",
            animation: `bar 900ms ${delay}s ease-in-out infinite`,
          }}
        />
      ))}
    </span>
  );
}
