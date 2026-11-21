import React from 'react';
import { View } from 'react-native';
import { Button, Chip, ProgressBar, Text } from 'react-native-paper';
import { MaterialCommunityIcons } from '@expo/vector-icons';
import { CATEGORIES, CONFIRM_RADIUS_M, CONFIRM_TARGET, SEVERITY_LABEL } from '../domain/categories';
import { distanceM } from '../domain/geo';
import { ago, expiresIn, formatDistance } from '../domain/format';
import { isActive, placeLabel, reportTitle, useStore } from '../state/AppStore';
import type { SheetEntry } from '../state/types';
import { InlineError, Muted, SheetHeader, styles, useAppTheme } from './common';

/** Report detail (spec §7.3 with §12.1, §12.3, §12b.2). */
export function ReportSheet() {
  const s = useStore();
  const t = useAppTheme();
  const id = (s.nav.current as Extract<SheetEntry, { view: 'report' }>).id;
  const r = s.data.reports.find((x) => x.id === id);

  if (!r) {
    return (
      <>
        <SheetHeader title="Laporan tidak ditemukan" onClose={s.home} />
        <Muted>Laporan ini sudah kedaluwarsa atau dihapus.</Muted>
      </>
    );
  }

  const cat = CATEGORIES[r.category];
  const place = placeLabel(r);
  const distM = s.me ? distanceM(s.me, r) : null;
  const dist = distM !== null ? formatDistance(distM) : null;
  const mine = s.myReports.has(r.id);
  const vote = s.myVotes[r.id];
  const expired = !isActive(r);
  const verified = r.status === 'verified';
  const conf = r.confirm_count ?? 0;
  const near = distM !== null && distM <= CONFIRM_RADIUS_M;
  const canVote = !expired && !mine && !vote && near;
  const send = (v: 'confirm' | 'dispute') => s.need({ type: 'vote', id: r.id, vote: v }, 'ikut memastikan laporan');

  return (
    <View style={{ gap: 16, opacity: expired ? 0.6 : 1 }}>
      <SheetHeader title={reportTitle(r)} subtitle={[cat.label, place, dist].filter(Boolean).join(' · ')} onClose={s.home} />
      <View style={styles.buttonsRow}>
        <Chip mode="outlined" compact>
          Keparahan: {SEVERITY_LABEL[r.severity]}
        </Chip>
        <Chip mode="outlined" compact>
          Dilaporkan {ago(r.created_at)}
        </Chip>
      </View>
      {/* Public reports are anonymous (spec §12.8). Reporter level is not in the contract yet. */}
      <Muted>
        Oleh {mine ? 'kamu' : 'Warga'} · {expired ? 'sudah berakhir' : `otomatis hilang ${expiresIn(r.expires_at)}`}
      </Muted>

      <View style={[styles.card, { backgroundColor: t.colors.surfaceContainer }]}>
        {expired ? (
          <Text variant="titleMedium" accessibilityLiveRegion="polite">
            Laporan ini sudah kedaluwarsa
          </Text>
        ) : verified ? (
          <View style={{ flexDirection: 'row', alignItems: 'center', gap: 8 }} accessibilityLiveRegion="polite">
            <MaterialCommunityIcons name="check-decagram" size={20} color={t.colors.primary} />
            <Text variant="bodyMedium">
              <Text style={{ fontWeight: '500' }}>Dipastikan {conf} warga</Text>
              {r.verified_at ? ` · ${ago(r.verified_at)}` : ''}
            </Text>
          </View>
        ) : (
          <View style={{ gap: 6 }} accessibilityLiveRegion="polite">
            <Text variant="titleMedium">
              Dipastikan {Math.min(conf, CONFIRM_TARGET)} dari {CONFIRM_TARGET} warga
            </Text>
            <ProgressBar progress={Math.min(1, conf / CONFIRM_TARGET)} color={t.colors.primary} style={{ height: 4, borderRadius: 2, backgroundColor: t.colors.secondaryContainer }} accessibilityLabel="Kemajuan konfirmasi" />
            <Muted>Laporan ini tampil sebagai belum pasti sampai {CONFIRM_TARGET} warga di sekitar lokasi membenarkannya.</Muted>
          </View>
        )}

        {canVote ? (
          <>
            <Text variant="titleMedium">{cat.ask(place)}</Text>
            <View style={{ flexDirection: 'row', gap: 8 }}>
              <Button mode="contained" style={{ flex: 1 }} onPress={() => send('confirm')} disabled={s.busy}>
                {cat.yes}
              </Button>
              <Button mode="outlined" style={{ flex: 1 }} onPress={() => send('dispute')} disabled={s.busy}>
                {cat.no}
              </Button>
            </View>
          </>
        ) : null}
        {s.actionError && canVote ? <InlineError message={s.actionError} onRetry={s.clearActionError} /> : null}
        {!expired && !mine && !vote && !near ? (
          <Muted>
            Hanya warga dalam 500 m dari lokasi yang bisa ikut memastikan.{dist ? ` Kamu ${dist} dari sini.` : ' Izinkan lokasi untuk ikut memastikan.'}
          </Muted>
        ) : null}
        {vote ? (
          <Text variant="bodyMedium">
            <Text style={{ fontWeight: '500' }}>Kamu menjawab: {vote === 'confirm' ? cat.yes : cat.no}</Text> · Terima kasih, jawabanmu membantu warga lain.
          </Text>
        ) : null}
        {mine && !expired ? <Muted>Ini laporanmu. Warga di sekitar sedang diminta memastikan, kami kabari kalau sudah dipastikan.</Muted> : null}
      </View>

      <Button
        mode="outlined"
        icon="share-variant"
        style={{ alignSelf: 'flex-start' }}
        onPress={() => s.share(`${cat.label}: ${reportTitle(r)} di ${place} (KotaSiaga)\nhttps://www.google.com/maps/search/?api=1&query=${r.lat},${r.lon}`)}
      >
        Bagikan
      </Button>
    </View>
  );
}
