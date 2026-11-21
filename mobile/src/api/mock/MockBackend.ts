// In-app stand-in for the Smart City core service while the app is built (frontend only).
// It follows AGENTS.md §7: mock the INPUTS (weather, reports, votes, disaster signals, faults)
// and run real rules for verification, risk and routing, so live updates show the system working.
// Responses use the exact envelopes and shapes from openapi.yaml.
//
// Paths marked "GAP" below are not in the contract yet; they exist only so the UI can be built.

import type {
  Alert,
  CellRisk,
  Change,
  ChangeType,
  DisasterSignal,
  ForecastPoint,
  Meta,
  MockScenario,
  Point,
  Report,
  ReportCategory,
  RiskFactor,
  RiskLevel,
  Route,
  Subscription,
  Vote,
  Weather,
  WeatherCondition,
} from '../types';
import { M_PER_DEG_LAT, M_PER_DEG_LON, distanceM, offset, parseBbox, inBbox } from '../../domain/geo';
import { CATEGORIES } from '../../domain/categories';
import { RISK_LABEL, RISK_ORDER } from '../../domain/risk';
import { STREETS, nearestStreet } from './gazetteer';
import { config } from '../../config';

// ---- rules (mirrors the single risk config of the core service, AGENTS.md §4) ----
const RULES = {
  cellSizeM: 500,
  reportWeight: 0.15,
  rainDivisor: 50,
  rainWeight: 0.4,
  disasterWeight: 0.4,
  cautionFrom: 0.35,
  dangerFrom: 0.7,
  verifyWeight: 3.0,
  expiryH: { flood: 3, accident: 2, road_closure: 6, road_damage: 168, event_crowd: 4 } as Record<ReportCategory, number>,
  maxLiveReports: 150,
  changeWindow: 500,
};

export const BOUNDS = { minLat: -6.98, maxLat: -6.84, minLon: 107.54, maxLon: 107.72 };
/** Where the demo user stands when device location is not used (near Jl. Pasteur / Cihampelas). */
export const MOCK_ME: Point = { lat: -6.893, lon: 107.604 };

const D_LAT = RULES.cellSizeM / M_PER_DEG_LAT;
const D_LON = RULES.cellSizeM / M_PER_DEG_LON;
const ROWS = Math.ceil((BOUNDS.maxLat - BOUNDS.minLat) / D_LAT);
const COLS = Math.ceil((BOUNDS.maxLon - BOUNDS.minLon) / D_LON);

export function cellOf(p: Point): { i: number; j: number; id: string } {
  const i = Math.max(0, Math.min(ROWS - 1, Math.floor((p.lat - BOUNDS.minLat) / D_LAT)));
  const j = Math.max(0, Math.min(COLS - 1, Math.floor((p.lon - BOUNDS.minLon) / D_LON)));
  return { i, j, id: `c_${i}_${j}` };
}

export function cellCenter(id: string): Point | null {
  const m = /^c_(\d+)_(\d+)$/.exec(id);
  if (!m) return null;
  return { lat: BOUNDS.minLat + (Number(m[1]) + 0.5) * D_LAT, lon: BOUNDS.minLon + (Number(m[2]) + 0.5) * D_LON };
}

function mulberry32(seed: number) {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

const DESCRIPTIONS: Record<ReportCategory, string[]> = {
  flood: ['Genangan setinggi 30 cm', 'Air naik di underpass', 'Genangan menutup satu lajur', 'Selokan meluap ke jalan'],
  accident: ['Tabrakan motor, satu lajur tertutup', 'Mobil tergelincir di tikungan', 'Motor jatuh, lalu lintas tersendat'],
  road_damage: ['Lubang besar di lajur kiri', 'Aspal amblas dekat saluran air', 'Jalan bergelombang dan licin'],
  road_closure: ['Ditutup untuk perbaikan pipa', 'Ditutup sementara, dialihkan', 'Pohon tumbang menutup jalan'],
  event_crowd: ['Konser di Sabuga, lalu lintas padat', 'Pawai, jalan dipenuhi massa', 'Pasar tumpah, macet panjang'],
};

interface StoredReport extends Report {
  reporter: string;
}

interface Resp {
  status: number;
  body: unknown;
  headers?: Record<string, string>;
}

interface Req {
  method: string;
  path: string;
  query: Record<string, string | undefined>;
  body?: any;
  token?: string;
}

export class MockBackend {
  seed: number;
  private rnd: () => number;
  private seq = 0;
  private updatedAt = new Date().toISOString();
  private changes: Change[] = [];
  private reports = new Map<string, StoredReport>();
  private votes = new Map<string, Map<string, Vote>>();
  private disasters = new Map<string, DisasterSignal>();
  private cells = new Map<string, CellRisk>();
  private subs = new Map<string, Subscription[]>();
  private alerts = new Map<string, Alert[]>();
  private users = new Map<string, number>(); // user id -> reputation
  private tokens = new Map<string, string>(); // token -> user id
  private storm = { lat: -6.8935, lon: 107.599, base: 55, until: 0 };
  private temp = 24;
  private tickN = 0;
  private quietTicks = 0;
  private lastRiskTransition = Date.now();
  private upstreamDownUntil = 0;
  private idN = 0;
  private timer: ReturnType<typeof setInterval> | null = null;
  private scenario: string | null = null;
  /** Initial confirmations of seed reports (no stored votes behind them). */
  private seeded = new Map<string, number>();

  constructor(seed?: number) {
    this.seed = seed ?? Math.floor(Math.random() * 2 ** 31);
    this.rnd = mulberry32(this.seed);
    // AGENTS.md §7: always log the seed at startup.
    console.log(`[mock] KotaSiaga mock backend seed=${this.seed} tick=${config.mockTickSeconds}s scale=${config.mockTimeScale}`);
    this.init();
  }

  start() {
    if (this.timer) return;
    this.timer = setInterval(() => this.tick(), config.mockTickSeconds * 1000);
  }

  stop() {
    if (this.timer) clearInterval(this.timer);
    this.timer = null;
  }

  // ---------------------------------------------------------------- setup
  private init() {
    for (let k = 1; k <= 20; k++) this.users.set(`user_${String(k).padStart(2, '0')}`, 0.2 + this.rnd() * 0.8);
    const now = Date.now();
    const seedReports: [ReportCategory, number, number, number, 'waiting' | 'verified', number, number][] = [
      // category, lat, lon, severity, status, confirms, minutes ago
      ['flood', -6.894, 107.6025, 2, 'waiting', 1, 12],
      ['accident', -6.8955, 107.6129, 3, 'verified', 5, 25],
      ['road_damage', -6.903, 107.6122, 2, 'verified', 4, 120],
      ['event_crowd', -6.887, 107.6082, 1, 'waiting', 0, 40],
      ['road_closure', -6.911, 107.6105, 2, 'verified', 6, 60],
    ];
    seedReports.forEach(([cat, lat, lon, sev, status, conf, mins], k) => {
      const created = now - mins * 60000;
      const r = this.makeReport(cat, { lat, lon }, sev, `user_${String(k + 3).padStart(2, '0')}`, created, DESCRIPTIONS[cat][0]);
      r.status = status;
      r.confirm_count = conf;
      this.seeded.set(r.id, conf);
      if (status === 'verified') r.verified_at = new Date(created + 5 * 60000).toISOString();
      // Seed reports get a full real-time window so the demo does not start by expiring them.
      r.expires_at = new Date(now + (10 + k * 4) * 60000).toISOString();
      this.reports.set(r.id, r);
    });
    this.storm = { lat: -6.8935, lon: 107.599, base: 55, until: now + 90000 };
    this.recomputeRisk(true);
    this.push('weather.updated', 'weather', 'Cuaca diperbarui');
  }

  private id(prefix: string) {
    this.idN += 1;
    return `${prefix}_${this.idN}${Math.floor(this.rnd() * 1000)}`;
  }

  private realExpiryMs(cat: ReportCategory) {
    const simulated = RULES.expiryH[cat] * 3600 * 1000;
    return Math.min(simulated / config.mockTimeScale, 30 * 60 * 1000);
  }

  private makeReport(cat: ReportCategory, p: Point, severity: number, reporter: string, created = Date.now(), description?: string): StoredReport {
    return {
      id: this.id('rpt'),
      category: cat,
      status: 'waiting',
      lat: p.lat,
      lon: p.lon,
      description,
      severity,
      confirm_count: 0,
      dispute_count: 0,
      created_at: new Date(created).toISOString(),
      verified_at: null,
      expires_at: new Date(created + this.realExpiryMs(cat)).toISOString(),
      reporter,
    };
  }

  // ---------------------------------------------------------------- feed
  private push(type: ChangeType, entity: string, summary: string) {
    this.seq += 1;
    this.updatedAt = new Date().toISOString();
    this.changes.push({ seq: this.seq, at: this.updatedAt, type, entity_id: entity, summary });
    if (this.changes.length > RULES.changeWindow) this.changes.splice(0, this.changes.length - RULES.changeWindow);
  }

  private meta(stale = false, extra: Partial<Meta> = {}): Meta {
    return { seq: this.seq, updated_at: this.updatedAt, source: 'mock', stale, ...extra };
  }

  private get upstreamDown() {
    return Date.now() < this.upstreamDownUntil;
  }

  // ---------------------------------------------------------------- generator
  tick() {
    this.tickN += 1;
    const seqBefore = this.seq;
    const actions = 1 + Math.floor(this.rnd() * 3);
    for (let k = 0; k < actions; k++) {
      const x = this.rnd();
      if (x < 0.35) this.weatherStep();
      else if (x < 0.6) this.newReport();
      else if (x < 0.85) this.fakeVotes();
      else this.disasterStep();
    }
    this.expireReports();
    this.resolveDisasters();
    if (this.rnd() < 0.03 && !this.upstreamDown) {
      this.upstreamDownUntil = Date.now() + (20 + this.rnd() * 20) * 1000;
    }
    this.recomputeRisk();
    // Guarantees so the demo never looks frozen.
    this.quietTicks = this.seq === seqBefore ? this.quietTicks + 1 : 0;
    if (this.quietTicks >= 3) {
      this.weatherStep(true);
      this.quietTicks = 0;
    }
    if (Date.now() - this.lastRiskTransition > 120000) {
      this.runScenario('rain_storm');
      this.lastRiskTransition = Date.now();
    }
  }

  private weatherStep(force = false) {
    if (this.upstreamDown && !force) return;
    const now = Date.now();
    if (now < this.storm.until) {
      this.storm.base = Math.min(60, Math.max(25, this.storm.base + (this.rnd() - 0.5) * 6));
    } else {
      const step = this.rnd() * 4 * (this.rnd() < 0.5 ? -1 : 1);
      this.storm.base = Math.max(0, Math.min(60, this.storm.base * 0.9 + step));
      if (this.storm.base < 2 && this.rnd() < 0.1) {
        // 10% of dry ticks start a 1 to 2 minute storm
        this.storm.base = 25 + this.rnd() * 20;
        this.storm.until = now + (60 + this.rnd() * 60) * 1000;
        const p = offset(MOCK_ME, (this.rnd() - 0.5) * 4000, (this.rnd() - 0.5) * 4000);
        this.storm.lat = p.lat;
        this.storm.lon = p.lon;
      }
    }
    // storm drifts slowly
    const p = offset(this.storm, (this.rnd() - 0.4) * 120, (this.rnd() - 0.5) * 120);
    this.storm.lat = Math.max(BOUNDS.minLat, Math.min(BOUNDS.maxLat, p.lat));
    this.storm.lon = Math.max(BOUNDS.minLon, Math.min(BOUNDS.maxLon, p.lon));
    this.temp = 30 - this.storm.base * 0.15 + (this.rnd() - 0.5) * 0.6;
    this.push('weather.updated', 'weather', 'Cuaca diperbarui');
  }

  private rainAt(p: Point, minutesAhead = 0): number {
    const decay = minutesAhead === 0 ? 1 : Math.pow(0.72, minutesAhead / 60);
    const d = distanceM(p, this.storm);
    const sigma = 2200;
    return Math.max(0, this.storm.base * decay * Math.exp(-(d * d) / (2 * sigma * sigma)));
  }

  private randomPlaceOnStreet(near?: Point, radiusM = 3000): Point {
    for (let attempt = 0; attempt < 20; attempt++) {
      const s = STREETS[Math.floor(this.rnd() * STREETS.length)];
      const k = Math.floor(this.rnd() * (s.path.length - 1));
      const t = this.rnd();
      const p = {
        lat: s.path[k][0] + (s.path[k + 1][0] - s.path[k][0]) * t,
        lon: s.path[k][1] + (s.path[k + 1][1] - s.path[k][1]) * t,
      };
      if (!near || distanceM(p, near) <= radiusM) return p;
    }
    return near ? offset(near, (this.rnd() - 0.5) * radiusM, (this.rnd() - 0.5) * radiusM) : MOCK_ME;
  }

  private pickCategory(): ReportCategory {
    const x = this.rnd();
    if (x < 0.3) return 'flood';
    if (x < 0.55) return 'accident';
    if (x < 0.7) return 'road_damage';
    if (x < 0.85) return 'road_closure';
    return 'event_crowd';
  }

  private liveReports() {
    return [...this.reports.values()].filter((r) => r.status !== 'expired');
  }

  private newReport(cat?: ReportCategory, at?: Point, severity?: number): StoredReport | null {
    if (this.liveReports().length >= RULES.maxLiveReports) return null;
    const c = cat ?? this.pickCategory();
    // Bias toward the demo user so "Bantu pastikan" and the duplicate check have something to show.
    const p = at ?? (this.rnd() < 0.4 ? this.randomPlaceOnStreet(MOCK_ME, 900) : this.randomPlaceOnStreet());
    const user = `user_${String(1 + Math.floor(this.rnd() * 20)).padStart(2, '0')}`;
    const list = DESCRIPTIONS[c];
    const r = this.makeReport(c, p, severity ?? 1 + Math.floor(this.rnd() * 3), user, Date.now(), list[Math.floor(this.rnd() * list.length)]);
    this.reports.set(r.id, r);
    this.push('report.created', r.id, `Laporan baru: ${CATEGORIES[c].label}`);
    return r;
  }

  private fakeVotes() {
    const waiting = this.liveReports().filter((r) => r.status === 'waiting');
    if (waiting.length === 0) return;
    const n = 1 + Math.floor(this.rnd() * 2);
    for (let k = 0; k < n; k++) {
      const r = waiting[Math.floor(this.rnd() * waiting.length)];
      const user = `user_${String(1 + Math.floor(this.rnd() * 20)).padStart(2, '0')}`;
      if (user === r.reporter) continue;
      this.applyVote(r, user, this.rnd() < 0.78 ? 'confirm' : 'dispute');
    }
  }

  private weight(user: string) {
    return this.users.get(user) ?? 0.6;
  }

  private applyVote(r: StoredReport, user: string, vote: Vote) {
    let m = this.votes.get(r.id);
    if (!m) {
      m = new Map();
      this.votes.set(r.id, m);
    }
    m.set(user, vote);
    let confirmW = 0;
    let disputeW = 0;
    let confirmN = 0;
    let disputeN = 0;
    m.forEach((v, u) => {
      if (v === 'confirm') {
        confirmW += this.weight(u);
        confirmN += 1;
      } else {
        disputeW += this.weight(u);
        disputeN += 1;
      }
    });
    // Seed reports start with confirmations that have no stored vote; each counts as weight 0.6.
    const seeded = this.seeded.get(r.id) ?? 0;
    r.confirm_count = seeded + confirmN;
    r.dispute_count = disputeN;
    const net = confirmW + seeded * 0.6 - disputeW;
    if (r.status === 'waiting' && net >= RULES.verifyWeight) {
      r.status = 'verified';
      r.verified_at = new Date().toISOString();
      this.push('report.verified', r.id, `${CATEGORIES[r.category].label} dipastikan warga`);
    } else if (net <= -2 || (r.status === 'verified' && disputeW >= 2)) {
      // Disputes ("Sudah surut") end a report early.
      this.expire(r, 'dibantah warga');
    }
  }

  private expire(r: StoredReport, why = 'berakhir') {
    if (r.status === 'expired') return;
    r.status = 'expired';
    this.push('report.expired', r.id, `${CATEGORIES[r.category].label} ${why}`);
  }

  private expireReports() {
    const now = Date.now();
    for (const r of this.reports.values()) {
      if (r.status !== 'expired' && Date.parse(r.expires_at) <= now) this.expire(r);
      // forget expired reports after 10 minutes
      if (r.status === 'expired' && Date.parse(r.expires_at) < now - 600000) this.reports.delete(r.id);
    }
  }

  private disasterStep(at?: Point) {
    const active = [...this.disasters.values()].filter((d) => d.status === 'active');
    if (active.length >= 3) return;
    if (!at && this.rnd() > 0.35) return;
    const p = at ?? offset(MOCK_ME, (this.rnd() - 0.5) * 8000, (this.rnd() - 0.5) * 8000);
    const types: DisasterSignal['type'][] = ['flood', 'flood', 'wind', 'fire'];
    const d: DisasterSignal = {
      id: this.id('dis'),
      type: types[Math.floor(this.rnd() * types.length)],
      lat: p.lat,
      lon: p.lon,
      reported_at: new Date().toISOString(),
      status: 'active',
      resolved_at: null,
    };
    (d as any)._until = Date.now() + (120 + this.rnd() * 180) * 1000;
    this.disasters.set(d.id, d);
    this.push('disaster.created', d.id, 'Sinyal bencana baru (PetaBencana)');
  }

  private resolveDisasters() {
    const now = Date.now();
    for (const d of this.disasters.values()) {
      if (d.status === 'active' && now >= (d as any)._until) {
        d.status = 'resolved';
        d.resolved_at = new Date().toISOString();
        this.push('disaster.resolved', d.id, 'Sinyal bencana selesai');
      }
      if (d.status === 'resolved' && Date.parse(d.resolved_at!) < now - 600000) this.disasters.delete(d.id);
    }
  }

  // ---------------------------------------------------------------- risk (real rules)
  private recomputeRisk(initial = false) {
    if (this.upstreamDown && !initial) return; // keep last good grid while a source is "down"
    const now = Date.now();
    const verified = new Map<string, StoredReport[]>();
    for (const r of this.reports.values()) {
      // Invariants: a waiting report never raises risk; an expired one never counts.
      if (r.status !== 'verified' || Date.parse(r.expires_at) <= now) continue;
      const c = cellOf(r).id;
      if (!verified.has(c)) verified.set(c, []);
      verified.get(c)!.push(r);
    }
    const disasterCells = new Map<string, number>();
    for (const d of this.disasters.values()) {
      if (d.status !== 'active') continue;
      const { i, j } = cellOf(d);
      for (let di = -1; di <= 1; di++)
        for (let dj = -1; dj <= 1; dj++) {
          const id = `c_${i + di}_${j + dj}`;
          disasterCells.set(id, (disasterCells.get(id) ?? 0) + 1);
        }
    }
    let transitions = 0;
    const raised: { id: string; center: Point; level: RiskLevel }[] = [];
    for (let i = 0; i < ROWS; i++) {
      for (let j = 0; j < COLS; j++) {
        const id = `c_${i}_${j}`;
        const center = { lat: BOUNDS.minLat + (i + 0.5) * D_LAT, lon: BOUNDS.minLon + (j + 0.5) * D_LON };
        const rain = this.rainAt(center);
        const reps = verified.get(id) ?? [];
        const dis = disasterCells.get(id) ?? 0;
        let score = Math.min(rain / RULES.rainDivisor, 1) * RULES.rainWeight + dis * RULES.disasterWeight;
        for (const r of reps) {
          const total = Date.parse(r.expires_at) - Date.parse(r.verified_at ?? r.created_at);
          const left = Date.parse(r.expires_at) - now;
          const decay = total > 0 ? Math.max(0, Math.min(1, left / total)) : 0;
          score += r.severity * RULES.reportWeight * decay;
        }
        const level: RiskLevel = score >= RULES.dangerFrom ? 'danger' : score >= RULES.cautionFrom ? 'caution' : 'safe';
        const factors: RiskFactor[] = [
          { type: 'rain', value: Math.round(rain * 10) / 10 },
          { type: 'verified_report', count: reps.length },
          { type: 'disaster_signal', count: dis },
        ];
        const prev = this.cells.get(id);
        this.cells.set(id, { cell_id: id, risk_level: level, score: Math.round(score * 100) / 100, factors });
        if (!initial && prev && prev.risk_level !== level) {
          transitions += 1;
          this.push('risk.changed', id, `Risiko berubah: ${RISK_LABEL[level]}`);
          if (RISK_ORDER[level] > RISK_ORDER[prev.risk_level]) raised.push({ id, center, level });
        }
      }
    }
    if (transitions > 0) this.lastRiskTransition = now;
    this.alertSubscribers(raised);
  }

  /** At most one alert per watched area per recompute: the worst cell that got riskier inside it. */
  private alertSubscribers(raised: { id: string; center: Point; level: RiskLevel }[]) {
    for (const [user, list] of this.subs) {
      for (const s of list) {
        const inside = raised.filter((r) => r.level !== 'safe' && distanceM(s, r.center) <= s.radius_m + 250);
        if (inside.length === 0) continue;
        inside.sort((a, b) => RISK_ORDER[b.level] - RISK_ORDER[a.level] || distanceM(s, a.center) - distanceM(s, b.center));
        const { id: cellId, center, level } = inside[0];
        const street = nearestStreet(center);
        const a: Alert = {
          id: this.id('alr'),
          subscription_id: s.id,
          cell_id: cellId,
          risk_level: level,
          at: new Date().toISOString(),
          message: `${RISK_LABEL[level]} di sekitar ${s.name}${street ? ', dekat ' + street : ''}.`,
        };
        const arr = this.alerts.get(user) ?? [];
        arr.unshift(a);
        this.alerts.set(user, arr.slice(0, 100));
      }
    }
  }

  // ---------------------------------------------------------------- weather
  private weatherAt(p: Point): Weather {
    const rain = this.rainAt(p);
    const forecast: ForecastPoint[] = [];
    const base = new Date();
    base.setMinutes(0, 0, 0);
    for (let h = 1; h <= 6; h++) {
      const r = this.rainAt(p, h * 60);
      forecast.push({
        at: new Date(base.getTime() + h * 3600000).toISOString(),
        rain_mm_h: Math.round(r * 10) / 10,
        temperature_c: Math.round((30 - r * 0.15) * 10) / 10,
      });
    }
    return {
      observed_at: this.updatedAt,
      temperature_c: Math.round((this.temp + (this.storm.base - rain) * 0.1) * 10) / 10,
      rain_mm_h: Math.round(rain * 10) / 10,
      condition: condition(rain),
      forecast_6h: forecast,
    };
  }

  // ---------------------------------------------------------------- routing (real, from current risk)
  private safeRoutes(origin: Point, dest: Point, alternatives: number): { recommended_id: string; routes: Route[] } {
    const straight = distanceM(origin, dest);
    const mid = { lat: (origin.lat + dest.lat) / 2, lon: (origin.lon + dest.lon) / 2 };
    const dx = (dest.lon - origin.lon) * M_PER_DEG_LON;
    const dy = (dest.lat - origin.lat) * M_PER_DEG_LAT;
    const len = Math.max(1, Math.hypot(dx, dy));
    const nx = -dy / len;
    const ny = dx / len;
    const bend = Math.max(400, straight * 0.3);
    const candidates: Point[][] = [
      [origin, { lat: dest.lat, lon: origin.lon }, dest],
      [origin, { lat: origin.lat, lon: dest.lon }, dest],
      [origin, offset(mid, nx * bend, ny * bend), dest],
      [origin, offset(mid, -nx * bend, -ny * bend), dest],
    ];
    const routes: (Route & { cost: number; worstCell: string | null })[] = candidates.map((pts, k) => {
      const dense = densify(pts, 50);
      let dist = 0;
      let penalty = 0;
      let worst: RiskLevel = 'safe';
      let worstCell: string | null = null;
      const passed = new Set<string>();
      for (let q = 1; q < dense.length; q++) {
        const step = distanceM(dense[q - 1], dense[q]);
        dist += step;
        const c = this.cells.get(cellOf(dense[q]).id);
        if (c && c.risk_level !== 'safe') {
          penalty += c.score * (step / 1000);
          passed.add(c.cell_id);
          if (RISK_ORDER[c.risk_level] > RISK_ORDER[worst] || !worstCell) {
            worst = RISK_ORDER[c.risk_level] > RISK_ORDER[worst] ? c.risk_level : worst;
            worstCell = c.cell_id;
          }
        }
      }
      const roadDist = dist * 1.25;
      const duration = roadDist / 6.5; // ~23 km/h city traffic
      const reasons: string[] = [];
      if (passed.size > 0) {
        reasons.push(`Melewati ${passed.size} area ${RISK_LABEL[worst]}${worstCell ? this.causeNear(worstCell) : ''}.`);
      }
      return {
        id: `r${k + 1}`,
        distance_m: Math.round(roadDist),
        duration_s: Math.round(duration),
        risk_penalty: Math.round(penalty * 100) / 100,
        max_risk_level: worst,
        geometry: { type: 'LineString', coordinates: dense.map((p) => [round6(p.lon), round6(p.lat)] as [number, number]) },
        reasons,
        cost: duration * (1 + 3 * penalty) + (worst === 'danger' ? 3600 : 0),
        worstCell,
      };
    });
    // drop near-duplicates, keep the best `alternatives`
    const uniq: typeof routes = [];
    for (const r of routes.sort((a, b) => a.cost - b.cost)) {
      if (uniq.some((u) => Math.abs(u.distance_m - r.distance_m) < 60 && u.max_risk_level === r.max_risk_level)) continue;
      uniq.push(r);
    }
    const chosen = uniq.slice(0, Math.max(1, Math.min(3, alternatives)));
    const rec = chosen[0];
    const fastest = [...chosen].sort((a, b) => a.duration_s - b.duration_s)[0];
    if (rec.reasons.length === 0) {
      if (fastest !== rec && fastest.worstCell) rec.reasons.push(`Menghindari area ${RISK_LABEL[fastest.max_risk_level]}${this.causeNear(fastest.worstCell)}.`);
      else rec.reasons.push('Tidak melewati area Waspada atau Bahaya.');
    }
    for (const r of chosen) if (r.reasons.length === 0) r.reasons.push('Tidak melewati area Waspada atau Bahaya.');
    return {
      recommended_id: rec.id,
      routes: chosen.map(({ cost, worstCell, ...r }) => r),
    };
  }

  private causeNear(cellId: string): string {
    const c = this.cells.get(cellId);
    const center = cellCenter(cellId);
    if (!c || !center) return '';
    const street = nearestStreet(center);
    const rep = this.liveReports().find((r) => r.status === 'verified' && cellOf(r).id === cellId);
    const what = rep
      ? CATEGORIES[rep.category].cause.toLowerCase()
      : (c.factors.find((f) => f.type === 'disaster_signal')?.count ?? 0) > 0
        ? 'sinyal bencana'
        : 'hujan lebat';
    return ` (${what}${street ? ' di ' + street : ''})`;
  }

  // ---------------------------------------------------------------- scenarios
  runScenario(name: MockScenario, lat?: number, lon?: number) {
    const at = lat !== undefined && lon !== undefined ? { lat, lon } : undefined;
    this.scenario = name;
    switch (name) {
      case 'rain_storm': {
        const p = at ?? offset(MOCK_ME, (this.rnd() - 0.5) * 2500, (this.rnd() - 0.5) * 2500);
        this.storm = { lat: p.lat, lon: p.lon, base: 55, until: Date.now() + 120000 };
        this.push('weather.updated', 'weather', 'Hujan badai');
        break;
      }
      case 'flood_verified': {
        const r = this.newReport('flood', at ?? this.randomPlaceOnStreet(MOCK_ME, 400), 3);
        if (r) for (const u of ['user_01', 'user_02', 'user_03', 'user_04', 'user_05']) this.applyVote(r, u, 'confirm');
        if (r && r.status !== 'verified') {
          r.status = 'verified';
          r.verified_at = new Date().toISOString();
          this.push('report.verified', r.id, 'Banjir dipastikan warga');
        }
        break;
      }
      case 'road_closure': {
        const r = this.newReport('road_closure', at ?? this.randomPlaceOnStreet(MOCK_ME, 1500), 2);
        if (r) {
          r.status = 'verified';
          r.verified_at = new Date().toISOString();
          r.confirm_count = 4;
          this.push('report.verified', r.id, 'Penutupan jalan dipastikan warga');
        }
        break;
      }
      case 'event_crowd':
        this.newReport('event_crowd', at ?? this.randomPlaceOnStreet(MOCK_ME, 1500), 2);
        break;
      case 'disaster_signal':
        this.disasterStep(at ?? offset(MOCK_ME, 600, -400));
        break;
      case 'upstream_down':
        this.upstreamDownUntil = Date.now() + 30000;
        break;
      case 'recover':
        this.upstreamDownUntil = 0;
        this.storm.base = 0;
        this.storm.until = 0;
        this.scenario = null;
        this.push('weather.updated', 'weather', 'Cuaca pulih');
        break;
      case 'reset':
        this.reports.clear();
        this.votes.clear();
        this.disasters.clear();
        this.seeded.clear();
        this.upstreamDownUntil = 0;
        this.scenario = null;
        this.rnd = mulberry32(this.seed);
        this.init();
        break;
    }
    this.recomputeRisk();
  }

  // ---------------------------------------------------------------- HTTP-like router
  handle(req: Req): Resp {
    const { method, path, query } = req;
    const ok = (data: unknown, stale = false, status = 200, extra: Partial<Meta> = {}): Resp => ({
      status,
      body: { data, meta: this.meta(stale, extra) },
    });
    const err = (status: number, code: string, message: string): Resp => ({
      status,
      body: { error: { code, message, request_id: this.id('req') } },
    });
    const user = req.token ? this.tokens.get(req.token) : undefined;
    const needUser = () => (user ? null : err(401, 'invalid_api_key', 'Token tidak valid'));
    const point = (): Point | null => {
      const lat = Number(query.lat);
      const lon = Number(query.lon);
      if (!Number.isFinite(lat) || !Number.isFinite(lon) || Math.abs(lat) > 90 || Math.abs(lon) > 180) return null;
      return { lat, lon };
    };
    const limit = Math.min(200, Math.max(1, Number(query.limit ?? 50) || 50));
    const offsetN = Math.max(0, Number(query.offset ?? 0) || 0);
    const stale = this.upstreamDown;

    if (method === 'GET' && path === '/health') return { status: 200, body: { status: 'ok' } };

    if (method === 'GET' && path === '/status') {
      const p = point();
      if (!p) return err(422, 'validation_error', 'lat dan lon wajib');
      const radius = Number(query.radius_m ?? 1000) || 1000;
      const cell = this.cells.get(cellOf(p).id)!;
      const active = this.liveReports().filter((r) => r.status === 'verified' && distanceM(p, r) <= radius).length;
      const dis = [...this.disasters.values()].filter((d) => d.status === 'active' && distanceM(p, d) <= radius).length;
      return ok({ cell_id: cell.cell_id, risk_level: cell.risk_level, weather: this.weatherAt(p), active_reports: active, disaster_signals: dis }, stale);
    }

    if (method === 'GET' && path === '/risk') {
      if (query.bbox) {
        const b = parseBbox(query.bbox);
        if (!b || query.lat) return err(422, 'validation_error', 'Gunakan bbox atau lat+lon, bukan keduanya');
        const out: CellRisk[] = [];
        for (const c of this.cells.values()) {
          const center = cellCenter(c.cell_id)!;
          if (inBbox(center, b)) out.push(c);
        }
        return ok(out, stale);
      }
      const p = point();
      if (!p) return err(422, 'validation_error', 'lat dan lon wajib');
      return ok(this.cells.get(cellOf(p).id), stale);
    }

    if (method === 'GET' && path === '/weather') {
      const p = point();
      if (!p) return err(422, 'validation_error', 'lat dan lon wajib');
      return ok(this.weatherAt(p), stale);
    }

    if (method === 'GET' && path === '/disasters') {
      const b = query.bbox ? parseBbox(query.bbox) : null;
      if (query.bbox && !b) return err(422, 'validation_error', 'bbox tidak valid');
      const out = [...this.disasters.values()].filter((d) => !b || inBbox(d, b)).map(({ _until, ...d }: any) => d);
      return ok(out, stale);
    }

    if (method === 'GET' && path === '/reports') {
      const b = query.bbox ? parseBbox(query.bbox) : null;
      if (query.bbox && !b) return err(422, 'validation_error', 'bbox tidak valid');
      let out = [...this.reports.values()];
      if (query.category) out = out.filter((r) => r.category === query.category);
      if (query.status) out = out.filter((r) => r.status === query.status);
      if (b) out = out.filter((r) => inBbox(r, b));
      if (query.since) out = out.filter((r) => r.created_at >= query.since!);
      out.sort((a, c) => c.created_at.localeCompare(a.created_at));
      return ok(out.slice(offsetN, offsetN + limit).map(publicReport));
    }

    if (method === 'POST' && path === '/reports') {
      const deny = needUser();
      if (deny) return deny;
      const b = req.body ?? {};
      if (!CATEGORIES[b.category as ReportCategory] || !Number.isFinite(b.lat) || !Number.isFinite(b.lon)) {
        return err(422, 'validation_error', 'category, lat, lon wajib');
      }
      const sev = b.severity === undefined ? 2 : Number(b.severity);
      if (![1, 2, 3].includes(sev)) return err(422, 'validation_error', 'severity 1 sampai 3');
      const r = this.makeReport(b.category, { lat: b.lat, lon: b.lon }, sev, user!, Date.now(), typeof b.description === 'string' ? b.description.slice(0, 500) : undefined);
      this.reports.set(r.id, r);
      this.push('report.created', r.id, `Laporan baru: ${CATEGORIES[r.category].label}`);
      return ok(publicReport(r), false, 201);
    }

    const voteMatch = /^\/reports\/([^/]+)\/votes$/.exec(path);
    if (method === 'POST' && voteMatch) {
      const deny = needUser();
      if (deny) return deny;
      const r = this.reports.get(voteMatch[1]);
      if (!r) return err(404, 'not_found', 'Laporan tidak ditemukan');
      const v = req.body?.vote;
      if (v !== 'confirm' && v !== 'dispute') return err(422, 'validation_error', 'vote harus confirm atau dispute');
      if (r.reporter === user) return err(403, 'forbidden', 'Tidak bisa memastikan laporan sendiri');
      if (r.status === 'expired') return err(403, 'forbidden', 'Laporan sudah kedaluwarsa');
      this.applyVote(r, user!, v);
      this.recomputeRisk();
      return ok(publicReport(r));
    }

    if (method === 'POST' && path === '/routes/safe') {
      const b = req.body ?? {};
      const o = b.origin;
      const d = b.destination;
      if (!o || !d || !Number.isFinite(o.lat) || !Number.isFinite(o.lon) || !Number.isFinite(d.lat) || !Number.isFinite(d.lon)) {
        return err(422, 'validation_error', 'origin dan destination wajib');
      }
      if (stale) return err(503, 'upstream_unavailable', 'Layanan rute sedang tidak tersedia');
      return ok(this.safeRoutes(o, d, Number(b.alternatives ?? 3) || 3));
    }

    if (method === 'GET' && path === '/changes') {
      const since = Number(query.since);
      if (!Number.isFinite(since) || since < 0) return err(422, 'validation_error', 'since wajib');
      const oldest = this.changes.length ? this.changes[0].seq : this.seq + 1;
      if (since < oldest - 1 && since < this.seq) return ok([], false, 200, { reset: true });
      return ok(this.changes.filter((c) => c.seq > since).slice(0, limit));
    }

    // GAP: not in openapi.yaml yet (proposed additive change, see README).
    if (method === 'POST' && path === '/auth/sso') {
      const t = req.body?.id_token;
      if (typeof t !== 'string' || !t) return err(422, 'validation_error', 'id_token wajib');
      const uid = 'user_app';
      this.users.set(uid, 0.6);
      const token = `mock.${this.id('jwt')}`;
      this.tokens.set(token, uid);
      return { status: 200, body: { data: { access_token: token, token_type: 'bearer', expires_in: 86400 } } };
    }

    if (method === 'GET' && path === '/subscriptions') {
      const deny = needUser();
      if (deny) return deny;
      return ok((this.subs.get(user!) ?? []).slice(offsetN, offsetN + limit));
    }

    if (method === 'POST' && path === '/subscriptions') {
      const deny = needUser();
      if (deny) return deny;
      const b = req.body ?? {};
      if (typeof b.name !== 'string' || !b.name.trim() || b.name.length > 60 || !Number.isFinite(b.lat) || !Number.isFinite(b.lon)) {
        return err(422, 'validation_error', 'name, lat, lon wajib');
      }
      const radius = b.radius_m === undefined ? 1000 : Number(b.radius_m);
      if (!(radius >= 100 && radius <= 5000)) return err(422, 'validation_error', 'radius_m 100 sampai 5000');
      const s: Subscription = { id: this.id('sub'), name: b.name.trim(), lat: b.lat, lon: b.lon, radius_m: radius, created_at: new Date().toISOString() };
      this.subs.set(user!, [...(this.subs.get(user!) ?? []), s]);
      return ok(s, false, 201);
    }

    // GAP: DELETE /subscriptions/{id} is not in openapi.yaml yet.
    const subDel = /^\/subscriptions\/([^/]+)$/.exec(path);
    if (method === 'DELETE' && subDel) {
      const deny = needUser();
      if (deny) return deny;
      const list = this.subs.get(user!) ?? [];
      if (!list.some((s) => s.id === subDel[1])) return err(404, 'not_found', 'Area tidak ditemukan');
      this.subs.set(user!, list.filter((s) => s.id !== subDel[1]));
      return { status: 204, body: null };
    }

    if (method === 'GET' && path === '/alerts') {
      const deny = needUser();
      if (deny) return deny;
      return ok((this.alerts.get(user!) ?? []).slice(offsetN, offsetN + limit));
    }

    if (method === 'POST' && path === '/_mock/scenario') {
      const name = req.body?.name as MockScenario;
      const valid: MockScenario[] = ['rain_storm', 'flood_verified', 'road_closure', 'event_crowd', 'disaster_signal', 'upstream_down', 'recover', 'reset'];
      if (!valid.includes(name)) return err(422, 'validation_error', 'Skenario tidak dikenal');
      this.runScenario(name, req.body?.lat, req.body?.lon);
      return ok({ name });
    }

    if (method === 'GET' && path === '/_mock/state') {
      return ok({
        seed: this.seed,
        tick_seconds: config.mockTickSeconds,
        time_scale: config.mockTimeScale,
        tick: this.tickN,
        active_scenario: this.scenario,
        live_reports: this.liveReports().length,
        active_disasters: [...this.disasters.values()].filter((d) => d.status === 'active').length,
        upstreams_down: this.upstreamDown ? ['open_meteo', 'petabencana'] : [],
      });
    }

    return err(404, 'not_found', `Tidak ada ${method} ${path}`);
  }
}

function publicReport(r: StoredReport): Report {
  const { reporter, ...rest } = r;
  return rest;
}

function condition(rain: number): WeatherCondition {
  if (rain < 0.2) return 'clear';
  if (rain < 1) return 'cloudy';
  if (rain < 10) return 'rain';
  if (rain < 35) return 'heavy_rain';
  return 'storm';
}

function round6(n: number) {
  return Math.round(n * 1e6) / 1e6;
}

function densify(pts: Point[], stepM: number): Point[] {
  const out: Point[] = [pts[0]];
  for (let k = 1; k < pts.length; k++) {
    const a = pts[k - 1];
    const b = pts[k];
    const n = Math.max(1, Math.ceil(distanceM(a, b) / stepM));
    for (let s = 1; s <= n; s++) out.push({ lat: a.lat + ((b.lat - a.lat) * s) / n, lon: a.lon + ((b.lon - a.lon) * s) / n });
  }
  return out;
}

