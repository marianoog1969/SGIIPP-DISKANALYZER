const base = {
  width: 16,
  height: 16,
  viewBox: "0 0 16 16",
  fill: "none",
  stroke: "currentColor",
  strokeWidth: 1.4,
  strokeLinecap: "round",
  strokeLinejoin: "round",
} as const;

type P = { className?: string };

export const Icon = {
  folder: ({ className }: P) => (
    <svg {...base} className={className}>
      <path
        d="M1.5 4.5a1 1 0 0 1 1-1h3.2l1.4 1.5h6.4a1 1 0 0 1 1 1v6a1 1 0 0 1-1 1h-11a1 1 0 0 1-1-1z"
        fill="currentColor"
        fillOpacity=".22"
      />
    </svg>
  ),
  file: ({ className }: P) => (
    <svg {...base} className={className}>
      <path d="M4 1.8h5l3 3v9a.8.8 0 0 1-.8.8H4a.8.8 0 0 1-.8-.8V2.6A.8.8 0 0 1 4 1.8z" />
      <path d="M9 1.8v3h3" />
    </svg>
  ),
  reveal: ({ className }: P) => (
    <svg {...base} className={className}>
      <path d="M6.5 3.5H3.8a1 1 0 0 0-1 1v7.7a1 1 0 0 0 1 1h7.7a1 1 0 0 0 1-1V9.5" />
      <path d="M9.5 2.5h4v4M13.5 2.5 7.5 8.5" />
    </svg>
  ),
  copy: ({ className }: P) => (
    <svg {...base} className={className}>
      <rect x="5.5" y="5.5" width="8" height="8" rx="1.2" />
      <path d="M10.5 5.5V3.7a1 1 0 0 0-1-1H3.7a1 1 0 0 0-1 1v5.8a1 1 0 0 0 1 1h1.8" />
    </svg>
  ),
  up: ({ className }: P) => (
    <svg {...base} className={className}>
      <path d="M8 13V3M3.5 7.5 8 3l4.5 4.5" />
    </svg>
  ),
  refresh: ({ className }: P) => (
    <svg {...base} className={className}>
      <path d="M13.2 8A5.2 5.2 0 1 1 11.6 4.3" />
      <path d="M13.4 2.2v3.1h-3.1" />
    </svg>
  ),
  drive: ({ className }: P) => (
    <svg {...base} className={className}>
      <rect x="1.8" y="8" width="12.4" height="5" rx="1.2" />
      <path d="M3 8 4.6 3.6a1 1 0 0 1 .94-.65h4.92a1 1 0 0 1 .94.65L13 8" />
      <circle cx="11.5" cy="10.5" r=".6" fill="currentColor" />
    </svg>
  ),
  search: ({ className }: P) => (
    <svg {...base} className={className}>
      <circle cx="7" cy="7" r="4.3" />
      <path d="M10.3 10.3L14 14" />
    </svg>
  ),
  trash: ({ className }: P) => (
    <svg {...base} className={className}>
      <path d="M2.5 4.2h11M6 4.2V2.6h4v1.6M4 4.2l.6 9h6.8l.6-9M6.6 6.6v4.4M9.4 6.6v4.4" />
    </svg>
  ),
  export: ({ className }: P) => (
    <svg {...base} className={className}>
      <path d="M8 2v7.4M5.2 6.8 8 9.6l2.8-2.8M2.8 10.5v2.2a1 1 0 0 0 1 1h8.4a1 1 0 0 0 1-1v-2.2" />
    </svg>
  ),
  gear: ({ className }: P) => (
    <svg {...base} className={className}>
      <circle cx="8" cy="8" r="2.1" />
      <path d="M8 1.8v1.7M8 12.5v1.7M1.8 8h1.7M12.5 8h1.7M3.6 3.6l1.2 1.2M11.2 11.2l1.2 1.2M12.4 3.6l-1.2 1.2M4.8 11.2l-1.2 1.2" />
    </svg>
  ),
  help: ({ className }: P) => (
    <svg {...base} className={className}>
      <circle cx="8" cy="8" r="6.2" />
      <path d="M6.2 6.3a1.9 1.9 0 1 1 2.7 1.7c-.6.3-.9.7-.9 1.3M8 11.5v.1" />
    </svg>
  ),
  sun: ({ className }: P) => (
    <svg {...base} className={className}>
      <circle cx="8" cy="8" r="2.6" />
      <path d="M8 1.5v1.6M8 12.9v1.6M1.5 8h1.6M12.9 8h1.6M3.4 3.4l1.1 1.1M11.5 11.5l1.1 1.1M12.6 3.4l-1.1 1.1M4.5 11.5l-1.1 1.1" />
    </svg>
  ),
  moon: ({ className }: P) => (
    <svg {...base} className={className}>
      <path d="M13 9.4A5.4 5.4 0 0 1 6.6 3 5.4 5.4 0 1 0 13 9.4z" />
    </svg>
  ),
  half: ({ className }: P) => (
    <svg {...base} className={className}>
      <circle cx="8" cy="8" r="5.8" />
      <path d="M8 2.2v11.6A5.8 5.8 0 0 0 8 2.2z" fill="currentColor" />
    </svg>
  ),
};
