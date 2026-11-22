import type { Point, Bbox } from '../api/types';

const R = 6371000;
const rad = (d: number) => (d * Math.PI) / 180;

export function distanceM(a: Point, b: Point): number {
  const dLat = rad(b.lat - a.lat);
  const dLon = rad(b.lon - a.lon);
  const h = Math.sin(dLat / 2) ** 2 + Math.cos(rad(a.lat)) * Math.cos(rad(b.lat)) * Math.sin(dLon / 2) ** 2;
  return 2 * R * Math.asin(Math.sqrt(h));
}

/** Meters per degree at Bandung's latitude, good enough for local math. */
export const M_PER_DEG_LAT = 110574;
export const M_PER_DEG_LON = 111320 * Math.cos(rad(-6.91));

export function offset(p: Point, dxM: number, dyM: number): Point {
  return { lat: p.lat + dyM / M_PER_DEG_LAT, lon: p.lon + dxM / M_PER_DEG_LON };
}

/** Distance from p to segment ab, in meters (planar approximation). */
export function distToSegmentM(p: Point, a: Point, b: Point): number {
  const ax = (a.lon - p.lon) * M_PER_DEG_LON;
  const ay = (a.lat - p.lat) * M_PER_DEG_LAT;
  const bx = (b.lon - p.lon) * M_PER_DEG_LON;
  const by = (b.lat - p.lat) * M_PER_DEG_LAT;
  const dx = bx - ax;
  const dy = by - ay;
  const len2 = dx * dx + dy * dy;
  const t = len2 === 0 ? 0 : Math.max(0, Math.min(1, -(ax * dx + ay * dy) / len2));
  const cx = ax + t * dx;
  const cy = ay + t * dy;
  return Math.sqrt(cx * cx + cy * cy);
}

export function circlePolygon(center: Point, radiusM: number, steps = 28): { latitude: number; longitude: number }[] {
  const out = [];
  for (let i = 0; i < steps; i++) {
    const a = (i / steps) * Math.PI * 2;
    const p = offset(center, Math.cos(a) * radiusM, Math.sin(a) * radiusM);
    out.push({ latitude: p.lat, longitude: p.lon });
  }
  return out;
}

export function inBbox(p: Point, b: Bbox): boolean {
  return p.lon >= b.minLon && p.lon <= b.maxLon && p.lat >= b.minLat && p.lat <= b.maxLat;
}

export function bboxParam(b: Bbox): string {
  return [b.minLon, b.minLat, b.maxLon, b.maxLat].map((n) => n.toFixed(5)).join(',');
}

export function parseBbox(s: string): Bbox | null {
  const parts = s.split(',').map(Number);
  if (parts.length !== 4 || parts.some((n) => !Number.isFinite(n))) return null;
  const [minLon, minLat, maxLon, maxLat] = parts;
  return { minLon, minLat, maxLon, maxLat };
}

export const toLatLng = (p: Point) => ({ latitude: p.lat, longitude: p.lon });
