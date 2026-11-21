import { config } from '../config';
import { MockBackend } from './mock/MockBackend';

export interface TransportRequest {
  method: 'GET' | 'POST' | 'DELETE';
  path: string; // relative to /api, e.g. "/reports"
  query?: Record<string, string | number | undefined>;
  body?: unknown;
  token?: string;
  signal?: AbortSignal;
}

export interface TransportResponse {
  status: number;
  body: any;
  retryAfter?: number;
}

export class NetworkError extends Error {}

export interface Transport {
  readonly name: string;
  send(req: TransportRequest): Promise<TransportResponse>;
}

function qs(query?: TransportRequest['query']): string {
  if (!query) return '';
  const parts = Object.entries(query)
    .filter(([, v]) => v !== undefined && v !== '')
    .map(([k, v]) => `${encodeURIComponent(k)}=${encodeURIComponent(String(v))}`);
  return parts.length ? `?${parts.join('&')}` : '';
}

/** Real HTTP. Not used while the backend is stubbed (EXPO_PUBLIC_API_MODE defaults to `mock`). */
export class HttpTransport implements Transport {
  readonly name = 'kotasiaga';
  constructor(private baseUrl: string) {}

  async send(req: TransportRequest): Promise<TransportResponse> {
    const headers: Record<string, string> = { Accept: 'application/json' };
    if (req.body !== undefined) headers['Content-Type'] = 'application/json';
    if (req.token) headers.Authorization = `Bearer ${req.token}`;
    let res: Response;
    try {
      res = await fetch(this.baseUrl + req.path + qs(req.query), {
        method: req.method,
        headers,
        body: req.body === undefined ? undefined : JSON.stringify(req.body),
        signal: req.signal,
      });
    } catch (e) {
      throw new NetworkError(String(e));
    }
    const text = await res.text();
    let body: any = null;
    try {
      body = text ? JSON.parse(text) : null;
    } catch {
      body = null;
    }
    const ra = res.headers.get('Retry-After');
    return { status: res.status, body, retryAfter: ra ? Number(ra) : undefined };
  }
}

/** Routes requests to the in-app mock backend with a little latency. */
export class MockTransport implements Transport {
  readonly name = 'kotasiaga-mock';
  /** Demo switch: pretend the phone is offline. */
  offline = false;
  constructor(readonly backend: MockBackend) {}

  async send(req: TransportRequest): Promise<TransportResponse> {
    await new Promise((r) => setTimeout(r, 80 + Math.random() * 180));
    if (req.signal?.aborted) throw new NetworkError('aborted');
    if (this.offline) throw new NetworkError('offline');
    const query: Record<string, string | undefined> = {};
    for (const [k, v] of Object.entries(req.query ?? {})) if (v !== undefined) query[k] = String(v);
    // Round-trip through JSON so the app never shares objects with the mock.
    const body = req.body === undefined ? undefined : JSON.parse(JSON.stringify(req.body));
    const res = this.backend.handle({ method: req.method, path: req.path, query, body, token: req.token });
    return { status: res.status, body: res.body === null ? null : JSON.parse(JSON.stringify(res.body)) };
  }
}

export function makeTransport(): Transport {
  if (config.apiMode === 'http' && config.apiBaseUrl) return new HttpTransport(config.apiBaseUrl.replace(/\/$/, ''));
  const backend = new MockBackend(config.mockSeed);
  backend.start();
  return new MockTransport(backend);
}
