import type { ReportCategory } from '../api/types';

// Spec §13 names Material Symbols (flood, car_crash, construction, block, groups). Paper renders
// MaterialCommunityIcons; these are the closest glyphs available there.
export interface CategoryInfo {
  label: string;
  hint: string;
  icon: string;
  /** Default title when a report has no description. */
  title: string;
  /** Plain question asked to nearby residents (spec §12.1). */
  ask: (place: string) => string;
  yes: string;
  no: string;
  expiresHint: string;
  /** Short cause word used in risk-zone labels ("Waspada · Genangan"). */
  cause: string;
}

export const CATEGORY_ORDER: ReportCategory[] = ['flood', 'accident', 'road_damage', 'road_closure', 'event_crowd'];

export const CATEGORIES: Record<ReportCategory, CategoryInfo> = {
  flood: {
    label: 'Banjir',
    hint: 'Genangan, air naik',
    icon: 'home-flood',
    title: 'Genangan air',
    ask: (p) => `Masih ada genangan di ${p}?`,
    yes: 'Masih ada',
    no: 'Sudah surut',
    expiresHint: 'dalam ±3 jam',
    cause: 'Genangan',
  },
  accident: {
    label: 'Kecelakaan',
    hint: 'Tabrakan, kendaraan terguling',
    icon: 'car-emergency',
    title: 'Kecelakaan lalu lintas',
    ask: (p) => `Kecelakaan di ${p} masih menghambat jalan?`,
    yes: 'Masih',
    no: 'Sudah bersih',
    expiresHint: 'dalam ±2 jam',
    cause: 'Kecelakaan',
  },
  road_damage: {
    label: 'Jalan rusak',
    hint: 'Lubang, aspal amblas',
    icon: 'traffic-cone',
    title: 'Jalan berlubang',
    ask: (p) => `Kerusakan di ${p} masih ada?`,
    yes: 'Masih ada',
    no: 'Sudah diperbaiki',
    expiresHint: 'dalam ±7 hari',
    cause: 'Jalan rusak',
  },
  road_closure: {
    label: 'Jalan ditutup',
    hint: 'Ditutup, dialihkan',
    icon: 'cancel',
    title: 'Jalan ditutup',
    ask: (p) => `${p} masih ditutup?`,
    yes: 'Masih ditutup',
    no: 'Sudah dibuka',
    expiresHint: 'dalam ±6 jam',
    cause: 'Jalan ditutup',
  },
  event_crowd: {
    label: 'Keramaian',
    hint: 'Acara, massa, macet',
    icon: 'account-group',
    title: 'Keramaian dan macet',
    ask: (p) => `Masih ramai di ${p}?`,
    yes: 'Masih ramai',
    no: 'Sudah sepi',
    expiresHint: 'dalam ±4 jam',
    cause: 'Keramaian',
  },
};

export const SEVERITY_LABEL: Record<number, string> = { 1: 'Ringan', 2: 'Sedang', 3: 'Berat' };

/** Confirmations needed before a report shows as confirmed. The real rule is weight ≥ 3.0 (AGENTS.md §4). */
export const CONFIRM_TARGET = 3;
/** Only people within this distance can confirm (spec §12.3, option D). */
export const CONFIRM_RADIUS_M = 500;
/** Duplicate check radius before sending (spec §7.6). */
export const DUPLICATE_RADIUS_M = 300;
