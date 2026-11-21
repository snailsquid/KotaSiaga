import React from 'react';
import { Pressable, StyleSheet, View } from 'react-native';
import { Button, Text } from 'react-native-paper';
import { MaterialCommunityIcons } from '@expo/vector-icons';
import { ApiService } from '../api/ApiService';
import type { Report } from '../api/types';
import { CATEGORIES } from '../domain/categories';
import { distanceM } from '../domain/geo';
import { formatDistance, hourLabel } from '../domain/format';
import { WEATHER, riskHeadline, riskWhy } from '../domain/risk';
import { canConfirm, isActive, matchesFilter, placeLabel, useStore } from '../state/AppStore';
import { RiskBadge } from '../components/RiskBadge';
import { Muted, Placeholder, ReportRow, SectionLabel, styles, useAppTheme, HIT } from './common';

/** The report nearby that the user can help confirm (spec §12.2). */
export function useVerifyTarget(): Report | null {
  const s = useStore();
  if (!s.me) return null;
  const me = s.me;
  const list = s.data.reports
    .filter((r) => r.status === 'waiting' && !s.dismissed.has(r.id) && canConfirm(r, me, s.myReports.has(r.id), !!s.myVotes[r.id]))
    .sort((a, b) => distanceM(me, a) - distanceM(me, b));
  return list[0] ?? null;
}

export function VerifyCard({ r }: { r: Report }) {
  const s = useStore();
  const t = useAppTheme();
  const cat = CATEGORIES[r.category];
  const dist = s.me ? formatDistance(distanceM(s.me, r)) : '';
  const vote = (v: 'confirm' | 'dispute') => s.need({ type: 'vote', id: r.id, vote: v }, 'ikut memastikan laporan');
  return (
    <View style={[styles.card, { backgroundColor: t.colors.surfaceContainer }]} accessibilityRole="summary">
      <View style={{ gap: 4 }}>
        <View style={{ flexDirection: 'row', alignItems: 'center', gap: 4 }}>
          <MaterialCommunityIcons name="help-circle-outline" size={16} color={t.colors.onSurfaceVariant} />
          <Muted variant="labelMedium">Bantu pastikan · {dist} dari kamu</Muted>
        </View>
        <Text variant="titleMedium">{cat.ask(placeLabel(r))}</Text>
      </View>
      <View style={{ flexDirection: 'row', gap: 8 }}>
        <Button hitSlop={HIT} mode="contained" style={{ flex: 1 }} onPress={() => vote('confirm')} disabled={s.busy}>
          {cat.yes}
        </Button>
        <Button hitSlop={HIT} mode="outlined" style={{ flex: 1 }} onPress={() => vote('dispute')} disabled={s.busy}>
          {cat.no}
        </Button>
      </View>
      <Button hitSlop={HIT} mode="text" compact onPress={() => s.dismiss(r.id)} style={{ alignSelf: 'center', marginTop: -4 }}>
        Tidak tahu
      </Button>
    </View>
  );
}

export function HomeSheet() {
  const s = useStore();
  const t = useAppTheme();
  const st = s.data.status;
  const half = s.nav.current.snap > 0;
  const target = useVerifyTarget();
  const area = ApiService.describe(s.focus).district;
  const cell = st ? s.data.cells.get(st.cell_id) ?? null : null;
  const riskMissing = !st || s.data.statusError;
  const w = st?.weather;
  const nearby = s.data.reports
    .filter((r) => isActive(r) && matchesFilter(r, s.filters))
    .sort((a, b) => distanceM(s.focus, a) - distanceM(s.focus, b))
    .slice(0, 3);

  return (
    <>
      {target ? <VerifyCard r={target} /> : null}

      <Pressable
        onPress={() => s.dispatch({ type: 'snap', snap: half ? 0 : 1 })}
        accessibilityRole="button"
        accessibilityLabel={
          riskMissing
            ? `${s.me ? 'Di sekitarmu' : 'Area di tengah peta'}, ${area}. Data risiko belum tersedia.`
            : `${s.me ? 'Di sekitarmu' : 'Area di tengah peta'}, ${area}. ${riskHeadline(cell)}. ${w ? `${Math.round(w.temperature_c)} derajat, ${WEATHER[w.condition].label}` : ''}`
        }
        accessibilityHint={half ? 'Perkecil panel' : 'Lihat detail'}
        style={home.summary}
      >
        <View style={{ flex: 1, gap: 4 }}>
          <Muted variant="labelMedium">
            {s.me ? 'Di sekitarmu' : 'Area di tengah peta'} · {area}
          </Muted>
          {!s.data.loaded ? (
            <Text variant="titleLarge">Memuat…</Text>
          ) : riskMissing ? (
            <Text variant="titleMedium">Data risiko belum tersedia</Text>
          ) : (
            <View style={{ flexDirection: 'row', alignItems: 'center', gap: 8, flexWrap: 'wrap' }}>
              <RiskBadge level={st!.risk_level} />
              <Text variant="titleLarge" style={{ flexShrink: 1 }}>
                {riskHeadline(cell ?? { cell_id: st!.cell_id, risk_level: st!.risk_level, score: 0, factors: [] })}
              </Text>
            </View>
          )}
        </View>
        {w ? (
          <View style={{ alignItems: 'flex-end' }}>
            <View style={{ flexDirection: 'row', alignItems: 'center', gap: 4 }}>
              <MaterialCommunityIcons name={WEATHER[w.condition].icon as any} size={24} color={t.colors.onSurfaceVariant} />
              <Text variant="titleLarge">{Math.round(w.temperature_c)}°</Text>
            </View>
            <Muted variant="bodySmall">{WEATHER[w.condition].label}</Muted>
          </View>
        ) : null}
      </Pressable>

      {half ? (
        <>
          <Muted>{riskMissing ? 'Data risiko belum tersedia. Cuaca dan laporan tetap ditampilkan.' : riskWhy(cell)}</Muted>
          {w ? <Forecast w={w} /> : <Placeholder rows={1} />}
          {st ? (
            <Muted variant="bodySmall">
              {st.active_reports} laporan aktif · {st.disaster_signals} sinyal bencana
            </Muted>
          ) : null}
          <View>
            <SectionLabel>Laporan terdekat</SectionLabel>
            {!s.data.loaded ? <Placeholder /> : nearby.length === 0 ? <Muted>Belum ada laporan di area ini.</Muted> : nearby.map((r) => <ReportRow key={r.id} r={r} />)}
            {nearby.length > 0 ? (
              <Button hitSlop={HIT} mode="text" compact onPress={() => s.go({ view: 'list' })} style={{ alignSelf: 'flex-start' }}>
                Lihat semua laporan
              </Button>
            ) : null}
          </View>
          <Button hitSlop={HIT}
            mode="outlined"
            icon="bell-outline"
            style={{ alignSelf: 'flex-start' }}
            onPress={() => s.need({ type: 'watch', center: s.focus, name: area }, 'memantau area')}
          >
            Pantau area ini
          </Button>
        </>
      ) : null}
    </>
  );
}

function Forecast({ w }: { w: NonNullable<ReturnType<typeof useStore>['data']['status']>['weather'] }) {
  const t = useAppTheme();
  const max = Math.max(10, ...w.forecast_6h.map((f) => f.rain_mm_h));
  return (
    <View
      style={[styles.outlined, { borderColor: t.colors.outlineVariant, backgroundColor: t.colors.surfaceContainerLowest }]}
      accessible
      accessibilityLabel={`Hujan 6 jam ke depan: ${w.forecast_6h.map((f) => `pukul ${hourLabel(f.at)}, ${Math.round(f.rain_mm_h)} milimeter per jam`).join('; ')}`}
    >
      <Text variant="titleSmall">
        {Math.round(w.temperature_c)}° {WEATHER[w.condition].label} · {Math.round(w.rain_mm_h)} mm/jam
      </Text>
      <Muted variant="bodySmall">Hujan 6 jam ke depan (mm/jam)</Muted>
      <View style={{ flexDirection: 'row', gap: 6, alignItems: 'flex-end' }}>
        {w.forecast_6h.map((f) => (
          <View key={f.at} style={{ flex: 1, alignItems: 'center', gap: 4 }}>
            <Muted variant="bodySmall">{Math.round(f.rain_mm_h)}</Muted>
            <View style={{ height: 40, width: '100%', justifyContent: 'flex-end' }}>
              <View style={{ height: Math.max(2, Math.round((f.rain_mm_h / max) * 40)), backgroundColor: t.colors.primary, borderTopLeftRadius: 4, borderTopRightRadius: 4 }} />
            </View>
            <Muted variant="bodySmall">{hourLabel(f.at)}</Muted>
          </View>
        ))}
      </View>
    </View>
  );
}

const home = StyleSheet.create({
  summary: { flexDirection: 'row', justifyContent: 'space-between', alignItems: 'center', gap: 12, minHeight: 56 },
});
