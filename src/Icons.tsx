import React from "react";
type P = React.SVGProps<SVGSVGElement>;

export const Ic = {
  search: (p: P) => <svg viewBox="0 0 16 16" width="14" height="14" fill="none" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" strokeLinejoin="round" {...p}><circle cx="7" cy="7" r="4.5"/><path d="M10.5 10.5 L13.5 13.5"/></svg>,
  pin: (p: P) => <svg viewBox="0 0 16 16" width="12" height="12" fill="currentColor" {...p}><path d="M9.7 1.3 6.6 4.4l-3-.5-.8.8 3.5 3.5-3.5 3.5.7.7 3.5-3.5 3.5 3.5.8-.8-.5-3 3.1-3.1-3.5-3.2z"/></svg>,
  mute: (p: P) => <svg viewBox="0 0 16 16" width="12" height="12" fill="none" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" strokeLinejoin="round" {...p}><path d="M3 6h2l3-2.5v9L5 10H3z"/><path d="M11 6l3 4M14 6l-3 4"/></svg>,
  plus: (p: P) => <svg viewBox="0 0 16 16" width="14" height="14" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" {...p}><path d="M8 3v10M3 8h10"/></svg>,
  dots: (p: P) => <svg viewBox="0 0 16 16" width="14" height="14" fill="currentColor" {...p}><circle cx="3" cy="8" r="1.2"/><circle cx="8" cy="8" r="1.2"/><circle cx="13" cy="8" r="1.2"/></svg>,
  x: (p: P) => <svg viewBox="0 0 16 16" width="12" height="12" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" {...p}><path d="M4 4l8 8M12 4l-8 8"/></svg>,
  reply: (p: P) => <svg viewBox="0 0 16 16" width="13" height="13" fill="none" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" strokeLinejoin="round" {...p}><path d="M6 4 2 8l4 4"/><path d="M2 8h7a4 4 0 0 1 4 4v1"/></svg>,
  forward: (p: P) => <svg viewBox="0 0 16 16" width="13" height="13" fill="none" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" strokeLinejoin="round" {...p}><path d="M10 4l4 4-4 4"/><path d="M14 8H7a4 4 0 0 0-4 4v1"/></svg>,
  smile: (p: P) => <svg viewBox="0 0 16 16" width="13" height="13" fill="none" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" {...p}><circle cx="8" cy="8" r="5.5"/><circle cx="6" cy="7" r="0.6" fill="currentColor"/><circle cx="10" cy="7" r="0.6" fill="currentColor"/><path d="M5.5 10c.7.8 1.6 1.2 2.5 1.2s1.8-.4 2.5-1.2"/></svg>,
  copy: (p: P) => <svg viewBox="0 0 16 16" width="12" height="12" fill="none" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" strokeLinejoin="round" {...p}><rect x="5" y="5" width="8" height="8" rx="1"/><path d="M3 11V3.5a.5.5 0 0 1 .5-.5H10"/></svg>,
  settings: (p: P) => <svg viewBox="0 0 16 16" width="14" height="14" fill="none" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" strokeLinejoin="round" {...p}><circle cx="8" cy="8" r="2"/><path d="M8 1.5v2M8 12.5v2M1.5 8h2M12.5 8h2M3.3 3.3l1.4 1.4M11.3 11.3l1.4 1.4M3.3 12.7l1.4-1.4M11.3 4.7l1.4-1.4"/></svg>,
  resume: (p: P) => <svg viewBox="0 0 16 16" width="12" height="12" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" {...p}><circle cx="8" cy="8" r="5.5"/><path d="M6.5 5v6l5-3z" fill="currentColor" stroke="none"/></svg>,
};

export function KindIcon({ kind, ...rest }: { kind: string } & P) {
  const terminal = (p: P) => <svg viewBox="0 0 16 16" width="14" height="14" fill="none" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" strokeLinejoin="round" {...p}><rect x="1.5" y="3" width="13" height="10" rx="1"/><path d="M4 7l2 1.5L4 10M8 10.5h4"/></svg>;
  const agent = (p: P) => <svg viewBox="0 0 16 16" width="14" height="14" fill="none" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" strokeLinejoin="round" {...p}><circle cx="8" cy="6" r="3"/><path d="M3 13.5c.6-2.2 2.7-3.5 5-3.5s4.4 1.3 5 3.5"/></svg>;
  const server = (p: P) => <svg viewBox="0 0 16 16" width="14" height="14" fill="none" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" strokeLinejoin="round" {...p}><rect x="2" y="3" width="12" height="4" rx="0.5"/><rect x="2" y="9" width="12" height="4" rx="0.5"/><circle cx="4.5" cy="5" r="0.6" fill="currentColor"/><circle cx="4.5" cy="11" r="0.6" fill="currentColor"/></svg>;
  switch (kind) {
    case "agent": return agent(rest);
    case "ssh": return server(rest);
    default: return terminal(rest);
  }
}
