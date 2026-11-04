import type { ReactNode } from "react";

// Ícones de traço simples, no estilo do Pipo (compartilhados pelas janelas).
const icon = (d: ReactNode, size = 18) => (
  <svg
    viewBox="0 0 24 24"
    width={size}
    height={size}
    fill="none"
    stroke="currentColor"
    strokeWidth="1.8"
    strokeLinecap="round"
    strokeLinejoin="round"
    aria-hidden="true"
  >
    {d}
  </svg>
);

export const ICONS = {
  chat: icon(<path d="M4 5h16v11H9l-5 4z" />),
  today: icon(
    <>
      <circle cx="12" cy="12" r="4" />
      <path d="M12 2v2M12 20v2M2 12h2M20 12h2M4.9 4.9l1.4 1.4M17.7 17.7l1.4 1.4M4.9 19.1l1.4-1.4M17.7 6.3l1.4-1.4" />
    </>,
  ),
  actions: icon(<path d="M13 2 4 14h7l-1 8 9-12h-7z" />),
  clipboard: icon(
    <>
      <rect x="6" y="4" width="12" height="17" rx="2" />
      <path d="M9 4h6v3H9z" />
    </>,
  ),
  system: icon(<path d="M3 12h4l2-6 4 12 2-6h6" />),
  usage: icon(
    <>
      <circle cx="12" cy="12" r="9" />
      <path d="M14.5 9.5c-.5-1-1.5-1.5-2.5-1.5-1.5 0-2.5.8-2.5 2s1 1.7 2.5 2 2.5.8 2.5 2-1 2-2.5 2c-1 0-2-.5-2.5-1.5M12 6v2M12 16v2" />
    </>,
  ),
  settings: icon(
    <>
      <circle cx="12" cy="12" r="3" />
      <path d="M12 2v3M12 19v3M2 12h3M19 12h3M4.9 4.9l2.1 2.1M17 17l2.1 2.1M4.9 19.1 7 17M17 7l2.1-2.1" />
    </>,
  ),
  spark: icon(<path d="M12 3v4M12 17v4M3 12h4M17 12h4M6 6l2.5 2.5M15.5 15.5 18 18M6 18l2.5-2.5M15.5 8.5 18 6" />),
  hide: icon(
    <>
      <path d="M3 12s3.5-6 9-6 9 6 9 6-3.5 6-9 6-9-6-9-6z" />
      <path d="M4 20 20 4" />
    </>,
  ),
  mic: icon(
    <>
      <rect x="9" y="3" width="6" height="11" rx="3" />
      <path d="M5 11a7 7 0 0 0 14 0M12 18v3" />
    </>,
  ),
  stop: icon(<rect x="6" y="6" width="12" height="12" rx="2" />),
  star: icon(<path d="m12 3 2.7 5.6 6.1.9-4.4 4.3 1 6.1L12 17l-5.4 2.9 1-6.1-4.4-4.3 6.1-.9z" />),
  power: icon(
    <>
      <path d="M12 3v8" />
      <path d="M6.3 7.5a8 8 0 1 0 11.4 0" />
    </>,
  ),
  close: icon(<path d="M6 6l12 12M18 6 6 18" />, 14),
  clock: icon(
    <>
      <circle cx="12" cy="13" r="8" />
      <path d="M12 9v4l2.5 2.5M9 2h6" />
    </>,
  ),
  search: icon(
    <>
      <circle cx="11" cy="11" r="6.5" />
      <path d="m16 16 4.5 4.5" />
    </>,
  ),
  flame: icon(<path d="M12 21c-3.9 0-6.5-2.6-6.5-6.2 0-3.4 2.6-5.6 3.8-8.8.4 2 1.6 3.2 2.7 3.6C12 6.8 13.5 4.4 15.5 3c-.3 2.7.9 4.6 2 6.2.9 1.4 1.5 2.8 1.5 4.6 0 4.2-3 7.2-7 7.2z" />),
  calendar: icon(
    <>
      <rect x="4" y="5" width="16" height="16" rx="2" />
      <path d="M4 10h16M9 3v4M15 3v4" />
    </>,
  ),
  lock: icon(
    <>
      <rect x="5" y="11" width="14" height="10" rx="2" />
      <path d="M8 11V8a4 4 0 0 1 8 0v3" />
    </>,
  ),
  island: icon(
    <>
      <rect x="3" y="4" width="18" height="16" rx="3" />
      <rect x="8.5" y="4" width="7" height="3.5" rx="1.75" fill="currentColor" stroke="none" />
    </>,
  ),
  expand: icon(<path d="M4 9V4h5M20 9V4h-5M4 15v5h5M20 15v5h-5" />, 16),
  collapse: icon(<path d="M9 4v5H4M15 4v5h5M9 20v-5H4M15 20v-5h5" />, 16),
};
