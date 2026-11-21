import React, { useEffect, useMemo } from 'react';
import { Linking, Pressable, StyleSheet, View } from 'react-native';
import { Banner, Button, IconButton, Text } from 'react-native-paper';
import { MaterialCommunityIcons } from '@expo/vector-icons';
import { ApiService } from '../api/ApiService';
import type { Point, RiskLevel, Route } from '../api/types';
import { formatDistance, formatDuration } from '../domain/format';
import { RISK_LABEL, RISK_ORDER } from '../domain/risk';
import { useStore } from '../state/AppStore';
import type { PlaceRef, SheetEntry } from '../state/types';
import { RiskBadge } from '../components/RiskBadge';
import { InlineError, Muted, Placeholder, SheetHeader, useAppTheme, HIT } from './common';

function navUrl(origin: Point, dest: Point) {
  return `https://www.google.com/maps/dir/?api=1&origin=${origin.lat},${origin.lon}&destination=${dest.lat},${dest.lon}&travelmode=driving`;
}

/** KotaSiaga does not do turn-by-turn; it hands off to the phone's maps app (spec §7.8). */
function openNavigation(origin: Point, dest: Point) {
  Linking.openURL(navUrl(origin, dest)).catch(() => undefined);
}

function useOrigin(origin: PlaceRef | null): Point {
  const s = useStore();
  return origin ?? s.me ?? s.focus;
}

/** route_plan (spec §7.8). Every recommended route is the safe one; no separate mode (spec §12b.3). */
export function RouteSheet() {
  const s = useStore();
  const t = useAppTheme();
  const e = s.nav.current as Extract<SheetEntry, { view: 'route' }>;
  const origin = useOrigin(e.origin);

  useEffect(() => {
    s.loadRoutes(origin, e.dest);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [origin.lat, origin.lon, e.dest.lat, e.dest.lon]);

  const rr = s.routes.result;
  const selected = e.selected ?? rr?.recommended_id;

  useEffect(() => {
    if (rr?.routes.length) {
      const pts = rr.routes.flatMap((r) => r.geometry.coordinates.map(([lon, lat]) => ({ lat, lon })));
      s.mapApi.current?.fit(pts);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [rr]);

  const ordered = rr ? [...rr.routes].sort((a, b) => (a.id === rr.recommended_id ? -1 : b.id === rr.recommended_id ? 1 : 0)) : [];

  return (
    <>
      <View style={{ flexDirection: 'row', alignItems: 'center', gap: 8 }}>
        <View style={{ flex: 1, gap: 8 }}>
          <Field
            icon="crosshairs-gps"
            iconColor="#4285F4"
            text={e.origin?.name ?? 'Lokasi saya'}
            label="Dari"
            onPress={() => s.go({ view: 'search', pickFor: { field: 'origin', dest: e.dest, origin: e.origin } })}
          />
          <Field icon="map-marker" iconColor={t.colors.error} text={e.dest.name} label="Ke" onPress={() => s.go({ view: 'search', pickFor: { field: 'dest', dest: e.dest, origin: e.origin } })} />
        </View>
        <IconButton
          icon="swap-vertical"
          accessibilityLabel="Tukar asal dan tujuan"
          onPress={() => {
            const me: PlaceRef = { kind: 'me', name: 'Lokasi saya', lat: origin.lat, lon: origin.lon };
            const newOrigin = e.dest.kind === 'me' ? null : e.dest;
            s.dispatch({ type: 'replace', to: { view: 'route', origin: newOrigin, dest: e.origin ?? me } });
          }}
        />
      </View>

      {s.routes.loading && !rr ? <Placeholder rows={2} /> : null}
      {s.routes.error ? (
        <View style={{ gap: 8 }}>
          <InlineError message={s.routes.error} onRetry={() => s.loadRoutes(origin, e.dest)} />
          <Button hitSlop={HIT} mode="outlined" icon="navigation-variant" onPress={() => openNavigation(origin, e.dest)}>
            Buka di aplikasi navigasi
          </Button>
        </View>
      ) : null}

      {ordered.map((r) => (
        <RouteCard
          key={r.id}
          r={r}
          recommended={r.id === rr!.recommended_id}
          selected={r.id === selected}
          onPress={() => {
            if (r.id === selected) s.go({ view: 'route_detail', dest: e.dest, origin: e.origin, routeId: r.id });
            else s.dispatch({ type: 'patch', patch: { selected: r.id } as Partial<SheetEntry> });
          }}
        />
      ))}

      {rr ? (
        <Button hitSlop={HIT} mode="contained" icon="navigation-variant" onPress={() => openNavigation(origin, e.dest)}>
          Mulai di aplikasi navigasi
        </Button>
      ) : null}
    </>
  );
}

function Field({ icon, iconColor, text, label, onPress }: { icon: string; iconColor: string; text: string; label: string; onPress: () => void }) {
  const t = useAppTheme();
  return (
    <Pressable onPress={onPress} accessibilityRole="button" accessibilityLabel={`${label}: ${text}. Ketuk untuk mengubah.`} style={[st.field, { borderColor: t.colors.outline }]}>
      <MaterialCommunityIcons name={icon as any} size={20} color={iconColor} />
      <Text variant="bodyLarge" numberOfLines={1} style={{ flex: 1 }}>
        {text}
      </Text>
    </Pressable>
  );
}

function RouteCard({ r, recommended, selected, onPress }: { r: Route; recommended: boolean; selected: boolean; onPress: () => void }) {
  const t = useAppTheme();
  return (
    <Pressable
      onPress={onPress}
      accessibilityRole="button"
      accessibilityState={{ selected }}
      accessibilityLabel={`${recommended ? 'Disarankan. ' : ''}${formatDuration(r.duration_s)}, ${formatDistance(r.distance_m)}, ${RISK_LABEL[r.max_risk_level]}. ${r.reasons.join(' ')}`}
      accessibilityHint={selected ? 'Ketuk untuk melihat detail rute' : 'Ketuk untuk memilih rute ini'}
      style={[
        st.card,
        selected ? { backgroundColor: t.colors.surfaceContainer, borderColor: t.colors.primary, borderWidth: 2 } : { backgroundColor: t.colors.surfaceContainerLowest, borderColor: t.colors.outlineVariant, borderWidth: 1 },
      ]}
    >
      <View style={{ flexDirection: 'row', alignItems: 'center', gap: 8, flexWrap: 'wrap' }}>
        <Text variant="titleLarge">{formatDuration(r.duration_s)}</Text>
        <Muted>{formatDistance(r.distance_m)}</Muted>
        <RiskBadge level={r.max_risk_level} />
        {recommended ? (
          <Text variant="labelMedium" style={{ color: t.colors.primary, marginLeft: 'auto' }}>
            Disarankan
          </Text>
        ) : null}
      </View>
      {r.reasons.map((x) => (
        <Muted key={x}>{x}</Muted>
      ))}
    </Pressable>
  );
}

/** Highest current risk along a route, recomputed from the live risk grid. */
function currentMaxRisk(r: Route, cells: Map<string, { risk_level: RiskLevel }>): RiskLevel {
  let worst: RiskLevel = 'safe';
  const seen = new Set<string>();
  for (const [lon, lat] of r.geometry.coordinates) {
    const id = ApiService.cellAt({ lat, lon });
    if (seen.has(id)) continue;
    seen.add(id);
    const lvl = cells.get(id)?.risk_level ?? 'safe';
    if (RISK_ORDER[lvl] > RISK_ORDER[worst]) worst = lvl;
  }
  return worst;
}

export function RouteDetailSheet() {
  const s = useStore();
  const e = s.nav.current as Extract<SheetEntry, { view: 'route_detail' }>;
  const origin = useOrigin(e.origin);
  const r = s.routes.result?.routes.find((x) => x.id === e.routeId);
  const now = useMemo(() => (r ? currentMaxRisk(r, s.data.cells) : null), [r, s.data.cells]);

  const areas = useMemo(() => {
    if (!r) return [];
    const out: { street: string; level: RiskLevel }[] = [];
    const seen = new Set<string>();
    for (const [lon, lat] of r.geometry.coordinates) {
      const id = ApiService.cellAt({ lat, lon });
      if (seen.has(id)) continue;
      seen.add(id);
      const lvl = s.data.cells.get(id)?.risk_level ?? 'safe';
      if (lvl === 'safe') continue;
      const street = ApiService.describe({ lat, lon }).street ?? 'Titik tanpa nama jalan';
      if (!out.some((o) => o.street === street)) out.push({ street, level: lvl });
    }
    return out;
  }, [r, s.data.cells]);

  if (!r) {
    return (
      <>
        <SheetHeader title="Rute" onClose={s.back} closeLabel="Kembali ke daftar rute" />
        <Muted>Rute aman belum tersedia</Muted>
        <Button hitSlop={HIT} mode="outlined" icon="navigation-variant" onPress={() => openNavigation(origin, e.dest)}>
          Buka di aplikasi navigasi
        </Button>
      </>
    );
  }

  const changed = now !== null && now !== r.max_risk_level;

  return (
    <>
      <SheetHeader title={`Ke ${e.dest.name}`} subtitle={`${formatDuration(r.duration_s)} · ${formatDistance(r.distance_m)}`} onClose={s.back} closeLabel="Kembali ke daftar rute" />
      <Banner
        visible={changed}
        icon="alert"
        actions={[{ label: 'Perbarui rute', onPress: () => s.loadRoutes(origin, e.dest) }]}
        style={{ borderRadius: 12 }}
        accessibilityLiveRegion="polite"
      >
        Risiko di rute ini berubah{now ? `: sekarang ${RISK_LABEL[now]}` : ''}.
      </Banner>
      <View style={{ flexDirection: 'row', alignItems: 'center', gap: 8 }}>
        <RiskBadge level={r.max_risk_level} />
        <Text variant="titleMedium">Kenapa rute ini</Text>
      </View>
      {r.reasons.map((x) => (
        <Muted key={x}>{x}</Muted>
      ))}
      <View style={{ gap: 8 }}>
        <Text variant="titleSmall">Area yang dilewati</Text>
        {areas.length === 0 ? (
          <Muted>Tidak ada area Waspada atau Bahaya di rute ini.</Muted>
        ) : (
          areas.map((a) => (
            <View key={a.street} style={{ flexDirection: 'row', alignItems: 'center', gap: 8 }}>
              <RiskBadge level={a.level} small />
              <Text variant="bodyMedium">{a.street}</Text>
            </View>
          ))
        )}
      </View>
      <Button hitSlop={HIT} mode="contained" icon="navigation-variant" onPress={() => openNavigation(origin, e.dest)}>
        Buka di aplikasi navigasi
      </Button>
    </>
  );
}

const st = StyleSheet.create({
  field: { flexDirection: 'row', alignItems: 'center', gap: 12, minHeight: 48, paddingHorizontal: 16, borderRadius: 4, borderWidth: 1 },
  card: { borderRadius: 12, paddingVertical: 12, paddingHorizontal: 16, gap: 4 },
});
