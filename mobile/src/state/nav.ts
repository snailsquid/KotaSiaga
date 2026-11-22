import { DEFAULT_SNAP, type SheetEntry, type SheetView, type Snap } from './types';

// One sheet whose content changes (spec §1). It remembers the last 5 contents (spec §3.3).
export interface NavState {
  current: SheetEntry;
  hist: SheetEntry[];
}

export type NavAction =
  | { type: 'go'; to: SheetView; snap?: Snap }
  | { type: 'replace'; to: SheetView; snap?: Snap }
  | { type: 'back' }
  | { type: 'home' }
  | { type: 'snap'; snap: Snap }
  | { type: 'patch'; patch: Partial<SheetEntry> };

export const initialNav: NavState = { current: { view: 'home', snap: 0 }, hist: [] };

// Transient contents that back() skips over (prototype: signin and filter fall through).
const SKIP_ON_BACK = new Set(['signin', 'filter', 'pick_location']);

export function navReducer(s: NavState, a: NavAction): NavState {
  switch (a.type) {
    case 'go': {
      const entry = { ...a.to, snap: a.snap ?? DEFAULT_SNAP[a.to.view] } as SheetEntry;
      if (a.to.view === 'home') return { current: entry, hist: [] };
      return { current: entry, hist: [...s.hist, s.current].slice(-5) };
    }
    case 'replace':
      return { ...s, current: { ...a.to, snap: a.snap ?? DEFAULT_SNAP[a.to.view] } as SheetEntry };
    case 'back': {
      const hist = s.hist.slice();
      let prev = hist.pop();
      while (prev && SKIP_ON_BACK.has(prev.view)) prev = hist.pop();
      return { current: prev ?? { view: 'home', snap: 0 }, hist };
    }
    case 'home':
      return { current: { view: 'home', snap: 0 }, hist: [] };
    case 'snap':
      if (s.current.snap === a.snap) return s;
      return { ...s, current: { ...s.current, snap: a.snap } };
    case 'patch':
      return { ...s, current: { ...s.current, ...a.patch } as SheetEntry };
  }
}
