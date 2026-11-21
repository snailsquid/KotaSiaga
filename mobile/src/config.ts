// All runtime settings come from env (EXPO_PUBLIC_* is inlined at build time).
// Nothing secret lives here: Google OAuth client IDs are public identifiers.

const env = process.env;

function bool(v: string | undefined, dflt: boolean): boolean {
  if (v === undefined || v === '') return dflt;
  return v === 'true' || v === '1';
}

function int(v: string | undefined, dflt: number, min: number, max: number): number {
  const n = v ? parseInt(v, 10) : NaN;
  return Number.isFinite(n) ? Math.min(max, Math.max(min, n)) : dflt;
}

export const config = {
  /** `mock` = in-app mock backend (default). `http` = real backend at apiBaseUrl. */
  apiMode: (env.EXPO_PUBLIC_API_MODE === 'http' ? 'http' : 'mock') as 'mock' | 'http',
  /** Smart City base URL, e.g. https://kotasiaga.stei.site/api. Release builds must use HTTPS. */
  apiBaseUrl: env.EXPO_PUBLIC_API_BASE_URL ?? '',
  /** Polling interval on the map screen (AGENTS.md §7: 5 to 10 s). */
  pollSeconds: int(env.EXPO_PUBLIC_POLL_SECONDS, 5, 5, 10),
  /** Mock generator tick (AGENTS.md §7 MOCK_TICK_SECONDS). */
  mockTickSeconds: int(env.EXPO_PUBLIC_MOCK_TICK_SECONDS, 5, 2, 60),
  mockTimeScale: int(env.EXPO_PUBLIC_MOCK_TIME_SCALE, 60, 1, 3600),
  mockSeed: env.EXPO_PUBLIC_MOCK_SEED ? parseInt(env.EXPO_PUBLIC_MOCK_SEED, 10) : undefined,
  /**
   * In mock mode the user is placed at a fixed point in Bandung so the demo works anywhere.
   * Set EXPO_PUBLIC_USE_DEVICE_LOCATION=true to use the phone's real position instead.
   */
  useDeviceLocation: bool(env.EXPO_PUBLIC_USE_DEVICE_LOCATION, env.EXPO_PUBLIC_API_MODE === 'http'),
  google: {
    androidClientId: env.EXPO_PUBLIC_GOOGLE_ANDROID_CLIENT_ID,
    iosClientId: env.EXPO_PUBLIC_GOOGLE_IOS_CLIENT_ID,
    webClientId: env.EXPO_PUBLIC_GOOGLE_WEB_CLIENT_ID,
  },
};

export const googleConfigured = !!(
  config.google.androidClientId ||
  config.google.iosClientId ||
  config.google.webClientId
);
