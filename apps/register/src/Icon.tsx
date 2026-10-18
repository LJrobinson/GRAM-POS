const paths: Record<string, React.ReactNode> = {
  register: <><rect x="3" y="4" width="18" height="12" rx="2"/><path d="M8 21h8M12 16v5M7 8h10M7 12h4"/></>,
  history: <><circle cx="12" cy="12" r="8"/><path d="M12 7v5l3 2M4 4v5H0"/></>,
  settings: <><path d="m10 3-1 3-3 1-3-1-2 4 3 2v3l-1 3 4 2 2-2h3l3 1 2-4-2-2v-3l1-3-4-2-2 1Z" transform="translate(2 0) scale(.9)"/><circle cx="12" cy="12" r="3"/></>,
  search: <><circle cx="10.5" cy="10.5" r="6.5"/><path d="m16 16 5 5"/></>,
  plus: <path d="M12 5v14M5 12h14"/>, minus: <path d="M5 12h14"/>, close: <path d="m6 6 12 12M18 6 6 18"/>,
  arrow: <path d="M4 12h16m-6-6 6 6-6 6"/>, back: <path d="M20 12H4m6-6-6 6 6 6"/>, check: <path d="m5 12 4 4L19 6"/>,
  bag: <><path d="M5 8h14l1 13H4L5 8Z"/><path d="M8 8V6a4 4 0 0 1 8 0v2"/></>,
  jar: <><rect x="6" y="7" width="12" height="14" rx="3"/><path d="M7 3h10v4H7zM6 12h12M10 16h4"/></>,
  box: <><path d="m12 3 9 5-9 5-9-5 9-5ZM3 8v9l9 5 9-5V8M12 13v9M8 5l9 5"/></>,
  scale: <><path d="M12 3v17M6 21h12M3 7h18M6 7l-4 8h8L6 7Zm12 0-4 8h8l-4-8Z"/></>,
  shield: <><path d="m12 3 8 3v6c0 5-8 9-8 9s-8-4-8-9V6l8-3Z"/><path d="m8 12 3 3 5-6"/></>,
  cash: <><rect x="2" y="5" width="20" height="14" rx="2"/><circle cx="12" cy="12" r="3"/><path d="M6 9h.01M18 15h.01"/></>,
  receipt: <><path d="M5 3h14v19l-3-2-4 2-4-2-3 2V3Z"/><path d="M9 7h6M9 11h6M9 15h3"/></>,
  chevron: <path d="m8 5 7 7-7 7"/>, info: <><circle cx="12" cy="12" r="9"/><path d="M12 11v6M12 7h.01"/></>,
  refresh: <><path d="M20 7V2m0 5h-5M4 17v5m0-5h5"/><path d="M20 7a9 9 0 0 0-15-3M4 17a9 9 0 0 0 15 3"/></>,
  lock: <><rect x="5" y="10" width="14" height="11" rx="2"/><path d="M8 10V7a4 4 0 0 1 8 0v3M12 14v3"/></>,
};
export function Icon({ name, size = 20, className = '' }: { name: string; size?: number; className?: string }) {
  return <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round" className={className} aria-hidden="true">{paths[name] || paths.box}</svg>;
}
