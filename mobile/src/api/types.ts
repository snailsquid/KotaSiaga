// Shapes from openapi.yaml (AGENTS.md §4). Do not add fields here that the contract lacks.

export type RiskLevel = 'safe' | 'caution' | 'danger';
export type ReportCategory = 'flood' | 'accident' | 'road_damage' | 'road_closure' | 'event_crowd';
export type ReportStatus = 'waiting' | 'verified' | 'expired';
export type WeatherCondition = 'clear' | 'cloudy' | 'rain' | 'heavy_rain' | 'storm';
export type Vote = 'confirm' | 'dispute';

export interface Point {
  lat: number;
  lon: number;
}

export interface Meta {
  seq: number;
  updated_at: string;
  source: 'live' | 'mock';
  stale: boolean;
  reset?: boolean;
}

export interface Envelope<T> {
  data: T;
  meta: Meta;
}

export type ErrorCode =
  | 'invalid_api_key'
  | 'forbidden'
  | 'not_found'
  | 'validation_error'
  | 'rate_limited'
  | 'upstream_unavailable';

export interface ApiErrorBody {
  error: { code: ErrorCode; message: string; request_id: string };
}

export interface RiskFactor {
  type: 'rain' | 'verified_report' | 'disaster_signal';
  value?: number;
  count?: number;
}

export interface CellRisk {
  cell_id: string;
  risk_level: RiskLevel;
  score: number;
  factors: RiskFactor[];
}

export interface ForecastPoint {
  at: string;
  rain_mm_h: number;
  temperature_c: number;
}

export interface Weather {
  observed_at: string;
  temperature_c: number;
  rain_mm_h: number;
  condition: WeatherCondition;
  forecast_6h: ForecastPoint[];
}

export interface Status {
  cell_id: string;
  risk_level: RiskLevel;
  weather: Weather;
  active_reports: number;
  disaster_signals: number;
}

export type DisasterType = 'flood' | 'earthquake' | 'fire' | 'haze' | 'wind' | 'volcano';

export interface DisasterSignal {
  id: string;
  type: DisasterType;
  lat: number;
  lon: number;
  reported_at: string;
  status: 'active' | 'resolved';
  resolved_at?: string | null;
}

export interface Report {
  id: string;
  category: ReportCategory;
  status: ReportStatus;
  lat: number;
  lon: number;
  description?: string;
  severity: number;
  confirm_count?: number;
  dispute_count?: number;
  created_at: string;
  verified_at?: string | null;
  expires_at: string;
}

export interface ReportCreate {
  category: ReportCategory;
  lat: number;
  lon: number;
  description?: string;
  severity?: number;
}

export interface SafeRouteRequest {
  origin: Point;
  destination: Point;
  alternatives?: number;
}

export interface LineString {
  type: 'LineString';
  /** GeoJSON order: [lon, lat] */
  coordinates: [number, number][];
}

export interface Route {
  id: string;
  distance_m: number;
  duration_s: number;
  risk_penalty: number;
  max_risk_level: RiskLevel;
  geometry: LineString;
  reasons: string[];
}

export interface SafeRouteResult {
  recommended_id: string;
  routes: Route[];
}

export type ChangeType =
  | 'report.created'
  | 'report.verified'
  | 'report.expired'
  | 'risk.changed'
  | 'weather.updated'
  | 'disaster.created'
  | 'disaster.resolved';

export interface Change {
  seq: number;
  at: string;
  type: ChangeType;
  entity_id: string;
  summary: string;
}

export interface Token {
  access_token: string;
  token_type: 'bearer';
  expires_in: number;
}

export interface Subscription {
  id: string;
  name: string;
  lat: number;
  lon: number;
  radius_m: number;
  created_at: string;
}

export interface SubscriptionCreate {
  name: string;
  lat: number;
  lon: number;
  radius_m?: number;
}

export interface Alert {
  id: string;
  subscription_id: string;
  cell_id: string;
  risk_level: RiskLevel;
  at: string;
  message: string;
}

export type MockScenario =
  | 'rain_storm'
  | 'flood_verified'
  | 'road_closure'
  | 'event_crowd'
  | 'disaster_signal'
  | 'upstream_down'
  | 'recover'
  | 'reset';

export interface Bbox {
  minLon: number;
  minLat: number;
  maxLon: number;
  maxLat: number;
}
