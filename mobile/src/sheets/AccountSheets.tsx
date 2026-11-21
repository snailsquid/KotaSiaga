import React, { useState } from 'react';
import { View } from 'react-native';
import { Avatar, Button, Chip, Dialog, IconButton, Menu, Portal, SegmentedButtons, Text } from 'react-native-paper';
import { BottomSheetTextInput } from '@gorhom/bottom-sheet';
import { MaterialCommunityIcons } from '@expo/vector-icons';
import { useAuth } from '../auth/AuthProvider';
import { ApiService } from '../api/ApiService';
import type { Subscription } from '../api/types';
import { agoShort, formatDistance } from '../domain/format';
import { RISK_LABEL } from '../domain/risk';
import { useStore } from '../state/AppStore';
import type { SheetEntry } from '../state/types';
import { RiskBadge } from '../components/RiskBadge';
import { InlineError, Muted, Row, SectionLabel, SheetHeader, styles, useAppTheme } from './common';

function subRisk(s: ReturnType<typeof useStore>, sub: Subscription) {
  return s.data.cells.get(ApiService.cellAt(sub))?.risk_level ?? null;
}

// ---------------------------------------------------------------- alerts (spec §7.9, §12.10)
export function AlertsSheet() {
  const s = useStore();
  const t = useAppTheme();
  const subName = (id: string) => s.subscriptions.find((x) => x.id === id)?.name ?? 'Area pantauan';
  const mineCount = s.myReports.size;
  const votes = Object.keys(s.myVotes).length;
  return (
    <>
      <View style={{ flexDirection: 'row', justifyContent: 'space-between', alignItems: 'center' }}>
        <Text variant="titleLarge" accessibilityRole="header">
          Notifikasi
        </Text>
        <Button mode="text" icon="account-circle" onPress={() => s.go({ view: 'account' })}>
          Akun
        </Button>
      </View>
      {/* Weekly contribution summary. Counts come from this session; no contract endpoint yet. */}
      <View style={[styles.card, { backgroundColor: t.colors.surfaceContainer, flexDirection: 'row', alignItems: 'center' }]}>
        <MaterialCommunityIcons name="trophy-award" size={32} color={t.colors.primary} />
        <View style={{ flex: 1 }}>
          <Text variant="titleMedium">{mineCount >= 5 ? 'Pelapor Aktif' : 'Warga Baru'}</Text>
          <Muted>
            Minggu ini: {mineCount} laporan, {votes} kali memastikan.
          </Muted>
        </View>
      </View>
      <View style={{ gap: 8 }}>
        <SectionLabel>Area pantauan</SectionLabel>
        <View style={styles.buttonsRow}>
          {s.subscriptions.map((sub) => {
            const lvl = subRisk(s, sub);
            return (
              <Chip key={sub.id} mode="outlined" icon="bell-outline" onPress={() => s.go({ view: 'subscriptions' })}>
                {sub.name}
                {lvl ? ` · ${RISK_LABEL[lvl]}` : ''}
              </Chip>
            );
          })}
          <Chip mode="outlined" icon="plus" onPress={() => s.runPending({ type: 'watch', center: s.focus })} textStyle={{ color: t.colors.primary }}>
            Tambah
          </Chip>
        </View>
      </View>
      {s.alerts.length === 0 ? (
        <>
          <Muted>Belum ada notifikasi. Pantau area untuk menerima peringatan.</Muted>
          <Button mode="outlined" style={{ alignSelf: 'flex-start' }} onPress={() => s.runPending({ type: 'watch', center: s.focus })}>
            Pantau area
          </Button>
        </>
      ) : (
        <View>
          {s.alerts.map((a) => {
            return (
              <Row
                key={a.id}
                icon={a.risk_level === 'danger' ? 'alert-octagon' : 'alert'}
                title={`Risiko ${RISK_LABEL[a.risk_level]} di ${subName(a.subscription_id)}`}
                subtitle={`${a.message} · ${agoShort(a.at)}`}
                bold={!s.isAlertRead(a.id)}
                onPress={() => {
                  s.markAlertRead(a.id);
                  const at = ApiService.cellCenter(a.cell_id);
                  if (at) {
                    s.go({ view: 'cell', cellId: a.cell_id, at });
                    s.mapApi.current?.flyTo(at);
                  }
                }}
              />
            );
          })}
        </View>
      )}
    </>
  );
}

// ---------------------------------------------------------------- account (spec §7.11)
export function AccountSheet() {
  const s = useStore();
  const auth = useAuth();
  const [confirm, setConfirm] = useState(false);
  const t = useAppTheme();
  if (!auth.user) {
    return (
      <>
        <SheetHeader title="Akun" onClose={s.back} />
        <Button mode="contained" onPress={() => s.need({ type: 'alerts' }, 'melihat notifikasi')}>
          Masuk dengan Google
        </Button>
      </>
    );
  }
  return (
    <>
      <SheetHeader title="Akun" onClose={s.back} />
      <View style={{ flexDirection: 'row', alignItems: 'center', gap: 16 }}>
        {auth.user.photoUrl ? (
          <Avatar.Image size={48} source={{ uri: auth.user.photoUrl }} />
        ) : (
          <Avatar.Text size={48} label={auth.user.name.slice(0, 1).toUpperCase()} style={{ backgroundColor: t.colors.primary }} />
        )}
        <View style={{ flex: 1 }}>
          <Text variant="titleMedium">{auth.user.name}</Text>
          <Muted>{auth.user.email}</Muted>
        </View>
      </View>
      <Muted>
        Level: {s.myReports.size >= 5 ? 'Pelapor Aktif' : 'Warga Baru'} · {s.myReports.size} laporan
      </Muted>
      <View>
        <Row icon="map-marker-radius-outline" title="Area pantauan" onPress={() => s.go({ view: 'subscriptions' })} />
        <Row icon="bell-outline" title="Notifikasi" onPress={() => s.go({ view: 'alerts' })} />
        <Row icon="information-outline" title="Tentang aplikasi" onPress={() => s.go({ view: 'about' })} />
      </View>
      <Button mode="outlined" icon="logout" style={{ alignSelf: 'flex-start' }} onPress={() => setConfirm(true)}>
        Keluar
      </Button>
      <Portal>
        <Dialog visible={confirm} onDismiss={() => setConfirm(false)}>
          <Dialog.Title>Keluar dari akun?</Dialog.Title>
          <Dialog.Actions>
            <Button onPress={() => setConfirm(false)}>Batal</Button>
            <Button
              onPress={async () => {
                setConfirm(false);
                await auth.signOut();
                s.home(); // account-only sheets close
              }}
            >
              Keluar
            </Button>
          </Dialog.Actions>
        </Dialog>
      </Portal>
    </>
  );
}

// ---------------------------------------------------------------- subscriptions (spec §7.10)
const RADII = [
  { value: '500', label: '500 m' },
  { value: '1000', label: '1 km' },
  { value: '2000', label: '2 km' },
];

export function SubscriptionsSheet() {
  const s = useStore();
  const t = useAppTheme();
  const e = s.nav.current as Extract<SheetEntry, { view: 'subscriptions' }>;
  const [menu, setMenu] = useState<string | null>(null);
  const [name, setName] = useState(e.add?.name ?? '');
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);

  React.useEffect(() => {
    if (e.add) {
      s.mapApi.current?.flyTo(e.add.center, false);
      setName(e.add.name ?? '');
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [e.add?.center.lat, e.add?.center.lon]);

  const remove = async (sub: Subscription) => {
    setMenu(null);
    try {
      await ApiService.deleteSubscription(sub.id);
      s.refreshAccountData();
      s.showToast('Area dihapus', {
        label: 'Urungkan',
        onPress: () => {
          ApiService.createSubscription({ name: sub.name, lat: sub.lat, lon: sub.lon, radius_m: sub.radius_m })
            .then(() => s.refreshAccountData())
            .catch(() => s.showToast('Gagal mengembalikan area'));
        },
      });
    } catch {
      s.showToast('Gagal menghapus. Coba lagi.');
    }
  };

  if (e.add) {
    return (
      <>
        <SheetHeader title="Tambah area pantauan" onClose={s.back} />
        <Muted>Geser peta untuk menaruh lingkaran di area yang ingin kamu pantau.</Muted>
        <SegmentedButtons value={String(s.subRadius)} onValueChange={(v) => s.setSubRadius(Number(v))} buttons={RADII.map((r) => ({ ...r, showSelectedCheck: true }))} />
        <BottomSheetTextInput
          value={name}
          onChangeText={setName}
          placeholder="Nama area, misalnya Rumah"
          placeholderTextColor={t.colors.onSurfaceVariant}
          maxLength={60}
          accessibilityLabel="Nama area"
          style={{ minHeight: 56, borderWidth: 1, borderColor: t.colors.outline, borderRadius: 4, paddingHorizontal: 16, fontSize: 16, color: t.colors.onSurface }}
        />
        {error ? <InlineError message={error} /> : null}
        <Button
          mode="contained"
          loading={saving}
          disabled={!name.trim() || saving}
          onPress={async () => {
            setSaving(true);
            setError(null);
            try {
              await ApiService.createSubscription({ name: name.trim(), lat: s.liveCenter.lat, lon: s.liveCenter.lon, radius_m: s.subRadius });
              await s.refreshAccountData();
              s.showToast('Area ini sekarang kamu pantau');
              s.dispatch({ type: 'replace', to: { view: 'subscriptions' } });
            } catch {
              setError('Gagal menyimpan. Masukanmu tetap ada, coba lagi.');
            } finally {
              setSaving(false);
            }
          }}
        >
          Simpan
        </Button>
      </>
    );
  }

  return (
    <>
      <SheetHeader title="Area pantauan" onClose={s.back} />
      {s.subscriptions.length === 0 ? <Muted>Belum ada area. Tambahkan rumah, kantor, atau sekolah untuk menerima peringatan.</Muted> : null}
      <View>
        {s.subscriptions.map((sub) => {
          const lvl = subRisk(s, sub);
          return (
            <Row
              key={sub.id}
              icon="bell-outline"
              title={sub.name}
              subtitle={`Radius ${formatDistance(sub.radius_m)}`}
              onPress={() => s.mapApi.current?.flyTo(sub, false)}
              trailing={
                <View style={{ flexDirection: 'row', alignItems: 'center' }}>
                  {lvl ? <RiskBadge level={lvl} small /> : null}
                  <Menu visible={menu === sub.id} onDismiss={() => setMenu(null)} anchor={<IconButton icon="dots-vertical" accessibilityLabel={`Pilihan untuk ${sub.name}`} onPress={() => setMenu(sub.id)} />}>
                    <Menu.Item leadingIcon="delete-outline" title="Hapus" onPress={() => remove(sub)} />
                  </Menu>
                </View>
              }
            />
          );
        })}
      </View>
      <Button mode="contained" icon="plus" style={{ alignSelf: 'flex-start' }} onPress={() => s.dispatch({ type: 'replace', to: { view: 'subscriptions', add: { center: s.focus } }, snap: 1 })}>
        Tambah area
      </Button>
    </>
  );
}
