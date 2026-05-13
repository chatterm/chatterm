// Visual persona — controls how session avatars are drawn.
//   "operator" — monogram tile (default, identity-first)
//   "pet"      — kawaii creature whose face changes with status
//
// Persisted to localStorage so the choice survives reloads. Subscribers
// (Avatar component instances) re-render when the value changes; this
// avoids threading the flag through prop drilling at every callsite.

export type Persona = "operator" | "pet";

const KEY = "chatterm-persona";
const DEFAULT: Persona = "operator";

let current: Persona = (() => {
  try {
    const v = localStorage.getItem(KEY);
    if (v === "operator" || v === "pet") return v;
  } catch {}
  return DEFAULT;
})();

const subscribers = new Set<(p: Persona) => void>();

export function getPersona(): Persona { return current; }

export function setPersona(p: Persona) {
  if (p === current) return;
  current = p;
  try { localStorage.setItem(KEY, p); } catch {}
  subscribers.forEach(fn => fn(p));
}

export function subscribePersona(fn: (p: Persona) => void): () => void {
  subscribers.add(fn);
  return () => { subscribers.delete(fn); };
}
