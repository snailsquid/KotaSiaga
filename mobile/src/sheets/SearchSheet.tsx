import React from 'react';
import { Keyboard, View } from 'react-native';
import { IconButton } from 'react-native-paper';
import { ApiService, type PlaceEntry } from '../api/ApiService';
import { CATEGORIES } from '../domain/categories';
import { distanceM } from '../domain/geo';
import { formatDistance } from '../domain/format';
import { isActive, placeLabel, reportTitle, useStore } from '../state/AppStore';
import type { PlaceRef, SheetEntry } from '../state/types';
import { Muted, Row, SectionLabel, statusShort } from './common';

const KIND_ICON: Record<string, string> = { place: 'map-marker-outline', tourism: 'camera-outline', donor: 'water', pin: 'map-marker', me: 'crosshairs-gps', subscription: 'bell-outline' };

/** Search lives in the sheet at full height (spec §5.1). Routes start here (spec §12b.3). */
export function SearchSheet({ query }: { query: string }) {
  const s = useStore();
  const entry = s.nav.current as Extract<SheetEntry, { view: 'search' }>;
  const from = s.me ?? s.focus;
  const q = query.trim().toLowerCase();

  const pickPlace = (p: PlaceRef) => {
    Keyboard.dismiss();
    if (p.kind !== 'me') s.addRecent(p);
    if (entry.pickFor) {
      const { field, dest, origin } = entry.pickFor;
      const next = field === 'dest' ? { dest: p, origin } : { dest, origin: p.kind === 'me' ? null : p };
      s.dispatch({ type: 'replace', to: { view: 'route', ...next } });
      return;
    }
    if (p.kind === 'me') {
      if (s.me) s.mapApi.current?.flyTo(s.me);
      s.home();
      return;
    }
    // Picking a result closes search and opens that item (spec §5.1).
    s.dispatch({ type: 'replace', to: { view: 'place', place: p } });
    s.mapApi.current?.flyTo(p);
  };

  const pickReport = (id: string) => {
    Keyboard.dismiss();
    s.dispatch({ type: 'replace', to: { view: 'report', id } });
    const r = s.data.reports.find((x) => x.id === id);
    if (r) s.mapApi.current?.flyTo(r);
  };

  const toRef = (p: PlaceEntry): PlaceRef => ({ ...p, kind: p.kind });
  const dist = (p: { lat: number; lon: number }) => formatDistance(distanceM(from, p));

  const reports = s.data.reports
    .filter((r) => isActive(r))
    .filter((r) => !q || reportTitle(r).toLowerCase().includes(q) || CATEGORIES[r.category].label.toLowerCase().includes(q) || placeLabel(r).toLowerCase().includes(q))
    .sort((a, b) => distanceM(from, a) - distanceM(from, b))
    .slice(0, q ? 5 : 2);

  if (!q) {
    const recent = s.recentSearches;
    return (
      <>
        <View>
          <Row icon="crosshairs-gps" title="Lokasi saya" subtitle={s.me ? placeLabel(s.me) : 'Lokasi belum diizinkan'} onPress={() => pickPlace({ kind: 'me', name: 'Lokasi saya', lat: from.lat, lon: from.lon })} />
          {recent.length ? (
            <>
              <SectionLabel>Terakhir dicari</SectionLabel>
              {recent.map((p) => (
                <Row
                  key={(p.id ?? '') + p.lat + p.lon}
                  icon="history"
                  title={p.name}
                  subtitle={[p.address, dist(p)].filter(Boolean).join(' · ')}
                  onPress={() => pickPlace(p)}
                  trailing={<IconButton icon="close" size={20} accessibilityLabel={`Hapus ${p.name} dari riwayat`} onPress={() => s.removeRecent(p)} />}
                />
              ))}
            </>
          ) : null}
          {s.subscriptions.length ? (
            <>
              <SectionLabel>Area pantauan</SectionLabel>
              {s.subscriptions.map((sub) => (
                <Row key={sub.id} icon="bell-outline" title={sub.name} subtitle={dist(sub)} onPress={() => pickPlace({ id: sub.id, kind: 'subscription', name: sub.name, address: 'Area pantauan', lat: sub.lat, lon: sub.lon })} />
              ))}
            </>
          ) : null}
          {!recent.length ? (
            <>
              <SectionLabel>Tempat</SectionLabel>
              {ApiService.searchPlaces('', from)
                .filter((p) => p.kind === 'place')
                .slice(0, 4)
                .map((p) => (
                  <Row key={p.id} icon={KIND_ICON.place} title={p.name} subtitle={`${p.address} · ${dist(p)}`} onPress={() => pickPlace(toRef(p))} />
                ))}
            </>
          ) : null}
        </View>
        {reports.length && !entry.pickFor ? (
          <View>
            <SectionLabel>Laporan</SectionLabel>
            {reports.map((r) => (
              <Row key={r.id} icon={CATEGORIES[r.category].icon} title={reportTitle(r)} subtitle={`${placeLabel(r)} · ${statusShort(r)}`} onPress={() => pickReport(r.id)} />
            ))}
          </View>
        ) : null}
      </>
    );
  }

  const places = ApiService.searchPlaces(q, from);
  const groups: { title: string; items: PlaceEntry[] }[] = [
    { title: 'Destinasi wisata', items: places.filter((p) => p.kind === 'tourism') },
    { title: 'Lokasi donor darah', items: places.filter((p) => p.kind === 'donor') },
    { title: 'Tempat', items: places.filter((p) => p.kind === 'place') },
  ];
  const nothing = reports.length === 0 && places.length === 0;

  return (
    <>
      {nothing ? <Muted>Tidak ditemukan. Coba kata lain atau tekan lama di peta untuk memilih titik.</Muted> : null}
      {reports.length && !entry.pickFor ? (
        <View>
          <SectionLabel>Laporan</SectionLabel>
          {reports.map((r) => (
            <Row key={r.id} icon={CATEGORIES[r.category].icon} title={reportTitle(r)} subtitle={`${placeLabel(r)} · ${statusShort(r)}`} onPress={() => pickReport(r.id)} />
          ))}
        </View>
      ) : null}
      {groups
        .filter((g) => g.items.length)
        .map((g) => (
          <View key={g.title}>
            <SectionLabel>{g.title}</SectionLabel>
            {g.items.map((p) => (
              <Row key={p.id} icon={KIND_ICON[p.kind]} title={p.name} subtitle={`${p.address} · ${dist(p)}`} onPress={() => pickPlace(toRef(p))} />
            ))}
          </View>
        ))}
    </>
  );
}
