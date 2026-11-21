# KotaSiaga mobile (Expo)

The citizen app: one Google-Maps-style map screen with a search bar, filter chips, map buttons,
an extended "Lapor" button and one bottom sheet whose content changes. Built from
[docs/mobile-ui-spec.md](../docs/mobile-ui-spec.md) (§0, §12, §12b, §13, §14 are final) and the
prototype in [docs/design/prototype.dc.html](../docs/design/prototype.dc.html). Rules:
[AGENTS.md](../AGENTS.md).

**Frontend only.** Every request goes through one `ApiService`, which currently talks to an
in-app mock backend. No real backend is called unless `EXPO_PUBLIC_API_MODE=http` is set.

## Run

```sh
cd mobile
npm install
cp .env.example .env      # optional; defaults work. Never commit .env
npx expo start            # press a (Android) or i (iOS), or scan with Expo Go
```

- Default is `MOCK` mode. The user is placed near Jl. Pasteur, Bandung, so the demo works anywhere
  (`EXPO_PUBLIC_USE_DEVICE_LOCATION=true` uses the phone's GPS).
- Sign-in is simulated with a test account ("Warga Uji") until Google client IDs are set in `.env`.
- Demo controls: menu (≡) → "Mode demo (MOCK)" runs the AGENTS.md §7 scenarios (rain storm,
  verified flood, road closure, disaster signal, upstream down → stale, offline, recover, reset).
- Android builds outside Expo Go need `GOOGLE_MAPS_ANDROID_API_KEY` in env (read by `app.config.js`).

Checks: `npx tsc --noEmit`, `npx expo-doctor`, `npx expo export --platform android`.

## Stack

Expo SDK 58, React Native Paper (Material Design 3, scheme from seed `#0061A4`; risk colors are
the only MD3 custom colors), MaterialCommunityIcons via `@expo/vector-icons`, react-native-maps,
@gorhom/bottom-sheet (+ reanimated, gesture-handler, worklets), expo-auth-session,
expo-web-browser, expo-secure-store, expo-location.

## Layout

| Path | What |
|---|---|
| `src/api/ApiService.ts` | The only HTTP entry. 5 s timeout, one retry with jitter on idempotent calls, last good response per endpoint served with `meta.stale: true` on failure, one log line per call (time, service, path, status, ms) |
| `src/api/sanitize.ts` | Validates responses against `openapi.yaml`, drops unknown fields |
| `src/api/transport.ts` | `HttpTransport` (fetch) and `MockTransport` (in-app mock) |
| `src/api/mock/MockBackend.ts` | Mock of the core service: randomizes inputs (weather, reports, votes, disaster signals, faults) and runs real verification, risk and routing rules. Logs its seed at start |
| `src/api/mock/gazetteer.ts` | Test place data for Bandung (streets, districts, places, tourism and donation points) |
| `src/state/` | Sheet history (last 5), filters, layers, gated actions that continue after sign-in, live polling |
| `src/map/` | Map: soft risk zones + colored road stretches (Waspada/Bahaya only), neutral pins, clusters, routes |
| `src/sheets/` | Every sheet content from spec §3.2 |
| `src/components/` | Search bar, filter chips + status strip, map FABs + layers card, drawer |

## Live updates (AGENTS.md §7 mobile)

- Polls `GET /changes?since=<seq>` every 5 s while the app is in the foreground and stops in the
  background. On `meta.reset` it refetches everything.
- Status strip: "Diperbarui HH:MM:SS", a **MOCK** chip when `meta.source === "mock"`, the stale line
  "Sebagian data mungkin tidak terbaru" when `meta.stale`, and the offline line (offline wins).
- Updates never move the camera or close the open sheet.

## What is stubbed

| Item | State |
|---|---|
| Backend | In-app mock (`MockTransport`). `HttpTransport` exists but is not used by default |
| Google sign-in | Simulated test account unless client IDs are set; `POST /auth/sso` is answered by the mock only |
| Photo ("Tambah foto") | Shows "Kirim foto belum tersedia di versi ini". No upload endpoint, no camera dependency |
| Severity after sending | Selectable on the "terkirim" sheet but not sent (no update endpoint) |
| Place search, reverse geocoding, tourism and donation layers | Local gazetteer through `ApiService` |
| Push notifications | Not implemented (in-app alerts list only) |
| Follow mode with map rotation (◎ twice) | Recenters only; no heading rotation |
| Remembered layers, recent searches, map position, first-launch flag | In memory only (no storage dependency approved). First launch is detected from the location permission state instead |

## Spec and contract gaps

Found while building. None of these were changed in `openapi.yaml`; each needs a human decision
and, if it touches the contract, the change protocol in AGENTS.md §10.

1. **Risk cells have no geometry.** `CellRisk` has `cell_id` (opaque) but no position, so the app
   cannot draw zones. The app currently parses the mock's `c_<row>_<col>` ids. Proposal: add
   `lat`, `lon` (cell center) or a polygon to `CellRisk` (additive).
2. **`POST /auth/sso`** (Google `id_token` → our JWT) is not in `openapi.yaml` yet (decided 2026-11-22).
3. **`DELETE /subscriptions/{id}`** is missing; spec §7.10 needs delete with undo.
4. **Reports lack display fields**: no street/place name, no reporter level ("Warga · Pelapor
   Aktif", spec §12.8), no "is mine" and no "my vote". The app derives place names from a local
   gazetteer and keeps own reports and votes in memory for the session.
5. **"Dipastikan n dari 3 warga"** shows `confirm_count` against 3, but verification is by weight
   (sum of reputations 0.2 to 1.0 ≥ 3.0). The count and the real threshold can disagree.
6. **500 m confirm rule** is checked only on the phone. The vote body has no location, so the server
   cannot enforce it.
7. **Alerts carry only risk changes.** Spec §12.10 also wants "Laporanmu sudah dipastikan (dilihat
   86 orang)", "Bantu pastikan" requests, read/unread state and a weekly summary. None exist in the
   contract; the weekly card uses this session's counts.
8. **No search / places endpoint** for the mobile app (places, tourism destinations, donation points).
   Tourism and Health data reach our core service through adapters, but nothing exposes them to the app.
9. **No photo upload** and **no report update** (severity, description after sending).
10. **Road stretches** need road geometry. The app clips a small local street list to the risk zones.
11. **Disaster "Rute menghindari area ini"** (spec §7.4) has no destination; it opens search, and every
    recommended route already avoids risk.
12. Prototype vs spec: when a filter is on, the prototype replaces the status strip with
    "Menampilkan N dari M"; the app does that but keeps the stale/offline line visible.
13. App identity (Android package, iOS bundle id) is not set; needed for builds and Google OAuth clients.
