import React from 'react';
import { ScrollView, StyleSheet, View } from 'react-native';
import { Badge, Chip, Text, TouchableRipple } from 'react-native-paper';
import { CATEGORIES, CATEGORY_ORDER } from '../domain/categories';
import { clock } from '../domain/format';
import { filterCount, isActive, matchesFilter, useStore } from '../state/AppStore';
import { elevation1, riskColors, theme } from '../theme/theme';

const c = theme.colors;

/** MD3 filter chips on the map, led by the "Filter" chip with a count badge (spec §13). */
export function FilterRow() {
  const s = useStore();
  const n = filterCount(s.filters);
  return (
    <ScrollView
      horizontal
      showsHorizontalScrollIndicator={false}
      contentContainerStyle={styles.row}
      accessibilityRole="toolbar"
      accessibilityLabel="Filter laporan"
      keyboardShouldPersistTaps="handled"
    >
      <View>
        <Chip
          icon="tune"
          mode="flat"
          elevated
          selected={n > 0}
          showSelectedCheck={false}
          onPress={() => s.go({ view: 'filter' })}
          accessibilityLabel={n > 0 ? `Filter, ${n} aktif` : 'Filter'}
          style={[styles.chip, { backgroundColor: n > 0 ? c.secondaryContainer : c.surfaceContainerLow }]}
          textStyle={styles.chipText}
        >
          Filter
        </Chip>
        {n > 0 ? (
          <Badge size={18} style={styles.count} accessibilityElementsHidden importantForAccessibility="no">
            {n}
          </Badge>
        ) : null}
      </View>
      {CATEGORY_ORDER.map((k) => {
        const on = s.filters.cats.includes(k);
        return (
          <Chip
            key={k}
            mode="flat"
            elevated
            selected={on}
            showSelectedCheck
            onPress={() => s.setFilters({ ...s.filters, cats: on ? s.filters.cats.filter((x) => x !== k) : [...s.filters.cats, k] })}
            accessibilityState={{ selected: on }}
            style={[styles.chip, { backgroundColor: on ? c.secondaryContainer : c.surfaceContainerLow }]}
            textStyle={styles.chipText}
          >
            {CATEGORIES[k].label}
          </Chip>
        );
      })}
    </ScrollView>
  );
}

/**
 * Status strip under the chips (spec §9): "Diperbarui HH:MM:SS" + MOCK chip; stale and offline
 * replace the time (offline wins). While a filter is on, "Menampilkan N dari M laporan · Hapus".
 */
export function StatusStrip() {
  const s = useStore();
  const m = s.data.meta;
  const active = s.data.reports.filter((r) => isActive(r));
  const shown = active.filter((r) => matchesFilter(r, s.filters));
  const filtered = filterCount(s.filters) > 0;

  let text: string | null = m ? `Diperbarui ${clock(m.updated_at)}` : null;
  let bg = c.surfaceContainer;
  let fg = c.onSurface;
  let a11yAlert = false;
  if (!s.online) {
    text = `Tidak ada koneksi. Menampilkan data terakhir${m ? ' ' + clock(m.updated_at).slice(0, 5) : ''}.`;
    bg = c.inverseSurface;
    fg = c.inverseOnSurface;
    a11yAlert = true;
  } else if (m?.stale) {
    text = 'Sebagian data mungkin tidak terbaru';
    bg = riskColors.caution.container;
    fg = riskColors.caution.onContainer;
    a11yAlert = true;
  }

  return (
    <View style={styles.strip}>
      {filtered ? (
        <View style={[styles.pill, { backgroundColor: c.surfaceContainer, flexDirection: 'row', alignItems: 'center', paddingRight: 0 }]}>
          <Text variant="labelMedium">
            Menampilkan {shown.length} dari {active.length} laporan
          </Text>
          <TouchableRipple onPress={() => s.setFilters({ cats: [], status: 'all' })} accessibilityRole="button" accessibilityLabel="Hapus filter" style={styles.clear}>
            <Text variant="labelMedium" style={{ color: c.primary }}>
              Hapus
            </Text>
          </TouchableRipple>
        </View>
      ) : null}
      {text && (!filtered || a11yAlert) ? (
        <View style={[styles.pill, { backgroundColor: bg }]} accessibilityLiveRegion={a11yAlert ? 'polite' : 'none'} accessibilityRole={a11yAlert ? 'alert' : 'text'}>
          <Text variant="labelMedium" style={{ color: fg }}>
            {text}
          </Text>
        </View>
      ) : null}
      {m?.source === 'mock' && !filtered ? (
        <View style={[styles.mock]} accessibilityLabel="Data demo, bukan data asli">
          <Text variant="labelSmall" style={{ color: c.inverseOnSurface }}>
            MOCK
          </Text>
        </View>
      ) : null}
    </View>
  );
}

const styles = StyleSheet.create({
  row: { paddingHorizontal: 16, gap: 8, paddingVertical: 4, alignItems: 'center' },
  chip: { height: 32, borderRadius: 8 },
  chipText: { marginVertical: 0, lineHeight: 20 },
  count: { position: 'absolute', right: -4, top: -6, backgroundColor: c.primary, color: c.onPrimary },
  strip: { flexDirection: 'row', alignItems: 'center', gap: 6, paddingHorizontal: 16, flexWrap: 'wrap' },
  pill: { paddingVertical: 4, paddingHorizontal: 10, borderRadius: 8, ...elevation1 },
  clear: { minHeight: 32, minWidth: 48, paddingHorizontal: 8, justifyContent: 'center', alignItems: 'center', borderRadius: 16 },
  mock: { paddingVertical: 2, paddingHorizontal: 8, borderRadius: 8, backgroundColor: c.inverseSurface },
});
