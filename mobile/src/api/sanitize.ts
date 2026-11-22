// AGENTS.md §6: treat upstream data as untrusted. Validate against the contract and drop unknown fields.
import type {
  Alert,
  CellRisk,
  Change,
  DisasterSignal,
  Meta,
  Report,
  Route,
  SafeRouteResult,
  Status,
  Subscription,
  Token,
  Weather,
} from './types';

export class ContractError extends Error {}

const isNum = (v: unknown): v is number => typeof v === 'number' && Number.isFinite(v);
const isStr = (v: unknown): v is string => typeof v === 'string';
const oneOf = <T extends string>(v: unknown, opts: readonly T[]): v is T => isStr(v) && (opts as readonly string[]).includes(v);

function need(cond: boolean, what: string): asserts cond {
  if (!cond) throw new ContractError(`Respons tidak sesuai kontrak: ${what}`);
}

const RISK = ['safe', 'caution', 'danger'] as const;
const CATS = ['flood', 'accident', 'road_damage', 'road_closure', 'event_crowd'] as const;

export function meta(m: any): Meta {
  need(m && isNum(m.seq) && isStr(m.updated_at) && oneOf(m.source, ['live', 'mock'] as const) && typeof m.stale === 'boolean', 'meta');
  const out: Meta = { seq: m.seq, updated_at: m.updated_at, source: m.source, stale: m.stale };
  if (m.reset === true) out.reset = true;
  return out;
}

export function cellRisk(c: any): CellRisk {
  need(c && isStr(c.cell_id) && oneOf(c.risk_level, RISK) && isNum(c.score) && Array.isArray(c.factors), 'CellRisk');
  return {
    cell_id: c.cell_id,
    risk_level: c.risk_level,
    score: c.score,
    factors: c.factors
      .filter((f: any) => f && oneOf(f.type, ['rain', 'verified_report', 'disaster_signal'] as const))
      .map((f: any) => ({ type: f.type, ...(isNum(f.value) ? { value: f.value } : {}), ...(isNum(f.count) ? { count: f.count } : {}) })),
  };
}

export function weather(w: any): Weather {
  need(w && isStr(w.observed_at) && isNum(w.temperature_c) && isNum(w.rain_mm_h) && oneOf(w.condition, ['clear', 'cloudy', 'rain', 'heavy_rain', 'storm'] as const) && Array.isArray(w.forecast_6h), 'Weather');
  return {
    observed_at: w.observed_at,
    temperature_c: w.temperature_c,
    rain_mm_h: w.rain_mm_h,
    condition: w.condition,
    forecast_6h: w.forecast_6h
      .filter((f: any) => f && isStr(f.at) && isNum(f.rain_mm_h) && isNum(f.temperature_c))
      .map((f: any) => ({ at: f.at, rain_mm_h: f.rain_mm_h, temperature_c: f.temperature_c })),
  };
}

export function status(s: any): Status {
  need(s && isStr(s.cell_id) && oneOf(s.risk_level, RISK) && isNum(s.active_reports) && isNum(s.disaster_signals), 'Status');
  return { cell_id: s.cell_id, risk_level: s.risk_level, weather: weather(s.weather), active_reports: s.active_reports, disaster_signals: s.disaster_signals };
}

export function disaster(d: any): DisasterSignal {
  need(d && isStr(d.id) && oneOf(d.type, ['flood', 'earthquake', 'fire', 'haze', 'wind', 'volcano'] as const) && isNum(d.lat) && isNum(d.lon) && isStr(d.reported_at) && oneOf(d.status, ['active', 'resolved'] as const), 'DisasterSignal');
  return { id: d.id, type: d.type, lat: d.lat, lon: d.lon, reported_at: d.reported_at, status: d.status, resolved_at: isStr(d.resolved_at) ? d.resolved_at : null };
}

export function report(r: any): Report {
  need(r && isStr(r.id) && oneOf(r.category, CATS) && oneOf(r.status, ['waiting', 'verified', 'expired'] as const) && isNum(r.lat) && isNum(r.lon) && isNum(r.severity) && isStr(r.created_at) && isStr(r.expires_at), 'Report');
  return {
    id: r.id,
    category: r.category,
    status: r.status,
    lat: r.lat,
    lon: r.lon,
    // Free text from other users: shown as plain text only, never interpreted.
    description: isStr(r.description) ? r.description.slice(0, 500) : undefined,
    severity: Math.min(3, Math.max(1, Math.round(r.severity))),
    confirm_count: isNum(r.confirm_count) ? r.confirm_count : 0,
    dispute_count: isNum(r.dispute_count) ? r.dispute_count : 0,
    created_at: r.created_at,
    verified_at: isStr(r.verified_at) ? r.verified_at : null,
    expires_at: r.expires_at,
  };
}

function route(r: any): Route {
  need(r && isStr(r.id) && isNum(r.distance_m) && isNum(r.duration_s) && isNum(r.risk_penalty) && oneOf(r.max_risk_level, RISK) && r.geometry?.type === 'LineString' && Array.isArray(r.geometry.coordinates), 'Route');
  return {
    id: r.id,
    distance_m: r.distance_m,
    duration_s: r.duration_s,
    risk_penalty: r.risk_penalty,
    max_risk_level: r.max_risk_level,
    geometry: { type: 'LineString', coordinates: r.geometry.coordinates.filter((c: any) => Array.isArray(c) && isNum(c[0]) && isNum(c[1])).map((c: any) => [c[0], c[1]]) },
    reasons: Array.isArray(r.reasons) ? r.reasons.filter(isStr) : [],
  };
}

export function safeRoute(s: any): SafeRouteResult {
  need(s && isStr(s.recommended_id) && Array.isArray(s.routes), 'SafeRouteResult');
  return { recommended_id: s.recommended_id, routes: s.routes.map(route) };
}

export function change(c: any): Change {
  need(c && isNum(c.seq) && isStr(c.at) && isStr(c.type) && isStr(c.entity_id), 'Change');
  return { seq: c.seq, at: c.at, type: c.type, entity_id: c.entity_id, summary: isStr(c.summary) ? c.summary : '' };
}

export function subscription(s: any): Subscription {
  need(s && isStr(s.id) && isStr(s.name) && isNum(s.lat) && isNum(s.lon) && isNum(s.radius_m) && isStr(s.created_at), 'Subscription');
  return { id: s.id, name: s.name, lat: s.lat, lon: s.lon, radius_m: s.radius_m, created_at: s.created_at };
}

export function alert(a: any): Alert {
  need(a && isStr(a.id) && isStr(a.subscription_id) && isStr(a.cell_id) && oneOf(a.risk_level, RISK) && isStr(a.at) && isStr(a.message), 'Alert');
  return { id: a.id, subscription_id: a.subscription_id, cell_id: a.cell_id, risk_level: a.risk_level, at: a.at, message: a.message };
}

export function token(t: any): Token {
  need(t && isStr(t.access_token) && t.token_type === 'bearer' && isNum(t.expires_in), 'Token');
  return { access_token: t.access_token, token_type: 'bearer', expires_in: t.expires_in };
}

export const list = <T>(fn: (x: any) => T) => (xs: any): T[] => {
  need(Array.isArray(xs), 'list');
  // Drop single invalid items instead of failing the whole list.
  const out: T[] = [];
  for (const x of xs) {
    try {
      out.push(fn(x));
    } catch {
      /* skip */
    }
  }
  return out;
};
