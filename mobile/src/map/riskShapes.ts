// Spec §12b.1: no grid squares. The 500 m cells are turned into what people see on the ground:
// soft-edged zones with a small label, and colored stretches of the roads that pass through.
// Only Waspada and Bahaya are drawn (spec §12.4).

import { ApiService } from '../api/ApiService';
import type { CellRisk, DisasterSignal, Point, Report, RiskLevel } from '../api/types';
import { STREETS } from '../api/mock/gazetteer';
import { CATEGORIES } from '../domain/categories';
import { distanceM } from '../domain/geo';
import { RISK_LABEL, RISK_ORDER } from '../domain/risk';

export interface Zone {
  cellId: string;
  center: Point;
  level: Exclude<RiskLevel, 'safe'>;
}

export interface ZoneLabel {
  key: string;
  at: Point;
  level: Exclude<RiskLevel, 'safe'>;
  text: string;
  cellId: string;
}

export interface RoadStretch {
  key: string;
  level: Exclude<RiskLevel, 'safe'>;
  coords: { latitude: number; longitude: number }[];
}

export function riskZones(cells: Map<string, CellRisk>): Zone[] {
  const out: Zone[] = [];
  for (const c of cells.values()) {
    if (c.risk_level === 'safe') continue;
    const center = ApiService.cellCenter(c.cell_id);
    if (center) out.push({ cellId: c.cell_id, center, level: c.risk_level });
  }
  return out;
}

/** What is causing risk in a cell, in one word for the label. */
export function cellCause(cell: CellRisk, center: Point, reports: Report[], disasters: DisasterSignal[]): string {
  const near = reports.filter((r) => r.status === 'verified' && distanceM(center, r) < 400);
  if (near.length) return CATEGORIES[near[0].category].cause;
  if ((cell.factors.find((f) => f.type === 'disaster_signal')?.count ?? 0) > 0 || disasters.some((d) => d.status === 'active' && distanceM(center, d) < 900)) {
    return 'Sinyal bencana';
  }
  return 'Hujan lebat';
}

/** One label per group of touching zones, placed on its strongest cell. */
export function zoneLabels(zones: Zone[], cells: Map<string, CellRisk>, reports: Report[], disasters: DisasterSignal[]): ZoneLabel[] {
  const left = new Set(zones.map((z) => z.cellId));
  const byId = new Map(zones.map((z) => [z.cellId, z]));
  const labels: ZoneLabel[] = [];
  for (const z of zones) {
    if (!left.has(z.cellId)) continue;
    const group: Zone[] = [];
    const stack = [z];
    left.delete(z.cellId);
    while (stack.length) {
      const cur = stack.pop()!;
      group.push(cur);
      for (const other of zones) {
        if (left.has(other.cellId) && distanceM(cur.center, other.center) < 760) {
          left.delete(other.cellId);
          stack.push(byId.get(other.cellId)!);
        }
      }
    }
    group.sort((a, b) => RISK_ORDER[b.level] - RISK_ORDER[a.level] || (cells.get(b.cellId)?.score ?? 0) - (cells.get(a.cellId)?.score ?? 0));
    const top = group[0];
    const cell = cells.get(top.cellId)!;
    labels.push({
      key: top.cellId,
      at: top.center,
      level: top.level,
      cellId: top.cellId,
      text: `${RISK_LABEL[top.level]} · ${cellCause(cell, top.center, reports, disasters)}`,
    });
  }
  return labels;
}

function densePath(path: [number, number][], stepM = 40): Point[] {
  const out: Point[] = [];
  for (let k = 0; k < path.length - 1; k++) {
    const a = { lat: path[k][0], lon: path[k][1] };
    const b = { lat: path[k + 1][0], lon: path[k + 1][1] };
    const n = Math.max(1, Math.ceil(distanceM(a, b) / stepM));
    for (let s = 0; s < n; s++) out.push({ lat: a.lat + ((b.lat - a.lat) * s) / n, lon: a.lon + ((b.lon - a.lon) * s) / n });
  }
  const last = path[path.length - 1];
  out.push({ lat: last[0], lon: last[1] });
  return out;
}

const DENSE = STREETS.map((s) => ({ name: s.name, pts: densePath(s.path) }));

/** Parts of known roads that pass through risky zones, colored like a traffic layer. */
export function roadStretches(zones: Zone[]): RoadStretch[] {
  if (zones.length === 0) return [];
  const out: RoadStretch[] = [];
  for (const s of DENSE) {
    let run: Point[] = [];
    let runLevel: Zone['level'] = 'caution';
    const flush = () => {
      if (run.length >= 2) {
        out.push({ key: `${s.name}-${out.length}`, level: runLevel, coords: run.map((p) => ({ latitude: p.lat, longitude: p.lon })) });
      }
      run = [];
    };
    for (const p of s.pts) {
      let level: Zone['level'] | null = null;
      for (const z of zones) {
        if (distanceM(p, z.center) <= 330) {
          if (!level || RISK_ORDER[z.level] > RISK_ORDER[level]) level = z.level;
        }
      }
      if (level && (run.length === 0 || level === runLevel)) {
        runLevel = level;
        run.push(p);
      } else {
        flush();
        if (level) {
          runLevel = level;
          run.push(p);
        }
      }
    }
    flush();
  }
  return out;
}
