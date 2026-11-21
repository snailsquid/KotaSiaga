import React, { useEffect, useRef, useState } from 'react';
import { BackHandler, StyleSheet, View, useWindowDimensions, type TextInputInstance } from 'react-native';
import { Snackbar } from 'react-native-paper';
import { MaterialCommunityIcons } from '@expo/vector-icons';
import Animated, { useAnimatedStyle, useSharedValue } from 'react-native-reanimated';
import { useSafeAreaInsets } from 'react-native-safe-area-context';
import { StatusBar } from 'expo-status-bar';
import { KsMap } from './map/KsMap';
import { SearchBar, SEARCH_BAR_HEIGHT } from './components/SearchBar';
import { FilterRow, StatusStrip } from './components/FilterRow';
import { LayersMenu, MapControls } from './components/MapControls';
import { MenuDrawer } from './components/MenuDrawer';
import { SheetHost } from './sheets/SheetHost';
import { useVerifyTarget } from './sheets/HomeSheet';
import { useStore } from './state/AppStore';
import { theme } from './theme/theme';

const NO_CHIPS = new Set(['place', 'route', 'route_detail', 'search', 'filter', 'pick_location', 'subscriptions', 'quick', 'sent', 'signin']);
const CONTROLS = new Set(['home', 'report', 'route', 'route_detail', 'dup', 'place', 'cell', 'disaster']);

/** KotaSiaga is one map screen; everything else is a panel on top of it (spec §1). */
export function MapScreen() {
  const s = useStore();
  const insets = useSafeAreaInsets();
  const { height } = useWindowDimensions();
  const [query, setQuery] = useState('');
  const [menuOpen, setMenuOpen] = useState(false);
  const [layersOpen, setLayersOpen] = useState(false);
  const searchRef = useRef<TextInputInstance>(null);
  const sheetTop = useSharedValue(height);
  const view = s.nav.current.view;
  const snap = s.nav.current.snap;
  const verify = useVerifyTarget();

  // Sheet heights (spec §3.1): peek ~ one summary line, half ~45-50 %, full = just under the search bar.
  const top = insets.top + 8 + SEARCH_BAR_HEIGHT + 8;
  const peek = (view === 'home' && verify ? 316 : view === 'pick_location' ? 196 : 132) + insets.bottom;
  const half = Math.max(peek + 40, Math.round(height * 0.5));
  const full = Math.max(half + 40, height - top);
  const snapPoints = [peek, half, full];

  useEffect(() => {
    const sub = BackHandler.addEventListener('hardwareBackPress', () => {
      if (menuOpen) {
        setMenuOpen(false);
        return true;
      }
      if (layersOpen) {
        setLayersOpen(false);
        return true;
      }
      if (view === 'search') {
        searchRef.current?.blur();
        setQuery('');
      }
      return s.back();
    });
    return () => sub.remove();
  }, [menuOpen, layersOpen, view, s]);

  useEffect(() => {
    if (view !== 'search') searchRef.current?.blur();
    if (view !== 'home') setLayersOpen(false);
  }, [view]);

  const showChips = snap < 2 && !NO_CHIPS.has(view);
  const showControls = snap < 2 && CONTROLS.has(view) && !(view === 'home' && snap > 0);
  const toastStyle = useAnimatedStyle(() => ({ bottom: Math.max(0, height - sheetTop.value) + 8 }));

  return (
    <View style={styles.root}>
      <StatusBar style="dark" />
      <KsMap sheetHalfPx={half} />

      {view === 'pick_location' ? (
        <View pointerEvents="none" style={[styles.crosshair, { top: (height - peek) / 2 - 40 }]} accessibilityElementsHidden>
          <MaterialCommunityIcons name="map-marker" size={44} color={theme.colors.error} />
        </View>
      ) : null}

      {showChips ? (
        <View style={[styles.chips, { top }]} pointerEvents="box-none">
          <FilterRow />
          <StatusStrip />
        </View>
      ) : null}

      <MapControls sheetTop={sheetTop} visible={showControls} showLapor={view === 'home' && snap === 0} layersOpen={layersOpen} onToggleLayers={() => setLayersOpen((v) => !v)} />
      {layersOpen && showControls ? <LayersMenu sheetTop={sheetTop} onClose={() => setLayersOpen(false)} /> : null}

      <SheetHost snapPoints={snapPoints} sheetTop={sheetTop} query={query} />

      {/* Toasts sit just above the sheet and auto-hide after 4 s (spec §2). */}
      <Animated.View style={[styles.toast, toastStyle]} pointerEvents="box-none">
        <Snackbar
          key={s.toast?.id}
          visible={!!s.toast}
          onDismiss={s.hideToast}
          duration={4000}
          action={s.toast?.action ? { label: s.toast.action.label, onPress: s.toast.action.onPress } : undefined}
          wrapperStyle={{ position: 'relative' }}
        >
          {s.toast?.text ?? ''}
        </Snackbar>
      </Animated.View>

      <SearchBar ref={searchRef} query={query} onQuery={setQuery} onMenu={() => setMenuOpen(true)} />
      <MenuDrawer open={menuOpen} onClose={() => setMenuOpen(false)} />
    </View>
  );
}

const styles = StyleSheet.create({
  root: { flex: 1, backgroundColor: '#EEF0EB' },
  chips: { position: 'absolute', left: 0, right: 0, gap: 4 },
  crosshair: { position: 'absolute', left: 0, right: 0, alignItems: 'center' },
  toast: { position: 'absolute', left: 0, right: 0 },
});
