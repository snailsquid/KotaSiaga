// The one place the app talks to backends (AGENTS.md §7 mobile). Base URL comes from env.
// Resilience per AGENTS.md §6: 5 s timeout, one retry with jitter on timeout or 5xx, never on 4xx,
// last good response per endpoint, served with meta.stale=true when the call fails.

import * as S from './sanitize';
import type {
  Alert,
  Bbox,
  CellRisk,
  Change,
  DisasterSignal,
  Meta,
  MockScenario,
  Point,
  Report,
  ReportCategory,
  ReportCreate,
  ReportStatus,
  SafeRouteResult,
  Status,
  Subscription,
  SubscriptionCreate,
  Token,
  Vote,
  Weather,
} from './types';
import { NetworkError, makeTransport, MockTransport, type Transport, type TransportRequest } from './transport';
import { bboxParam, distanceM } from '../domain/geo';
import { PLACES, nearestDistrict, nearestStreet, type PlaceEntry, type PlaceKind } from './mock/gazetteer';
import { cellCenter as mockCellCenter, cellOf as mockCellOf } from './mock/MockBackend';

const TIMEOUT_MS = 5000;

export type ApiErrorCode =
  | 'invalid_api_key'
  | 'forbidden'
  | 'not_found'
  | 'validation_error'
  | 'rate_limited'
  | 'upstream_unavailable'
  | 'network'
  | 'timeout'
  | 'contract';

export class ApiError extends Error {
  constructor(
    readonly code: ApiErrorCode,
    message: string,
    readonly status = 0,
    readonly retryAfter?: number,
  ) {
    super(message);
  }
}

export interface Result<T> {
  data: T;
  meta: Meta;
}

interface CacheEntry {
  at: number;
  value: Result<any>;
}

type Listener = (s: { online: boolean }) => void;

class ApiServiceImpl {
  private transport: Transport = makeTransport();
  private cache = new Map<string, CacheEntry>();
  private token: string | null = null;
  private listeners = new Set<Listener>();
  online = true;

  setToken(t: string | null) {
    this.token = t;
  }

  get isMock() {
    return this.transport instanceof MockTransport;
  }

  onConnectivity(fn: Listener) {
    this.listeners.add(fn);
    return () => this.listeners.delete(fn);
  }

  private setOnline(v: boolean) {
    if (this.online === v) return;
    this.online = v;
    this.listeners.forEach((l) => l({ online: v }));
  }

  private log(req: TransportRequest, status: number | string, ms: number) {
    // One line per call: timestamp, service, path, status, ms (AGENTS.md §6 "bukti").
    console.log(`${new Date().toISOString()} ${this.transport.name} ${req.method} ${req.path} ${status} ${ms}ms`);
  }

  private async once(req: TransportRequest) {
    const ctrl = new AbortController();
    const timer = setTimeout(() => ctrl.abort(), TIMEOUT_MS);
    const t0 = Date.now();
    try {
      const res = await this.transport.send({ ...req, signal: ctrl.signal });
      this.log(req, res.status, Date.now() - t0);
      return res;
    } catch (e) {
      const timedOut = ctrl.signal.aborted;
      this.log(req, timedOut ? 'timeout' : 'network', Date.now() - t0);
      throw timedOut ? new ApiError('timeout', 'Waktu habis') : e instanceof NetworkError ? new ApiError('network', 'Tidak ada koneksi') : e;
    } finally {
      clearTimeout(timer);
    }
  }

  private async send(req: TransportRequest) {
    try {
      let res;
      try {
        res = await this.once(req);
        if (res.status >= 500) throw new ApiError('upstream_unavailable', 'Server bermasalah', res.status);
      } catch (e) {
        if (!(e instanceof ApiError) || !['timeout', 'network', 'upstream_unavailable'].includes(e.code)) throw e;
        // One retry with jitter. Only for idempotent calls: a retried POST /reports could create a duplicate.
        const idempotent = req.method === 'GET' || req.path === '/routes/safe';
        if (!idempotent) throw e;
        await new Promise((r) => setTimeout(r, 300 + Math.random() * 700));
        res = await this.once(req);
      }
      this.setOnline(true);
      if (res.status >= 200 && res.status < 300) return res;
      const code = res.body?.error?.code as ApiErrorCode | undefined;
      throw new ApiError(code ?? (res.status >= 500 ? 'upstream_unavailable' : 'validation_error'), res.body?.error?.message ?? `HTTP ${res.status}`, res.status, res.retryAfter);
    } catch (e) {
      if (e instanceof ApiError && e.code === 'network') this.setOnline(false);
      throw e;
    }
  }

  /** GET with last-good cache; on failure returns cached data marked stale. */
  private async get<T>(path: string, query: TransportRequest['query'], parse: (d: any) => T, opts: { auth?: boolean } = {}): Promise<Result<T>> {
    const key = path + JSON.stringify(query ?? {});
    try {
      const res = await this.send({ method: 'GET', path, query, token: opts.auth ? this.token ?? undefined : undefined });
      let value: Result<T>;
      try {
        value = { data: parse(res.body?.data), meta: S.meta(res.body?.meta) };
      } catch (e) {
        throw new ApiError('contract', String((e as Error).message));
      }
      this.cache.set(key, { at: Date.now(), value });
      if (this.cache.size > 200) this.cache.delete(this.cache.keys().next().value!);
      return value;
    } catch (e) {
      const hit = this.cache.get(key);
      const err = e as ApiError;
      const recoverable = err instanceof ApiError && ['network', 'timeout', 'upstream_unavailable', 'rate_limited', 'contract'].includes(err.code);
      if (hit && recoverable) return { data: hit.value.data, meta: { ...hit.value.meta, stale: true } };
      throw e;
    }
  }

  private async write<T>(method: 'POST' | 'DELETE', path: string, body: unknown, parse: (d: any) => T, auth = true): Promise<T> {
    const res = await this.send({ method, path, body, token: auth ? this.token ?? undefined : undefined });
    if (res.status === 204) return undefined as T;
    try {
      return parse(res.body?.data);
    } catch (e) {
      throw new ApiError('contract', String((e as Error).message));
    }
  }

  // ---------------------------------------------------------------- contract endpoints (openapi.yaml)
  status(p: Point, radius_m = 1000) {
    return this.get<Status>('/status', { lat: p.lat, lon: p.lon, radius_m }, S.status);
  }
  riskAt(p: Point) {
    return this.get<CellRisk>('/risk', { lat: p.lat, lon: p.lon }, S.cellRisk);
  }
  riskInBox(b: Bbox) {
    return this.get<CellRisk[]>('/risk', { bbox: bboxParam(b) }, S.list(S.cellRisk));
  }
  weather(p: Point) {
    return this.get<Weather>('/weather', { lat: p.lat, lon: p.lon }, S.weather);
  }
  disasters(b: Bbox) {
    return this.get<DisasterSignal[]>('/disasters', { bbox: bboxParam(b) }, S.list(S.disaster));
  }
  reports(q: { bbox?: Bbox; category?: ReportCategory; status?: ReportStatus; since?: string; limit?: number; offset?: number }) {
    return this.get<Report[]>(
      '/reports',
      { bbox: q.bbox ? bboxParam(q.bbox) : undefined, category: q.category, status: q.status, since: q.since, limit: q.limit ?? 200, offset: q.offset },
      S.list(S.report),
    );
  }
  changes(since: number, limit = 200) {
    return this.get<Change[]>('/changes', { since, limit }, S.list(S.change));
  }
  safeRoute(origin: Point, destination: Point, alternatives = 3) {
    return this.write<SafeRouteResult>('POST', '/routes/safe', { origin, destination, alternatives }, S.safeRoute, false);
  }
  createReport(body: ReportCreate) {
    return this.write<Report>('POST', '/reports', body, S.report);
  }
  vote(id: string, vote: Vote) {
    return this.write<Report>('POST', `/reports/${encodeURIComponent(id)}/votes`, { vote }, S.report);
  }
  subscriptions() {
    return this.get<Subscription[]>('/subscriptions', { limit: 200 }, S.list(S.subscription), { auth: true });
  }
  createSubscription(body: SubscriptionCreate) {
    return this.write<Subscription>('POST', '/subscriptions', body, S.subscription);
  }
  alerts() {
    return this.get<Alert[]>('/alerts', { limit: 100 }, S.list(S.alert), { auth: true });
  }

  // ---------------------------------------------------------------- not in the contract yet (GAP, see README)
  /** Proposed POST /auth/sso: Google id_token in, our JWT out. */
  authSso(id_token: string) {
    return this.write<Token>('POST', '/auth/sso', { id_token }, S.token, false);
  }
  /** Proposed DELETE /subscriptions/{id}. */
  deleteSubscription(id: string) {
    return this.write<void>('DELETE', `/subscriptions/${encodeURIComponent(id)}`, undefined, () => undefined);
  }

  // ---------------------------------------------------------------- mock only (demo controls, AGENTS.md §7)
  async mockScenario(name: MockScenario | 'offline', at?: Point) {
    if (!(this.transport instanceof MockTransport)) throw new ApiError('not_found', 'Hanya tersedia di mode MOCK');
    if (name === 'offline') {
      this.transport.offline = !this.transport.offline;
      return;
    }
    if (name === 'recover') this.transport.offline = false;
    // The real endpoint needs an admin X-API-Key; the in-app mock does not.
    await this.write('POST', '/_mock/scenario', { name, lat: at?.lat, lon: at?.lon }, (d) => d, false);
  }

  // ---------------------------------------------------------------- local stubs, no HTTP yet (GAP)
  /** Place search. No search endpoint exists; uses the mock gazetteer. */
  searchPlaces(q: string, from: Point | null): (PlaceEntry & { distance_m: number | null })[] {
    const needle = q.trim().toLowerCase();
    return PLACES.filter((p) => !needle || p.name.toLowerCase().includes(needle) || p.address.toLowerCase().includes(needle) || (p.category ?? '').toLowerCase().includes(needle))
      .map((p) => ({ ...p, distance_m: from ? distanceM(from, p) : null }))
      .sort((a, b) => (a.distance_m ?? 0) - (b.distance_m ?? 0));
  }
  /** Tourism destinations (JelajahLokal feed) and donation points (DonorKita) for map layers. */
  placesOfKind(kind: PlaceKind): PlaceEntry[] {
    return PLACES.filter((p) => p.kind === kind);
  }
  /**
   * Where a risk cell is. CellRisk has no geometry in the contract and cell_id is opaque, so this
   * reads the mock grid's "c_<row>_<col>" ids. Needs a contract field (e.g. center lat/lon).
   */
  cellCenter(cellId: string): Point | null {
    return mockCellCenter(cellId);
  }
  cellAt(p: Point): string {
    return mockCellOf(p).id;
  }
  /** Reverse geocoding: street and district names for a point. */
  describe(p: Point): { street: string | null; district: string } {
    return { street: nearestStreet(p), district: nearestDistrict(p) };
  }
}

export const ApiService = new ApiServiceImpl();
export type { PlaceEntry };
