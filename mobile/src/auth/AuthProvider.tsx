// Google SSO via expo-auth-session (decided 2026-11-22). The Google id_token is exchanged for our
// own JWT through the proposed POST /auth/sso (not in openapi.yaml yet, see README).
// Without Google client IDs in env, sign-in is simulated with a test account so the UI can be built.

import React, { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState } from 'react';
import * as WebBrowser from 'expo-web-browser';
import * as SecureStore from 'expo-secure-store';
import * as Google from 'expo-auth-session/providers/google';
import { ApiService } from '../api/ApiService';
import { config, googleConfigured } from '../config';

WebBrowser.maybeCompleteAuthSession();

export interface AppUser {
  name: string;
  email: string;
  photoUrl?: string;
}

interface AuthValue {
  user: AppUser | null;
  signedIn: boolean;
  /** Resolves true when signed in, false when cancelled or failed. */
  signIn: () => Promise<boolean>;
  signOut: () => Promise<void>;
  /** Call when the backend says the session is no longer valid (401). */
  sessionEnded: () => void;
}

const AuthContext = createContext<AuthValue | null>(null);

const STORE_KEY = 'kotasiaga.session';

function decodeIdToken(idToken: string): AppUser | null {
  try {
    const part = idToken.split('.')[1];
    const json = JSON.parse(globalThis.atob(part.replace(/-/g, '+').replace(/_/g, '/')));
    return { name: json.name ?? 'Warga', email: json.email ?? '', photoUrl: json.picture };
  } catch {
    return null;
  }
}

type PromptFn = () => Promise<string | null>;

/** Only mounted when client IDs exist: the Google hook throws without them. */
function GoogleBridge({ onReady }: { onReady: (prompt: PromptFn) => void }) {
  const [request, , promptAsync] = Google.useIdTokenAuthRequest({
    androidClientId: config.google.androidClientId,
    iosClientId: config.google.iosClientId,
    webClientId: config.google.webClientId,
    selectAccount: true,
  });
  useEffect(() => {
    if (!request) return;
    onReady(async () => {
      const res = await promptAsync();
      if (res.type !== 'success') return null;
      return (res.params?.id_token as string | undefined) ?? null;
    });
  }, [request, promptAsync, onReady]);
  return null;
}

export function AuthProvider({ children }: { children: React.ReactNode }) {
  const [user, setUser] = useState<AppUser | null>(null);
  const prompt = useRef<PromptFn | null>(null);
  // Persist only against a real backend; the in-app mock forgets tokens on reload.
  const persist = !ApiService.isMock;

  useEffect(() => {
    if (!persist) return;
    SecureStore.getItemAsync(STORE_KEY)
      .then((raw) => {
        if (!raw) return;
        const s = JSON.parse(raw) as { token: string; user: AppUser };
        ApiService.setToken(s.token);
        setUser(s.user);
      })
      .catch(() => undefined);
  }, [persist]);

  const signIn = useCallback(async () => {
    try {
      let idToken: string | null;
      let profile: AppUser | null;
      if (googleConfigured) {
        if (!prompt.current) return false;
        idToken = await prompt.current();
        if (!idToken) return false; // cancelled: nothing lost
        profile = decodeIdToken(idToken);
      } else {
        // Simulated Google window. Test account only (AGENTS.md rule 8).
        await new Promise((r) => setTimeout(r, 600));
        idToken = 'mock-google-id-token';
        profile = { name: 'Warga Uji', email: 'warga.uji@example.com' };
      }
      const token = await ApiService.authSso(idToken);
      ApiService.setToken(token.access_token);
      const u = profile ?? { name: 'Warga', email: '' };
      setUser(u);
      if (persist) await SecureStore.setItemAsync(STORE_KEY, JSON.stringify({ token: token.access_token, user: u }));
      return true;
    } catch {
      return false;
    }
  }, [persist]);

  const clear = useCallback(() => {
    ApiService.setToken(null);
    setUser(null);
    if (persist) SecureStore.deleteItemAsync(STORE_KEY).catch(() => undefined);
  }, [persist]);

  const value = useMemo<AuthValue>(
    () => ({ user, signedIn: !!user, signIn, signOut: async () => clear(), sessionEnded: clear }),
    [user, signIn, clear],
  );

  const onReady = useCallback((p: PromptFn) => {
    prompt.current = p;
  }, []);

  return (
    <AuthContext.Provider value={value}>
      {googleConfigured ? <GoogleBridge onReady={onReady} /> : null}
      {children}
    </AuthContext.Provider>
  );
}

export function useAuth() {
  const v = useContext(AuthContext);
  if (!v) throw new Error('useAuth outside AuthProvider');
  return v;
}
