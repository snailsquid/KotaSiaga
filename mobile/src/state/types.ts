import type { Point, ReportCategory, Vote } from '../api/types';

export type PlaceRefKind = 'place' | 'tourism' | 'donor' | 'pin' | 'me' | 'subscription';

export interface PlaceRef {
  id?: string;
  kind: PlaceRefKind;
  name: string;
  address?: string;
  lat: number;
  lon: number;
  category?: string;
  opens_at?: string;
  closes_at?: string;
  free_slots?: number;
}

/** Actions that need an account (spec §8.1). They continue by themselves after sign-in. */
export type PendingAction =
  | { type: 'vote'; id: string; vote: Vote; then?: 'report' }
  | { type: 'submit'; cat: ReportCategory; at: Point }
  | { type: 'watch'; center: Point; name?: string }
  | { type: 'alerts' }
  | { type: 'subscriptions' };

export type SheetView =
  | { view: 'home' }
  | { view: 'search'; pickFor?: { field: 'origin' | 'dest'; dest: PlaceRef; origin: PlaceRef | null } }
  | { view: 'place'; place: PlaceRef }
  | { view: 'route'; dest: PlaceRef; origin: PlaceRef | null; selected?: string }
  | { view: 'route_detail'; dest: PlaceRef; origin: PlaceRef | null; routeId: string }
  | { view: 'report'; id: string }
  | { view: 'quick'; at?: Point }
  | { view: 'pick_location'; at: Point }
  | { view: 'dup'; dupId: string; cat: ReportCategory; at: Point }
  | { view: 'sent'; id: string; cat: ReportCategory }
  | { view: 'signin'; reason: string; pending: PendingAction }
  | { view: 'list' }
  | { view: 'alerts' }
  | { view: 'filter' }
  | { view: 'cell'; cellId: string; at: Point }
  | { view: 'disaster'; id: string }
  | { view: 'account' }
  | { view: 'subscriptions'; add?: { center: Point; name?: string } }
  | { view: 'location_intro' }
  | { view: 'about' };

export type ViewName = SheetView['view'];

/** 0 = peek, 1 = half, 2 = full (spec §3.1). */
export type Snap = 0 | 1 | 2;

export type SheetEntry = SheetView & { snap: Snap };

export const DEFAULT_SNAP: Record<ViewName, Snap> = {
  home: 0,
  search: 2,
  place: 1,
  route: 1,
  route_detail: 1,
  report: 1,
  quick: 2,
  pick_location: 0,
  dup: 1,
  sent: 2,
  signin: 1,
  list: 2,
  alerts: 2,
  filter: 2,
  cell: 1,
  disaster: 1,
  account: 1,
  subscriptions: 2,
  location_intro: 1,
  about: 1,
};

export type StatusFilter = 'all' | 'verified' | 'waiting';

export interface Filters {
  cats: ReportCategory[];
  status: StatusFilter;
}

export interface Layers {
  risk: boolean;
  reports: boolean;
  disasters: boolean;
  tourism: boolean;
  donor: boolean;
}
