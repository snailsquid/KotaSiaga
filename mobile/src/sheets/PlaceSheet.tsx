import React, { useEffect } from 'react';
import { View } from 'react-native';
import { Button, Text } from 'react-native-paper';
import type { RiskLevel } from '../api/types';
import { distanceM } from '../domain/geo';
import { formatDistance } from '../domain/format';
import { RISK_LABEL } from '../domain/risk';
import { useStore } from '../state/AppStore';
import type { PlaceRef, SheetEntry } from '../state/types';
import { RiskBadge } from '../components/RiskBadge';
import { Muted, Placeholder, SheetHeader, styles, useAppTheme, HIT } from './common';

/** Place sheet (spec §7.5 + §12b.3): name, address, Rute / Pantau / Bagikan, and the way there. */
export function PlaceSheet() {
  const s = useStore();
  const t = useAppTheme();
  const p = (s.nav.current as Extract<SheetEntry, { view: 'place' }>).place;
  const from = s.me ?? s.focus;
  const dist = formatDistance(distanceM(from, p));

  // "Kondisi menuju ke sana" comes from the same safe-route call the route sheet uses.
  useEffect(() => {
    s.loadRoutes(from, p);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [p.lat, p.lon]);

  const rr = s.routes.result;
  const rec = rr?.routes.find((r) => r.id === rr.recommended_id);
  const fastest = rr ? [...rr.routes].sort((a, b) => a.duration_s - b.duration_s)[0] : undefined;
  const condition: { level: RiskLevel; note: string } | null = rec
    ? fastest && fastest.id !== rec.id && fastest.max_risk_level !== 'safe'
      ? { level: fastest.max_risk_level, note: `Jalan tercepat melewati area ${RISK_LABEL[fastest.max_risk_level]}. ${rec.reasons[0] ?? 'Rute yang disarankan lebih aman.'}` }
      : { level: rec.max_risk_level, note: rec.reasons[0] ?? 'Tidak melewati area Waspada atau Bahaya.' }
    : null;

  const subtitle = [p.address, p.kind === 'me' ? null : dist].filter(Boolean).join(' · ');

  return (
    <>
      <SheetHeader title={p.name} subtitle={subtitle} onClose={s.home} />
      {p.kind === 'tourism' && p.category ? <Muted>Destinasi wisata · {p.category}</Muted> : null}
      {p.kind === 'donor' ? (
        <Muted>
          Donor darah{p.opens_at ? ` · buka ${p.opens_at}–${p.closes_at}` : ''}
          {p.free_slots !== undefined ? ` · ${p.free_slots} slot tersisa` : ''}
        </Muted>
      ) : null}
      <View style={styles.buttonsRow}>
        <Button hitSlop={HIT} mode="contained" icon="directions" onPress={() => s.go({ view: 'route', dest: p, origin: null })}>
          Rute
        </Button>
        <Button hitSlop={HIT} mode="outlined" icon="bell-outline" onPress={() => s.need({ type: 'watch', center: p, name: p.kind === 'pin' ? undefined : p.name }, 'memantau area')}>
          Pantau
        </Button>
        <Button hitSlop={HIT} mode="outlined" icon="share-variant" onPress={() => s.share(shareText(p))}>
          Bagikan
        </Button>
      </View>
      <View style={[styles.card, { backgroundColor: t.colors.surfaceContainer, gap: 8 }]}>
        {condition ? (
          <>
            <View style={{ flexDirection: 'row', alignItems: 'center', gap: 8 }}>
              <RiskBadge level={condition.level} />
              <Text variant="titleMedium">Kondisi menuju ke sana</Text>
            </View>
            <Muted>{condition.note}</Muted>
          </>
        ) : s.routes.error ? (
          <>
            <Text variant="titleMedium">Kondisi menuju ke sana</Text>
            <Muted>Data risiko belum tersedia.</Muted>
          </>
        ) : (
          <Placeholder rows={1} />
        )}
      </View>
      <Button hitSlop={HIT} mode="text" icon="map-marker-plus-outline" style={{ alignSelf: 'flex-start' }} onPress={() => s.go({ view: 'quick', at: p })}>
        Laporkan di sini
      </Button>
    </>
  );
}

function shareText(p: PlaceRef) {
  return `${p.name}${p.address ? ', ' + p.address : ''}\nhttps://www.google.com/maps/search/?api=1&query=${p.lat},${p.lon}`;
}
