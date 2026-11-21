import React, { forwardRef } from 'react';
import { StyleSheet, TextInput, View, type TextInputInstance } from 'react-native';
import { Avatar, Badge, IconButton, TouchableRipple } from 'react-native-paper';
import { useSafeAreaInsets } from 'react-native-safe-area-context';
import { useAuth } from '../auth/AuthProvider';
import { useStore } from '../state/AppStore';
import { elevation3, theme } from '../theme/theme';

export const SEARCH_BAR_HEIGHT = 56;

/** MD3 search bar, always visible (spec §2). Left: menu on home, back elsewhere. Right: avatar or "Masuk". */
export const SearchBar = forwardRef<TextInputInstance, { query: string; onQuery: (q: string) => void; onMenu: () => void }>(function SearchBar(
  { query, onQuery, onMenu },
  inputRef,
) {
  const s = useStore();
  const auth = useAuth();
  const insets = useSafeAreaInsets();
  const view = s.nav.current.view;
  const isHome = view === 'home';
  const placeName =
    view === 'place' ? (s.nav.current as any).place.name : view === 'route' || view === 'route_detail' ? (s.nav.current as any).dest.name : null;
  const c = theme.colors;

  return (
    <View style={[styles.bar, { top: insets.top + 8 }]}>
      {isHome ? (
        <IconButton icon="menu" iconColor={c.onSurfaceVariant} onPress={onMenu} accessibilityLabel="Menu" style={styles.icon} />
      ) : (
        <IconButton
          icon="arrow-left"
          iconColor={c.onSurfaceVariant}
          onPress={() => {
            if (inputRef && typeof inputRef !== 'function') inputRef.current?.blur();
            if (view === 'search') onQuery('');
            s.back();
          }}
          accessibilityLabel="Kembali"
          style={styles.icon}
        />
      )}
      <TextInput
        ref={inputRef}
        value={view === 'search' ? query : ''}
        onChangeText={onQuery}
        placeholder={placeName ?? 'Cari tempat atau laporan'}
        placeholderTextColor={c.onSurfaceVariant}
        accessibilityLabel="Cari tempat atau laporan"
        onFocus={() => {
          if (view !== 'search') s.go({ view: 'search' });
        }}
        returnKeyType="search"
        style={styles.input}
      />
      <TouchableRipple
        borderless
        onPress={() => {
          // The avatar is the door to notifications (spec §12.13).
          s.need({ type: 'alerts' }, 'melihat notifikasi');
        }}
        accessibilityRole="button"
        accessibilityLabel={auth.signedIn ? `Notifikasi dan akun, ${s.unread} belum dibaca` : 'Masuk'}
        style={styles.avatarBtn}
      >
        <View>
          {auth.signedIn ? (
            <Avatar.Text size={30} label={(auth.user?.name ?? 'W').slice(0, 1).toUpperCase()} style={{ backgroundColor: c.primary }} color={c.onPrimary} />
          ) : (
            <Avatar.Icon size={34} icon="account-circle" color={c.onSurfaceVariant} style={{ backgroundColor: 'transparent' }} />
          )}
          {auth.signedIn && s.unread > 0 ? (
            <Badge size={16} style={styles.badge}>
              {s.unread}
            </Badge>
          ) : null}
        </View>
      </TouchableRipple>
    </View>
  );
});

const styles = StyleSheet.create({
  bar: {
    position: 'absolute',
    left: 16,
    right: 16,
    height: SEARCH_BAR_HEIGHT,
    borderRadius: 28,
    backgroundColor: theme.colors.surfaceContainerHigh,
    flexDirection: 'row',
    alignItems: 'center',
    paddingHorizontal: 4,
    gap: 4,
    ...elevation3,
  },
  icon: { margin: 0, width: 48, height: 48 },
  input: { flex: 1, minWidth: 0, fontSize: 16, lineHeight: 24, letterSpacing: 0.5, color: theme.colors.onSurface, paddingVertical: 0 },
  avatarBtn: { width: 48, height: 48, borderRadius: 24, alignItems: 'center', justifyContent: 'center' },
  badge: { position: 'absolute', top: -4, right: -6, backgroundColor: theme.colors.error },
});
