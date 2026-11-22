import React, { useCallback, useEffect, useRef } from 'react';
import { Pressable, StyleSheet, View } from 'react-native';
import BottomSheet, { BottomSheetBackdrop, BottomSheetScrollView, type BottomSheetBackdropProps } from '@gorhom/bottom-sheet';
import { ReduceMotion, type SharedValue } from 'react-native-reanimated';
import { useStore } from '../state/AppStore';
import type { Snap } from '../state/types';
import { theme } from '../theme/theme';
import { HomeSheet } from './HomeSheet';
import { SearchSheet } from './SearchSheet';
import { PlaceSheet } from './PlaceSheet';
import { RouteDetailSheet, RouteSheet } from './RouteSheets';
import { ReportSheet } from './ReportSheet';
import { DuplicateSheet, PickLocationSheet, QuickReportSheet, SentSheet } from './ReportFlow';
import { AboutSheet, CellSheet, DisasterSheet, FilterSheet, ListSheet, LocationIntroSheet, SignInSheet } from './MiscSheets';
import { AccountSheet, AlertsSheet, SubscriptionsSheet } from './AccountSheets';

/** The one bottom sheet (spec §1, §3). Changing context swaps its content; sheets never stack. */
export function SheetHost({ snapPoints, sheetTop, query }: { snapPoints: number[]; sheetTop: SharedValue<number>; query: string }) {
  const s = useStore();
  const ref = useRef<BottomSheet>(null);
  const cur = s.nav.current;

  useEffect(() => {
    ref.current?.snapToIndex(cur.snap);
  }, [cur]);

  useEffect(() => {
    s.clearActionError();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [cur.view]);

  const onChange = useCallback(
    (i: number) => {
      if (i >= 0 && i <= 2) s.dispatch({ type: 'snap', snap: i as Snap });
    },
    [s],
  );

  const backdrop = useCallback(
    (p: BottomSheetBackdropProps) => <BottomSheetBackdrop {...p} appearsOnIndex={1} disappearsOnIndex={0} opacity={0.32} pressBehavior="none" onPress={() => s.back()} accessibilityLabel="Tutup filter" />,
    [s],
  );

  const handle = useCallback(
    () => (
      <Pressable
        onPress={() => s.dispatch({ type: 'snap', snap: s.nav.current.snap === 0 ? 1 : s.nav.current.snap === 1 ? 2 : 1 })}
        accessibilityRole="adjustable"
        accessibilityLabel={s.nav.current.snap === 0 ? 'Perbesar panel' : 'Ubah ukuran panel'}
        accessibilityActions={[{ name: 'increment' }, { name: 'decrement' }]}
        onAccessibilityAction={(e) => {
          const n = s.nav.current.snap;
          const next = e.nativeEvent.actionName === 'increment' ? Math.min(2, n + 1) : Math.max(0, n - 1);
          s.dispatch({ type: 'snap', snap: next as Snap });
        }}
        style={styles.handle}
      >
        <View style={styles.bar} />
      </Pressable>
    ),
    [s],
  );

  return (
    <BottomSheet
      ref={ref}
      index={cur.snap}
      snapPoints={snapPoints}
      enableDynamicSizing={false}
      animatedPosition={sheetTop}
      onChange={onChange}
      enablePanDownToClose={false}
      handleComponent={handle}
      backdropComponent={cur.view === 'filter' ? backdrop : undefined}
      backgroundStyle={styles.bg}
      keyboardBehavior="interactive"
      android_keyboardInputMode="adjustResize"
      overrideReduceMotion={ReduceMotion.System}
      style={styles.shadow}
    >
      <BottomSheetScrollView contentContainerStyle={styles.content} keyboardShouldPersistTaps="handled" accessibilityLiveRegion="none">
        <Content query={query} />
      </BottomSheetScrollView>
    </BottomSheet>
  );
}

function Content({ query }: { query: string }) {
  const s = useStore();
  switch (s.nav.current.view) {
    case 'home':
      return <HomeSheet />;
    case 'search':
      return <SearchSheet query={query} />;
    case 'place':
      return <PlaceSheet />;
    case 'route':
      return <RouteSheet />;
    case 'route_detail':
      return <RouteDetailSheet />;
    case 'report':
      return <ReportSheet />;
    case 'quick':
      return <QuickReportSheet />;
    case 'pick_location':
      return <PickLocationSheet />;
    case 'dup':
      return <DuplicateSheet />;
    case 'sent':
      return <SentSheet />;
    case 'signin':
      return <SignInSheet />;
    case 'list':
      return <ListSheet />;
    case 'filter':
      return <FilterSheet />;
    case 'alerts':
      return <AlertsSheet />;
    case 'subscriptions':
      return <SubscriptionsSheet />;
    case 'account':
      return <AccountSheet />;
    case 'cell':
      return <CellSheet />;
    case 'disaster':
      return <DisasterSheet />;
    case 'location_intro':
      return <LocationIntroSheet />;
    case 'about':
      return <AboutSheet />;
  }
}

const styles = StyleSheet.create({
  bg: { backgroundColor: theme.colors.surfaceContainerLow, borderTopLeftRadius: 28, borderTopRightRadius: 28 },
  shadow: { shadowColor: '#000', shadowOpacity: 0.2, shadowRadius: 4, shadowOffset: { width: 0, height: -1 }, elevation: 4 },
  handle: { height: 36, alignItems: 'center', justifyContent: 'center' },
  bar: { width: 32, height: 4, borderRadius: 2, backgroundColor: 'rgba(67,71,78,0.4)' },
  content: { paddingHorizontal: 16, paddingBottom: 32, gap: 16 },
});
