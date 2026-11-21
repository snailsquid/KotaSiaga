import React, { useState } from 'react';
import { Pressable, StyleSheet, View } from 'react-native';
import { Button, Divider, SegmentedButtons, Text } from 'react-native-paper';
import { MaterialCommunityIcons } from '@expo/vector-icons';
import type { Point, ReportCategory } from '../api/types';
import { CATEGORIES, CATEGORY_ORDER, DUPLICATE_RADIUS_M } from '../domain/categories';
import { distanceM } from '../domain/geo';
import { ago, formatDistance } from '../domain/format';
import { isActive, placeLabel, reportTitle, useStore } from '../state/AppStore';
import { ApiService } from '../api/ApiService';
import type { SheetEntry } from '../state/types';
import { InlineError, Muted, SheetHeader, statusShort, useAppTheme } from './common';

/** "Apa yang kamu lihat?": one tap on a tile sends the report (spec §7.6, option A). */
export function QuickReportSheet() {
  const s = useStore();
  const t = useAppTheme();
  const e = s.nav.current as Extract<SheetEntry, { view: 'quick' }>;
  const at: Point | null = e.at ?? s.me;

  const pick = (cat: ReportCategory) => {
    if (!at) {
      s.go({ view: 'pick_location', at: s.focus });
      return;
    }
    // Duplicate check: same category within ~300 m (spec §7.6.3).
    const dup = s.data.reports
      .filter((r) => r.category === cat && isActive(r) && !s.myReports.has(r.id) && distanceM(at, r) <= DUPLICATE_RADIUS_M)
      .sort((a, b) => distanceM(at, a) - distanceM(at, b))[0];
    if (dup) s.go({ view: 'dup', dupId: dup.id, cat, at });
    else s.need({ type: 'submit', cat, at }, 'mengirim laporan');
  };

  const d = at ? ApiService.describe(at) : null;

  return (
    <>
      <SheetHeader title="Apa yang kamu lihat?" onClose={s.home} closeLabel="Batal" />
      <View style={q.loc}>
        <MaterialCommunityIcons name="map-marker-outline" size={24} color={t.colors.onSurfaceVariant} />
        <View style={{ flex: 1 }}>
          <Text variant="bodyLarge">{d ? `${d.street ?? 'Titik di peta'}, ${d.district}` : 'Lokasi belum diketahui'}</Text>
          <Muted>{e.at ? 'Titik yang kamu pilih' : at ? 'Lokasimu sekarang' : 'Pilih titik di peta'}</Muted>
        </View>
        <Button mode="text" compact onPress={() => s.go({ view: 'pick_location', at: at ?? s.focus })} accessibilityLabel="Ubah lokasi laporan">
          Ubah
        </Button>
      </View>
      {s.actionError ? <InlineError message={s.actionError} onRetry={s.clearActionError} /> : null}
      <View style={q.grid}>
        {CATEGORY_ORDER.map((k) => {
          const c = CATEGORIES[k];
          return (
            <Pressable
              key={k}
              onPress={() => pick(k)}
              disabled={s.busy}
              accessibilityRole="button"
              accessibilityLabel={`${c.label}. ${c.hint}. Ketuk untuk langsung mengirim laporan.`}
              android_ripple={{ color: t.colors.surfaceContainerHighest }}
              style={[q.tile, { borderColor: t.colors.outlineVariant, backgroundColor: t.colors.surfaceContainerLowest }]}
            >
              <MaterialCommunityIcons name={c.icon as any} size={28} color={t.colors.primary} />
              <Text variant="titleMedium">{c.label}</Text>
              <Muted variant="bodySmall">{c.hint}</Muted>
            </Pressable>
          );
        })}
      </View>
      <Muted variant="bodySmall">Satu ketukan langsung mengirim laporan. Detail bisa ditambah setelahnya.</Muted>
    </>
  );
}

/** "Ubah": the map moves under a fixed center pin, like picking a ride pickup point. */
export function PickLocationSheet() {
  const s = useStore();
  const d = ApiService.describe(s.liveCenter);
  return (
    <>
      <Text variant="titleMedium">Geser peta untuk memilih titik</Text>
      <Muted>
        {d.street ?? 'Titik di peta'}, {d.district}
      </Muted>
      <Button
        mode="contained"
        onPress={() => {
          s.dispatch({ type: 'back' });
          s.dispatch({ type: 'replace', to: { view: 'quick', at: s.liveCenter } });
        }}
      >
        Pakai titik ini
      </Button>
    </>
  );
}

export function DuplicateSheet() {
  const s = useStore();
  const t = useAppTheme();
  const e = s.nav.current as Extract<SheetEntry, { view: 'dup' }>;
  const r = s.data.reports.find((x) => x.id === e.dupId);
  if (!r) {
    return (
      <Button mode="contained" onPress={() => s.need({ type: 'submit', cat: e.cat, at: e.at }, 'mengirim laporan')}>
        Kirim laporan baru
      </Button>
    );
  }
  const c = CATEGORIES[r.category];
  const dist = formatDistance(distanceM(e.at, r));
  return (
    <>
      <Text variant="titleLarge" accessibilityRole="header">
        Sudah ada yang melaporkan
      </Text>
      <Muted>
        Ada laporan {c.label.toLowerCase()} {dist} dari kamu. Kalau ini kejadian yang sama, cukup pastikan saja. Laporan jadi lebih cepat dipercaya.
      </Muted>
      <View style={[q.dup, { borderColor: t.colors.outlineVariant }]}>
        <MaterialCommunityIcons name={c.icon as any} size={24} color={t.colors.onSurfaceVariant} />
        <View style={{ flex: 1 }}>
          <Text variant="bodyLarge">{reportTitle(r)}</Text>
          <Muted>
            {placeLabel(r)} · {ago(r.created_at)} · {statusShort(r)}
          </Muted>
        </View>
      </View>
      {s.actionError ? <InlineError message={s.actionError} onRetry={s.clearActionError} /> : null}
      <Button mode="contained" loading={s.busy} disabled={s.busy} onPress={() => s.need({ type: 'vote', id: r.id, vote: 'confirm', then: 'report' }, 'ikut memastikan laporan')}>
        Ya, sama. Pastikan laporan ini
      </Button>
      <Button mode="outlined" disabled={s.busy} onPress={() => s.need({ type: 'submit', cat: e.cat, at: e.at }, 'mengirim laporan')}>
        Bukan, kirim laporan baru
      </Button>
    </>
  );
}

export function SentSheet() {
  const s = useStore();
  const t = useAppTheme();
  const e = s.nav.current as Extract<SheetEntry, { view: 'sent' }>;
  const [sev, setSev] = useState('');
  return (
    <>
      <View style={{ alignItems: 'center', gap: 8 }} accessibilityLiveRegion="polite">
        <MaterialCommunityIcons name="check-circle" size={48} color={t.colors.primary} />
        <Text variant="titleLarge" style={{ textAlign: 'center' }} accessibilityRole="header">
          Laporan {CATEGORIES[e.cat].label.toLowerCase()} terkirim
        </Text>
        <Muted style={{ textAlign: 'center' }}>Warga di sekitar akan diminta memastikan. Kami kabari kalau sudah dipastikan.</Muted>
      </View>
      <Divider />
      <Text variant="titleSmall">
        Tambah detail <Text style={{ fontWeight: '400', color: t.colors.onSurfaceVariant }}>(opsional, bikin lebih cepat dipercaya)</Text>
      </Text>
      <SegmentedButtons
        value={sev}
        onValueChange={(v) => {
          setSev(v);
          // No endpoint to update a report after sending yet (see README gaps); kept on screen only.
        }}
        buttons={[
          { value: '1', label: 'Ringan', showSelectedCheck: true },
          { value: '2', label: 'Sedang', showSelectedCheck: true },
          { value: '3', label: 'Berat', showSelectedCheck: true },
        ]}
      />
      <Button mode="outlined" icon="camera-outline" onPress={() => s.showToast('Kirim foto belum tersedia di versi ini')}>
        Tambah foto
      </Button>
      <Button
        mode="contained"
        onPress={() => {
          s.home();
          const r = s.data.reports.find((x) => x.id === e.id);
          if (r) s.mapApi.current?.flyTo(r, false);
        }}
      >
        Selesai
      </Button>
    </>
  );
}

const q = StyleSheet.create({
  loc: { flexDirection: 'row', alignItems: 'center', gap: 16, minHeight: 56 },
  grid: { flexDirection: 'row', flexWrap: 'wrap', gap: 8 },
  tile: { flexBasis: '47%', flexGrow: 1, minHeight: 96, borderWidth: 1, borderRadius: 12, padding: 16, gap: 4 },
  dup: { flexDirection: 'row', alignItems: 'center', gap: 16, borderWidth: 1, borderRadius: 12, paddingVertical: 12, paddingHorizontal: 16 },
});
