import React, { createContext, useCallback, useContext, useEffect, useMemo, useReducer, useRef, useState } from 'react';
import { AppState, Share } from 'react-native';
import * as Location from 'expo-location';
import { ApiService, ApiError } from '../api/ApiService';
import type { Alert, CellRisk, DisasterSignal, Meta, Point, Report, ReportCategory, SafeRouteResult, Status, Subscription, Vote } from '../api/types';
import { MOCK_ME, BOUNDS } from '../api/mock/MockBackend';
import { config } from '../config';
import { useAuth } from '../auth/AuthProvider';
import { CATEGORIES, CONFIRM_RADIUS_M } from '../domain/categories';
import { distanceM } from '../domain/geo';
import { RISK_LABEL } from '../domain/risk';
import { initialNav, navReducer, type NavAction, type NavState } from './nav';
import type { Filters, Layers, PendingAction, PlaceRef, SheetView, Snap } from './types';

const CITY = { minLon: BOUNDS.minLon, minLat: BOUNDS.minLat, maxLon: BOUNDS.maxLon, maxLat: BOUNDS.maxLat };

export interface ToastMsg {
  id: number;
  text: string;
  action?: { label: string; onPress: () => void };
}

export interface MapApi {
  flyTo: (p: Point, zoomIn?: boolean) => void;
  fit: (points: Point[]) => void;
  center: () => Point;
}

export type LocationState = 'unknown' | 'granted' | 'denied';

interface Data {
  reports: Report[];
  cells: Map<string, CellRisk>;
  disasters: DisasterSignal[];
  status: Status | null;
  statusError: boolean;
  riskChangedAt: Map<string, string>;
  meta: Meta | null;
  loaded: boolean;
}

export interface RouteState {
  key: string;
  result: SafeRouteResult | null;
  loading: boolean;
  error: string | null;
}

interface Store {
  routes: RouteState;
  loadRoutes: (origin: Point, dest: Point) => void;
  subRadius: number;
  setSubRadius: (m: number) => void;
  /** Map center while the map moves (used by "Tambah area" and the report location picker). */
  liveCenter: Point;
  setLiveCenter: (p: Point) => void;
  nav: NavState;
  dispatch: React.Dispatch<NavAction>;
  go: (to: SheetView, snap?: Snap) => void;
  back: () => boolean;
  home: () => void;
  // data
  data: Data;
  online: boolean;
  subscriptions: Subscription[];
  alerts: Alert[];
  unread: number;
  markAlertRead: (id: string) => void;
  refreshAccountData: () => void;
  refreshAll: () => void;
  // user and ui
  location: LocationState;
  me: Point | null;
  focus: Point;
  setMapCenter: (p: Point) => void;
  requestLocation: () => Promise<boolean>;
  filters: Filters;
  setFilters: (f: Filters) => void;
  layers: Layers;
  setLayers: (l: Layers) => void;
  myVotes: Record<string, Vote>;
  myReports: Set<string>;
  dismissed: Set<string>;
  dismiss: (id: string) => void;
  recentSearches: PlaceRef[];
  addRecent: (p: PlaceRef) => void;
  removeRecent: (p: PlaceRef) => void;
  toast: ToastMsg | null;
  showToast: (text: string, action?: ToastMsg['action']) => void;
  hideToast: () => void;
  mapApi: React.MutableRefObject<MapApi | null>;
  // actions
  need: (a: PendingAction, reason: string) => void;
  runPending: (a: PendingAction) => void;
  actionError: string | null;
  clearActionError: () => void;
  busy: boolean;
  openReport: (id: string) => void;
  openPlace: (p: PlaceRef) => void;
  share: (text: string) => void;
}

const Ctx = createContext<Store | null>(null);

export function AppStoreProvider({ children }: { children: React.ReactNode }) {
  const auth = useAuth();
  const [nav, dispatch] = useReducer(navReducer, initialNav);
  const navRef = useRef(nav);
  navRef.current = nav;

  // ---------------------------------------------------------------- toast
  const [toast, setToast] = useState<ToastMsg | null>(null);
  const showToast = useCallback((text: string, action?: ToastMsg['action']) => setToast({ id: Date.now(), text, action }), []);
  const hideToast = useCallback(() => setToast(null), []);

  // ---------------------------------------------------------------- location
  const [location, setLocation] = useState<LocationState>('unknown');
  const [devicePos, setDevicePos] = useState<Point | null>(null);
  const [mapCenter, setMapCenterState] = useState<Point>(MOCK_ME);
  const me = location === 'granted' ? (config.useDeviceLocation ? devicePos : MOCK_ME) : null;
  const focus = me ?? mapCenter;

  const requestLocation = useCallback(async () => {
    try {
      const r = await Location.requestForegroundPermissionsAsync();
      setLocation(r.granted ? 'granted' : 'denied');
      return r.granted;
    } catch {
      setLocation('denied');
      return false;
    }
  }, []);

  useEffect(() => {
    // First launch only: explain before the system prompt (spec §6). Without storage we use the
    // permission state itself: undetermined means we have not asked yet.
    Location.getForegroundPermissionsAsync()
      .then((r) => {
        if (r.granted) setLocation('granted');
        else if (r.canAskAgain) dispatch({ type: 'go', to: { view: 'location_intro' } });
        else setLocation('denied');
      })
      .catch(() => setLocation('denied'));
  }, []);

  useEffect(() => {
    if (location !== 'granted' || !config.useDeviceLocation) return;
    let sub: Location.LocationSubscription | null = null;
    Location.watchPositionAsync({ accuracy: Location.Accuracy.Balanced, distanceInterval: 20 }, (pos) =>
      setDevicePos({ lat: pos.coords.latitude, lon: pos.coords.longitude }),
    )
      .then((s) => (sub = s))
      .catch(() => undefined);
    return () => sub?.remove();
  }, [location]);

  // ---------------------------------------------------------------- data
  const [data, setData] = useState<Data>({
    reports: [],
    cells: new Map(),
    disasters: [],
    status: null,
    statusError: false,
    riskChangedAt: new Map(),
    meta: null,
    loaded: false,
  });
  const dataRef = useRef(data);
  dataRef.current = data;
  const [online, setOnline] = useState(true);
  useEffect(() => {
    const off = ApiService.onConnectivity((s) => setOnline(s.online));
    return () => {
      off();
    };
  }, []);

  const seqRef = useRef(0);
  const staleRef = useRef<Record<string, boolean>>({});
  const focusRef = useRef(focus);
  focusRef.current = focus;
  const meRef = useRef(me);
  meRef.current = me;

  const applyMeta = useCallback((key: string, m: Meta) => {
    staleRef.current[key] = m.stale;
    if (m.seq > seqRef.current) seqRef.current = m.seq;
    const anyStale = Object.values(staleRef.current).some(Boolean);
    setData((d) => {
      const prev = d.meta;
      const next: Meta = {
        seq: Math.max(prev?.seq ?? 0, m.seq),
        updated_at: !prev || m.updated_at > prev.updated_at ? m.updated_at : prev.updated_at,
        source: m.source,
        stale: anyStale,
      };
      return { ...d, meta: next };
    });
  }, []);

  const loadReports = useCallback(async () => {
    try {
      const r = await ApiService.reports({ bbox: CITY, limit: 200 });
      applyMeta('reports', r.meta);
      setData((d) => ({ ...d, reports: r.data }));
    } catch {
      /* keep current data */
    }
  }, [applyMeta]);

  const loadRisk = useCallback(async () => {
    try {
      const r = await ApiService.riskInBox(CITY);
      applyMeta('risk', r.meta);
      setData((d) => ({ ...d, cells: new Map(r.data.map((c) => [c.cell_id, c])) }));
    } catch {
      setData((d) => (d.cells.size ? d : { ...d, cells: new Map() }));
    }
  }, [applyMeta]);

  const loadDisasters = useCallback(async () => {
    try {
      const r = await ApiService.disasters(CITY);
      applyMeta('disasters', r.meta);
      setData((d) => ({ ...d, disasters: r.data }));
    } catch {
      /* keep */
    }
  }, [applyMeta]);

  const loadStatus = useCallback(async () => {
    try {
      const r = await ApiService.status(focusRef.current, 1000);
      applyMeta('status', r.meta);
      setData((d) => ({ ...d, status: r.data, statusError: false }));
    } catch {
      setData((d) => ({ ...d, statusError: d.status === null }));
    }
  }, [applyMeta]);

  const refreshAll = useCallback(async () => {
    await Promise.all([loadReports(), loadRisk(), loadDisasters(), loadStatus()]);
    setData((d) => ({ ...d, loaded: true }));
  }, [loadReports, loadRisk, loadDisasters, loadStatus]);

  // ---------------------------------------------------------------- account data
  const [subscriptions, setSubscriptions] = useState<Subscription[]>([]);
  const [alerts, setAlerts] = useState<Alert[]>([]);
  const [readAlerts, setReadAlerts] = useState<Set<string>>(new Set());
  const signedInRef = useRef(auth.signedIn);
  signedInRef.current = auth.signedIn;

  const refreshAccountData = useCallback(async () => {
    if (!signedInRef.current) return;
    try {
      const [s, a] = await Promise.all([ApiService.subscriptions(), ApiService.alerts()]);
      setSubscriptions(s.data);
      setAlerts(a.data);
    } catch (e) {
      if (e instanceof ApiError && e.status === 401) auth.sessionEnded();
    }
  }, [auth]);

  useEffect(() => {
    if (auth.signedIn) refreshAccountData();
    else {
      setSubscriptions([]);
      setAlerts([]);
    }
  }, [auth.signedIn, refreshAccountData]);

  // ---------------------------------------------------------------- live updates (AGENTS.md §7 mobile)
  const myReportsRef = useRef<Set<string>>(new Set());
  const poll = useCallback(async () => {
    try {
      const r = await ApiService.changes(seqRef.current);
      if (r.meta.reset) {
        seqRef.current = r.meta.seq;
        await refreshAll();
        return;
      }
      if (r.data.length === 0) {
        applyMeta('changes', r.meta);
        return;
      }
      const types = new Set(r.data.map((c) => c.type.split('.')[0]));
      const riskChanged = r.data.filter((c) => c.type === 'risk.changed');
      if (riskChanged.length) {
        setData((d) => {
          const m = new Map(d.riskChangedAt);
          riskChanged.forEach((c) => m.set(c.entity_id, c.at));
          return { ...d, riskChangedAt: m };
        });
      }
      const prevLevel = dataRef.current.status?.risk_level;
      const jobs: Promise<unknown>[] = [];
      if (types.has('report')) jobs.push(loadReports());
      if (types.has('risk')) jobs.push(loadRisk());
      if (types.has('disaster')) jobs.push(loadDisasters());
      if (types.has('weather') || types.has('risk') || types.has('report')) jobs.push(loadStatus());
      if (types.has('risk') && signedInRef.current) jobs.push(refreshAccountData());
      await Promise.all(jobs);
      applyMeta('changes', r.meta);
      seqRef.current = Math.max(seqRef.current, r.data[r.data.length - 1].seq);

      // Toasts (spec §9). Updates never move the camera or close what is open.
      const nowLevel = dataRef.current.status?.risk_level;
      if (prevLevel && nowLevel && prevLevel !== nowLevel && meRef.current) {
        showToast(`Risiko di sekitarmu berubah: ${RISK_LABEL[nowLevel]}`);
        return;
      }
      const here = meRef.current;
      if (here) {
        for (const c of r.data) {
          if (c.type !== 'report.created' || myReportsRef.current.has(c.entity_id)) continue;
          const rep = dataRef.current.reports.find((x) => x.id === c.entity_id);
          if (rep && distanceM(here, rep) <= 1000) {
            showToast(`Laporan baru di dekatmu: ${rep.description ?? CATEGORIES[rep.category].title}`);
            break;
          }
        }
      }
    } catch {
      setData((d) => (d.meta ? { ...d, meta: { ...d.meta, stale: true } } : d));
    }
  }, [applyMeta, loadDisasters, loadReports, loadRisk, loadStatus, refreshAccountData, refreshAll, showToast]);

  useEffect(() => {
    refreshAll();
  }, [refreshAll]);

  useEffect(() => {
    // Poll while the app is in the foreground; stop in background (no SSE in React Native).
    let timer: ReturnType<typeof setInterval> | null = null;
    const start = () => {
      if (!timer) timer = setInterval(poll, config.pollSeconds * 1000);
    };
    const stop = () => {
      if (timer) clearInterval(timer);
      timer = null;
    };
    if (AppState.currentState === 'active') start();
    const sub = AppState.addEventListener('change', (st) => {
      if (st === 'active') {
        poll();
        start();
      } else stop();
    });
    return () => {
      stop();
      sub.remove();
    };
  }, [poll]);

  // Refetch the summary when the focus point moves (map stopped, or the user moved).
  const lastFocus = useRef(focus);
  useEffect(() => {
    if (distanceM(lastFocus.current, focus) < 200) return;
    lastFocus.current = focus;
    loadStatus();
  }, [focus, loadStatus]);

  // ---------------------------------------------------------------- ui state
  const [filters, setFilters] = useState<Filters>({ cats: [], status: 'all' });
  const [layers, setLayers] = useState<Layers>({ risk: true, reports: true, disasters: true, tourism: false, donor: false });
  const [myVotes, setMyVotes] = useState<Record<string, Vote>>({});
  const [myReports, setMyReports] = useState<Set<string>>(new Set());
  myReportsRef.current = myReports;
  const [dismissed, setDismissed] = useState<Set<string>>(new Set());
  const [recentSearches, setRecent] = useState<PlaceRef[]>([]);
  const [actionError, setActionError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const mapApi = useRef<MapApi | null>(null);
  const [subRadius, setSubRadius] = useState(1000);
  const [liveCenter, setLiveCenter] = useState<Point>(MOCK_ME);

  // ---------------------------------------------------------------- routes
  const [routes, setRoutes] = useState<RouteState>({ key: '', result: null, loading: false, error: null });
  const routeKeyRef = useRef('');
  const loadRoutes = useCallback(async (origin: Point, dest: Point) => {
    const key = `${origin.lat.toFixed(5)},${origin.lon.toFixed(5)}>${dest.lat.toFixed(5)},${dest.lon.toFixed(5)}`;
    routeKeyRef.current = key;
    setRoutes((r) => ({ key, result: r.key === key ? r.result : null, loading: true, error: null }));
    try {
      const result = await ApiService.safeRoute(origin, dest, 3);
      if (routeKeyRef.current === key) setRoutes({ key, result, loading: false, error: null });
    } catch (e) {
      if (routeKeyRef.current !== key) return;
      const msg = e instanceof ApiError && e.code === 'rate_limited' ? 'Coba lagi sebentar lagi.' : 'Rute aman belum tersedia';
      setRoutes({ key, result: null, loading: false, error: msg });
    }
  }, []);

  const go = useCallback((to: SheetView, snap?: Snap) => dispatch({ type: 'go', to, snap }), []);
  const home = useCallback(() => dispatch({ type: 'home' }), []);
  const back = useCallback(() => {
    const n = navRef.current;
    if (n.current.view === 'home') {
      if (n.current.snap > 0) {
        dispatch({ type: 'snap', snap: 0 });
        return true;
      }
      return false; // leave the app
    }
    dispatch({ type: 'back' });
    return true;
  }, []);

  const keyOf = (p: PlaceRef) => p.id ?? `${p.lat.toFixed(5)},${p.lon.toFixed(5)}`;
  const addRecent = useCallback((p: PlaceRef) => setRecent((r) => [p, ...r.filter((x) => keyOf(x) !== keyOf(p))].slice(0, 10)), []);
  const removeRecent = useCallback((p: PlaceRef) => setRecent((r) => r.filter((x) => keyOf(x) !== keyOf(p))), []);

  const openReport = useCallback(
    (id: string) => {
      const r = dataRef.current.reports.find((x) => x.id === id);
      go({ view: 'report', id });
      if (r) mapApi.current?.flyTo(r);
    },
    [go],
  );

  const openPlace = useCallback(
    (p: PlaceRef) => {
      go({ view: 'place', place: p });
      mapApi.current?.flyTo(p);
    },
    [go],
  );

  const share = useCallback(
    (text: string) => {
      Share.share({ message: text }).catch(() => showToast('Gagal membuka menu bagikan'));
    },
    [showToast],
  );

  // ---------------------------------------------------------------- gated actions (spec §8)
  const handleActionError = useCallback(
    (e: unknown, pending: PendingAction, reason: string) => {
      if (e instanceof ApiError && e.status === 401) {
        // Session ended: show sign-in only now, when the user tries a gated action.
        auth.sessionEnded();
        dispatch({ type: 'go', to: { view: 'signin', reason, pending } });
        return;
      }
      if (e instanceof ApiError && e.code === 'rate_limited') setActionError('Coba lagi sebentar lagi.');
      else if (e instanceof ApiError && e.code === 'forbidden') setActionError(e.message);
      else setActionError('Gagal mengirim. Periksa koneksi lalu coba lagi.');
    },
    [auth],
  );

  const runPending = useCallback(
    async (a: PendingAction) => {
      setActionError(null);
      switch (a.type) {
        case 'vote': {
          setBusy(true);
          try {
            const r = await ApiService.vote(a.id, a.vote);
            setMyVotes((v) => ({ ...v, [a.id]: a.vote }));
            setData((d) => ({ ...d, reports: d.reports.map((x) => (x.id === r.id ? r : x)) }));
            showToast('Terima kasih! Jawabanmu membantu warga lain.');
            if (a.then === 'report') dispatch({ type: 'go', to: { view: 'report', id: a.id } });
          } catch (e) {
            handleActionError(e, a, 'ikut memastikan laporan');
          } finally {
            setBusy(false);
          }
          break;
        }
        case 'submit': {
          setBusy(true);
          try {
            const r = await ApiService.createReport({ category: a.cat, lat: a.at.lat, lon: a.at.lon });
            setMyReports((s) => new Set(s).add(r.id));
            setData((d) => ({ ...d, reports: [r, ...d.reports.filter((x) => x.id !== r.id)] }));
            dispatch({ type: 'go', to: { view: 'sent', id: r.id, cat: a.cat } });
          } catch (e) {
            handleActionError(e, a, 'mengirim laporan');
          } finally {
            setBusy(false);
          }
          break;
        }
        case 'watch':
          dispatch({ type: 'go', to: { view: 'subscriptions', add: { center: a.center, name: a.name } }, snap: 1 });
          break;
        case 'alerts':
          dispatch({ type: 'go', to: { view: 'alerts' } });
          break;
        case 'subscriptions':
          dispatch({ type: 'go', to: { view: 'subscriptions' } });
          break;
      }
    },
    [handleActionError, showToast],
  );

  const need = useCallback(
    (a: PendingAction, reason: string) => {
      if (auth.signedIn) runPending(a);
      else dispatch({ type: 'go', to: { view: 'signin', reason, pending: a } });
    },
    [auth.signedIn, runPending],
  );

  const unread = alerts.filter((a) => !readAlerts.has(a.id)).length;

  const value: Store = {
    routes,
    loadRoutes,
    subRadius,
    setSubRadius,
    liveCenter,
    setLiveCenter,
    nav,
    dispatch,
    go,
    back,
    home,
    data,
    online,
    subscriptions,
    alerts,
    unread,
    markAlertRead: (id) => setReadAlerts((s) => new Set(s).add(id)),
    refreshAccountData,
    refreshAll,
    location,
    me,
    focus,
    setMapCenter: setMapCenterState,
    requestLocation,
    filters,
    setFilters,
    layers,
    setLayers,
    myVotes,
    myReports,
    dismissed,
    dismiss: (id) => setDismissed((s) => new Set(s).add(id)),
    recentSearches,
    addRecent,
    removeRecent,
    toast,
    showToast,
    hideToast,
    mapApi,
    need,
    runPending,
    actionError,
    clearActionError: () => setActionError(null),
    busy,
    openReport,
    openPlace,
    share,
  };

  return <Ctx.Provider value={value}>{children}</Ctx.Provider>;
}

export function useStore() {
  const v = useContext(Ctx);
  if (!v) throw new Error('useStore outside AppStoreProvider');
  return v;
}

// ---------------------------------------------------------------- derived helpers

export function isActive(r: Report, now = Date.now()) {
  return r.status !== 'expired' && Date.parse(r.expires_at) > now;
}

export function matchesFilter(r: Report, f: Filters) {
  if (f.cats.length && !f.cats.includes(r.category)) return false;
  if (f.status === 'verified' && r.status !== 'verified') return false;
  if (f.status === 'waiting' && r.status !== 'waiting') return false;
  return true;
}

export function filterCount(f: Filters) {
  return f.cats.length + (f.status !== 'all' ? 1 : 0);
}

/** Who may answer the "Bantu pastikan" question (spec §12.2, §12.3). */
export function canConfirm(r: Report, me: Point | null, mine: boolean, voted: boolean) {
  return !!me && !mine && !voted && isActive(r) && distanceM(me, r) <= CONFIRM_RADIUS_M;
}

export function placeLabel(p: Point): string {
  const d = ApiService.describe(p);
  return d.street ?? 'Titik di peta';
}

export function reportTitle(r: Report) {
  return r.description?.trim() || CATEGORIES[r.category].title;
}

export type { ReportCategory };
