import React, { useState } from 'react';
import { Pressable, View } from 'react-native';
import { Button, Checkbox, Divider, RadioButton, Text } from 'react-native-paper';
import { MaterialCommunityIcons } from '@expo/vector-icons';
import { useAuth } from '../auth/AuthProvider';
import { ApiService } from '../api/ApiService';
import { CATEGORIES, CATEGORY_ORDER } from '../domain/categories';
import { distanceM } from '../domain/geo';
import { ago } from '../domain/format';
import { RISK_LABEL, riskWhy } from '../domain/risk';
import { config } from '../config';
import { isActive, matchesFilter, useStore } from '../state/AppStore';
import type { Filters, SheetEntry, StatusFilter } from '../state/types';
import { RiskBadge } from '../components/RiskBadge';
import { DISASTER_LABEL } from '../map/KsMap';
import { Muted, ReportRow, SheetHeader, styles, useAppTheme, HIT } from './common';

// ---------------------------------------------------------------- sign_in (spec §8)
export function SignInSheet() {
  const s = useStore();
  const auth = useAuth();
  const e = s.nav.current as Extract<SheetEntry, { view: 'signin' }>;
  const [busy, setBusy] = useState(false);
  const t = useAppTheme();
  return (
    <>
      <Text variant="titleLarge" accessibilityRole="header">
        Masuk untuk {e.reason}
      </Text>
      <Muted>Akun membuat laporanmu lebih dipercaya warga lain. Namamu tidak ditampilkan. Di peta kamu tampil sebagai “Warga”.</Muted>
      <Button hitSlop={HIT}
        mode="outlined"
        icon={() => <MaterialCommunityIcons name="google" size={18} color={t.colors.onSurface} />}
        textColor={t.colors.onSurface}
        loading={busy}
        disabled={busy}
        onPress={async () => {
          setBusy(true);
          const ok = await auth.signIn();
          setBusy(false);
          if (!ok) {
            s.showToast('Gagal masuk. Coba lagi.');
            return;
          }
          // Land exactly where the user was, then continue the action by itself (spec §8.2).
          s.dispatch({ type: 'back' });
          s.runPending(e.pending);
        }}
      >
        Masuk dengan Google
      </Button>
      <Button hitSlop={HIT} mode="text" onPress={() => s.back()} disabled={busy}>
        Nanti saja
      </Button>
    </>
  );
}

// ---------------------------------------------------------------- report_list (spec §7.7)
const PAGE = 20;
export function ListSheet() {
  const s = useStore();
  const [shown, setShown] = useState(PAGE);
  const from = s.me ?? s.focus;
  const all = s.data.reports.filter((r) => isActive(r));
  const list = all.filter((r) => matchesFilter(r, s.filters)).sort((a, b) => distanceM(from, a) - distanceM(from, b));
  const filtered = s.filters.cats.length > 0 || s.filters.status !== 'all';
  return (
    <>
      <View style={{ flexDirection: 'row', justifyContent: 'space-between', alignItems: 'baseline' }}>
        <Text variant="titleLarge" accessibilityRole="header">
          Laporan di sekitar
        </Text>
        <Muted variant="bodySmall">{list.length} laporan</Muted>
      </View>
      {list.length === 0 ? (
        filtered ? (
          <>
            <Muted>Tidak ada laporan yang cocok dengan filter.</Muted>
            <Button hitSlop={HIT} mode="outlined" style={{ alignSelf: 'flex-start' }} onPress={() => s.setFilters({ cats: [], status: 'all' })}>
              Hapus filter
            </Button>
          </>
        ) : (
          <Muted>Belum ada laporan di area ini.</Muted>
        )
      ) : (
        <View>
          {list.slice(0, shown).map((r) => (
            <ReportRow key={r.id} r={r} />
          ))}
          {shown < list.length ? (
            <Button hitSlop={HIT} mode="text" onPress={() => setShown((n) => n + PAGE)}>
              Muat lebih banyak
            </Button>
          ) : null}
        </View>
      )}
    </>
  );
}

// ---------------------------------------------------------------- filter sheet (spec §13)
const STATUS: [StatusFilter, string][] = [
  ['all', 'Semua'],
  ['verified', 'Sudah dipastikan'],
  ['waiting', 'Perlu dipastikan'],
];

export function FilterSheet() {
  const s = useStore();
  const [draft, setDraft] = useState<Filters>(s.filters);
  const active = s.data.reports.filter((r) => isActive(r));
  const n = active.filter((r) => matchesFilter(r, draft)).length;
  const t = useAppTheme();
  return (
    <>
      <View style={{ flexDirection: 'row', justifyContent: 'space-between', alignItems: 'center' }}>
        <Text variant="titleLarge" accessibilityRole="header">
          Filter laporan
        </Text>
        <Button hitSlop={HIT} mode="text" onPress={() => setDraft({ cats: [], status: 'all' })}>
          Atur ulang
        </Button>
      </View>
      <View accessibilityRole="list" accessibilityLabel="Jenis kejadian">
        <Text variant="titleSmall" style={{ color: t.colors.onSurfaceVariant, paddingBottom: 4 }}>
          Jenis kejadian
        </Text>
        {CATEGORY_ORDER.map((k) => {
          const on = draft.cats.includes(k);
          const count = active.filter((r) => r.category === k).length;
          const toggle = () => setDraft({ ...draft, cats: on ? draft.cats.filter((x) => x !== k) : [...draft.cats, k] });
          return (
            <Pressable
              key={k}
              onPress={toggle}
              accessibilityRole="checkbox"
              accessibilityState={{ checked: on }}
              accessibilityLabel={`${CATEGORIES[k].label}, ${count} laporan`}
              android_ripple={{ color: t.colors.surfaceContainerHighest }}
              style={{ flexDirection: 'row', alignItems: 'center', gap: 16, minHeight: 56 }}
            >
              <MaterialCommunityIcons name={CATEGORIES[k].icon as any} size={24} color={t.colors.onSurfaceVariant} />
              <Text variant="bodyLarge" style={{ flex: 1 }}>
                {CATEGORIES[k].label}
              </Text>
              <Muted>{count}</Muted>
              <View importantForAccessibility="no-hide-descendants" accessibilityElementsHidden>
                <Checkbox status={on ? 'checked' : 'unchecked'} onPress={toggle} />
              </View>
            </Pressable>
          );
        })}
      </View>
      <Divider />
      <View>
        <Text variant="titleSmall" style={{ color: t.colors.onSurfaceVariant, paddingBottom: 4 }}>
          Status
        </Text>
        <RadioButton.Group value={draft.status} onValueChange={(v) => setDraft({ ...draft, status: v as StatusFilter })}>
          {STATUS.map(([v, l]) => (
            <RadioButton.Item key={v} value={v} label={l} labelVariant="bodyLarge" style={{ paddingHorizontal: 0, minHeight: 56 }} />
          ))}
        </RadioButton.Group>
      </View>
      <Button hitSlop={HIT}
        mode="contained"
        disabled={n === 0}
        onPress={() => {
          s.setFilters(draft);
          s.home();
        }}
      >
        {n === 0 ? 'Tidak ada laporan' : `Tampilkan ${n} laporan`}
      </Button>
    </>
  );
}

// ---------------------------------------------------------------- cell (spec §7.2)
export function CellSheet() {
  const s = useStore();
  const e = s.nav.current as Extract<SheetEntry, { view: 'cell' }>;
  const cell = s.data.cells.get(e.cellId) ?? null;
  const changedAt = s.data.riskChangedAt.get(e.cellId);
  const reports = s.data.reports.filter((r) => isActive(r) && distanceM(e.at, r) <= 500).slice(0, 5);
  const d = ApiService.describe(e.at);
  return (
    <>
      <SheetHeader title="Area ini" subtitle={`${d.street ? d.street + ', ' : ''}${d.district}`} onClose={s.home} />
      {cell ? (
        <View style={{ flexDirection: 'row', alignItems: 'center', gap: 8 }}>
          <RiskBadge level={cell.risk_level} />
          {changedAt ? <Muted>Berubah {ago(changedAt)}</Muted> : null}
        </View>
      ) : null}
      <Muted>{cell ? riskWhy(cell) : 'Data risiko belum tersedia.'}</Muted>
      {reports.length ? (
        <View>
          {reports.map((r) => (
            <ReportRow key={r.id} r={r} />
          ))}
          <Button hitSlop={HIT} mode="text" compact style={{ alignSelf: 'flex-start' }} onPress={() => s.go({ view: 'list' })}>
            Lihat semua
          </Button>
        </View>
      ) : null}
      <View style={styles.buttonsRow}>
        <Button hitSlop={HIT} mode="contained" icon="directions" onPress={() => s.go({ view: 'route', dest: { kind: 'pin', name: d.street ?? 'Area ini', lat: e.at.lat, lon: e.at.lon }, origin: null })}>
          Rute ke sini
        </Button>
        <Button hitSlop={HIT} mode="outlined" icon="bell-outline" onPress={() => s.need({ type: 'watch', center: e.at, name: d.street ?? d.district }, 'memantau area')}>
          Pantau area ini
        </Button>
      </View>
    </>
  );
}

// ---------------------------------------------------------------- disaster (spec §7.4)
export function DisasterSheet() {
  const s = useStore();
  const e = s.nav.current as Extract<SheetEntry, { view: 'disaster' }>;
  const d = s.data.disasters.find((x) => x.id === e.id);
  if (!d) {
    return (
      <>
        <SheetHeader title="Sinyal bencana" onClose={s.home} />
        <Muted>Sinyal ini sudah selesai.</Muted>
      </>
    );
  }
  return (
    <>
      <SheetHeader title={DISASTER_LABEL[d.type] ?? 'Sinyal bencana'} subtitle={`Sumber: PetaBencana · mulai ${ago(d.reported_at)}`} onClose={s.home} />
      <Text variant="titleMedium">{d.status === 'active' ? 'Aktif' : `Selesai ${d.resolved_at ? ago(d.resolved_at) : ''}`}</Text>
      <Muted>Area yang terdampak ditandai di peta. Rute yang disarankan otomatis menghindari area ini.</Muted>
      <Button hitSlop={HIT} mode="contained" icon="directions" style={{ alignSelf: 'flex-start' }} onPress={() => s.go({ view: 'search' })}>
        Rute menghindari area ini
      </Button>
    </>
  );
}

// ---------------------------------------------------------------- first launch (spec §6)
export function LocationIntroSheet() {
  const s = useStore();
  return (
    <>
      <MaterialCommunityIcons name="crosshairs-gps" size={32} color={useAppTheme().colors.primary} />
      <Text variant="titleLarge" accessibilityRole="header">
        Lihat risiko di sekitarmu
      </Text>
      <Muted>KotaSiaga memakai lokasimu untuk menunjukkan risiko di sekitarmu.</Muted>
      {ApiService.isMock && !config.useDeviceLocation ? (
        <Muted variant="bodySmall">Mode demo: posisimu ditaruh di Bandung (dekat Jl. Pasteur).</Muted>
      ) : null}
      <Button hitSlop={HIT}
        mode="contained"
        onPress={async () => {
          const ok = await s.requestLocation();
          s.home();
          if (ok && s.me) s.mapApi.current?.flyTo(s.me);
        }}
      >
        Izinkan lokasi
      </Button>
      <Button hitSlop={HIT} mode="text" onPress={s.home}>
        Nanti saja
      </Button>
    </>
  );
}

export function AboutSheet() {
  const s = useStore();
  return (
    <>
      <SheetHeader title="Tentang aplikasi" onClose={s.back} />
      <Muted>KotaSiaga menunjukkan risiko di jalan dari laporan warga, cuaca, dan sinyal bencana resmi. Warga di sekitar memastikan setiap laporan.</Muted>
      <Muted variant="bodySmall">
        Sumber data: {ApiService.isMock ? 'data demo (MOCK)' : 'KotaSiaga'} · Laporan dipastikan oleh warga dalam 500 m · Risiko: {RISK_LABEL.safe}, {RISK_LABEL.caution}, {RISK_LABEL.danger}.
      </Muted>
    </>
  );
}
