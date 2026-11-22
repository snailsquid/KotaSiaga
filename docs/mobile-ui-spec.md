# KotaSiaga Mobile: UI/UX Spec

- Status: **DRAFT**. Covers layout, flows, interactions and user-visible states.
- Out of scope here: visual look (color, type, icons; comes later), and system/API design (handled separately).
- Last updated: 2026-11-22.

UI copy is the real Indonesian text the app shows. Everything else is English.

Prototype: the "KotaSiaga Mobile UI" design canvas (interactive). Its source is saved in `docs/design/prototype.dc.html`: one HTML file with every sheet state, the exact Material 3 colors, sizes and copy, and the interaction logic in the script at the bottom. Where this spec and the prototype disagree, the later sections of this spec (§12, §12b, §13, §14) win.

---

## 0. Who uses it and what they need

KotaSiaga only works if ordinary people report and confirm what they see. Every screen is judged by: *does this make it easier to contribute, or to trust what others contributed?*

| Person | Situation | What they need | Design answer |
|---|---|---|---|
| **Pengendara** (commuter, often on a motorbike) | About to leave, or stopped at a light, in the rain | "Is my way OK right now?" in one glance | `home` peek is one sentence + one word of risk ("Waspada · Hati-hati, ada genangan"); risk shown only where it matters (no green squares cluttering the map) |
| **Saksi** (witness) | Sees a flood or crash, wet hands, little time, slightly unsure | Report in seconds without typing | **Two-tap report**: "Lapor" → pick category → sent. Location is automatic. Details are optional, after sending. |
| **Warga sekitar** (people nearby) | Passing a reported spot | Be asked a simple question they can answer | A **"Bantu pastikan"** card asks a plain yes/no ("Masih ada genangan di Jl. Pasteur?") instead of "confirm/dispute" |
| **Pelapor rutin** (regular contributor) | Has reported before | Know it mattered | Feedback: "Laporanmu terverifikasi… dilihat 86 orang", weekly contribution summary, levels |

Fears and how the design handles them:

- **"What if I'm wrong?"** Reports start as *Menunggu* and are visibly unconfirmed (dashed pin). The witness doesn't have to be certain; neighbours confirm.
- **"Someone already reported it."** Before sending, the app checks for the same category within ~300 m and offers to confirm that one instead. Fewer duplicates, faster verification.
- **"I don't want my name on this."** Public reports show "Warga · <level>", never a name. Sign-in exists for trust, and the sign-in sheet says so.
- **"Do I have to sign in first?"** No. Browse freely. Sign-in appears only when you send or confirm, *after* you've chosen what to report, and the action continues by itself.
- **"Can I trust this pin?"** Filled pin = confirmed by people nearby; dashed = not yet. The legend is in the Lapisan panel. Only people within 500 m of a report can confirm it, and the app says so.

---

## 1. Concept

KotaSiaga is **one map screen**. Everything happens in panels on top of the map, following the Google Maps interaction model:

- a floating **search bar** at the top,
- a row of **filter chips** under it,
- **map buttons** on the right edge,
- one **bottom sheet** whose content changes with what the user is doing,
- a **"Lapor" button** to create a report.

No tab bar, no separate pages. The only full-screen things are owned by the system: the Google sign-in window and the camera/photo picker.

Principles:

1. **The map is always there.** Panels cover part of it, never all of it.
2. **One sheet.** Changing context swaps the sheet's content; sheets never stack.
3. **Select on the map, read in the sheet.** Tapping something on the map opens its details in the sheet and centers it in the visible map area above the sheet.
4. **Back always works.** Swipe-back, the Android back button, or the back arrow in the search bar undoes one step.
5. **Nothing blocks the map.** Loading happens inside the sheet (placeholder rows), never as a full-screen spinner.

---

## 2. Screen anatomy

```
┌──────────────────────────────────┐
│ ┌──────────────────────────────┐ │  Search bar
│ │ ≡  Cari tempat atau laporan (A)│ │  (A) = avatar / Masuk
│ └──────────────────────────────┘ │
│ [Daftar][Banjir][Kecelakaan][...]│  Filter chips (scrolls sideways)
│ Diperbarui 10:15:03  [MOCK]      │  Status strip
│                                  │
│            MAP                [◇]│  [◇] Lapisan (layers)
│                               [◎]│  [◎] Lokasi saya
│                                  │
│                          [+ Lapor]│  Create report
│ ┌──────────────────────────────┐ │
│ │ ───                          │ │  Bottom sheet (peek)
│ │ Coblong · Waspada · 27° Hujan│ │
│ └──────────────────────────────┘ │
└──────────────────────────────────┘
```

Stacking order, top first:

| Order | Element | Behavior |
|---|---|---|
| 1 | Toasts | Short confirmations ("Laporan terkirim"). Auto-hide after 4 s. Appear just above the sheet. |
| 2 | Search bar | Always visible. Left icon is **≡** (menu) on `home`, **←** (back) everywhere else. Right is the account avatar, or a "Masuk" icon when signed out. |
| 3 | Filter chips | Hidden when the sheet is full. |
| 4 | Status strip | Small text under the chips. See §9. |
| 5 | Map buttons | Right edge. Slide up with the sheet at half height; hide at full height. |
| 6 | "Lapor" button | Bottom right, above the sheet. Visible only when the sheet is at peek. |
| 7 | Bottom sheet | Three heights (§3). |
| 8 | Map | Pan, zoom, rotate. |

---

## 3. Bottom sheet

### 3.1 Heights

| Height | Size | Feels like |
|---|---|---|
| **Peek** | ~120 px, one summary line | "Glance": map is the focus |
| **Half** | ~45% of screen | "Look": details beside a visible map |
| **Full** | Up to just under the search bar | "Work": lists and forms |

- Drag the handle to move between heights; a fast flick skips to the next height.
- At half, scrolling the content first expands the sheet to full, then scrolls.
- At full, pulling down at the top of the content shrinks it back to half.
- Tapping the map while the sheet is half or full shrinks it to peek (it does not close the current item unless the tap lands on empty map, §4.3).

### 3.2 Sheet contents

| ID | Header | Opened by | Starts at |
|---|---|---|---|
| `home` | Area name + risk level | App start; closing anything | Peek |
| `cell` | "Area ini" | Tapping a risk area on the map | Half |
| `report` | Report title | Tapping a report pin, list row or notification | Half |
| `disaster` | Disaster name | Tapping a disaster area | Half |
| `place` | Place name or coordinates | Search result; long-press on map; tapping a tourism or blood-donation point | Half |
| `report_list` | "Laporan di sekitar" | "Daftar" chip; swiping `home` up to full | Full |
| `route_plan` | "Rute aman" | "Rute" button on any item; menu | Half |
| `route_detail` | Route name | Tapping a route card or line | Half |
| `report_new` | "Buat laporan" | "Lapor" button; "Laporkan di sini" | Full |
| `alerts` | "Notifikasi" | Menu; avatar badge | Full |
| `subscriptions` | "Area pantauan" | From `alerts`; "Pantau area ini" | Full |
| `account` | "Akun" | Tapping the avatar | Half |
| `sign_in` | "Masuk untuk melanjutkan" | Any action that needs an account (§8) | Half |

### 3.3 Back

The sheet remembers the last 5 contents. Back returns to the previous one at the height it had. From `home` at peek, back leaves the app.

Example: `home` → tap pin → `report` → "Rute" → `route_plan` → back → `report` → back → `home`.

### 3.4 Menu (≡)

A side drawer from the left with: Notifikasi, Area pantauan, Lapisan peta, Tentang aplikasi. It overlays the map and closes on tap outside.

---

## 4. Map

### 4.1 What's drawn

| Layer | How it looks (shape only, look TBD) | Default |
|---|---|---|
| Risk areas | Grid squares filled by level; label "Aman / Waspada / Bahaya" inside when zoomed in | On |
| Reports | Pin per report with a category icon. **Menunggu** = outlined pin, **Terverifikasi** = filled pin. Expired reports are not shown. | On |
| Disaster signals | Shaded area or pin | On |
| Tourism destinations | Small point with icon | Off |
| Blood-donation sites | Small point with icon | Off |
| Routes | Lines; the selected route is drawn on top and thicker | Only while planning a route |
| Me | Blue-dot style marker with accuracy ring and direction | On when location is allowed |

Risk is never shown by color alone: every risk badge and area label also has the word.

### 4.2 Zoom behavior

- **Zoomed out (city):** risk areas hidden; report pins grouped into count bubbles.
- **Middle (district):** risk areas without labels; nearby pins grouped.
- **Zoomed in (street):** risk areas with labels; individual pins.
- New data for the visible area appears after the map stops moving, not while dragging.

### 4.3 Gestures

| Gesture | Result |
|---|---|
| Tap empty map | Clear selection, back to `home` at peek |
| Tap pin, area or risk square | Open `report`, `disaster` or `cell`. A pin wins over the square under it. |
| Tap a count bubble | Zoom in until it splits |
| Long-press | Drop a pin, open `place` with its coordinates |
| Tap ◎ once | Center on me |
| Tap ◎ again | Follow me with map rotating to my direction; any drag stops following |

Selected item: its pin enlarges and gets a ring; the camera moves so it sits in the middle of the map area left above the sheet.

### 4.4 Layers panel ("Lapisan")

Tapping ◇ opens a small card over the map (not the main sheet) with a toggle per layer from §4.1. Closes on tap outside. Choices are remembered.

---

## 5. Search and filter chips

### 5.1 Search

Tapping the search bar:

1. Sheet goes to full, chips hide, keyboard opens.
2. **Before typing:** "Lokasi saya", recent searches (max 10, each with ✕ to remove), and saved "Area pantauan".
3. **While typing:** results in groups: **Laporan**, **Destinasi wisata**, **Lokasi donor darah**, **Tempat**.
4. Picking a result closes search, moves the map, and opens that item's sheet.
5. ← or back closes search and returns to the previous sheet.

No results: "Tidak ditemukan. Coba kata lain atau tekan lama di peta untuk memilih titik."

### 5.2 Filter chips

```
[≣ Daftar] [Banjir] [Kecelakaan] [Jalan rusak] [Jalan ditutup] [Keramaian] [Status: Semua ▾]
```

- **Daftar** opens `report_list`.
- Category chips toggle on/off (multi-select). None on = all categories.
- **Status** opens a small menu: Semua / Menunggu / Terverifikasi.
- Filters apply to the map and `report_list` together. When any filter is on, a **"Hapus filter"** chip appears at the start of the row.

---

## 6. First launch and app start

**First launch only:**

1. Map opens on Bandung.
2. A sheet at half explains location before the system prompt: "KotaSiaga memakai lokasimu untuk menunjukkan risiko di sekitarmu." Buttons: **Izinkan lokasi**, **Nanti saja**.
3. No sign-in, no tutorial. The app is usable right away.

**Every start:**

- Map returns to where the user left it (or their location if allowed).
- Sheet shows `home` at peek.

---

## 7. Sheet contents in detail

### 7.1 `home`

Peek:

```
───
Coblong · [Waspada] · 27° Hujan lebat          ⌃
```

Half:

```
───
Coblong                                  [Waspada]
Risiko sedang karena hujan lebat dan 2 laporan
terverifikasi di dekatmu.

27°  Hujan lebat · 18 mm/jam
11:00  12:00  13:00  14:00  15:00  16:00
 22     15     8      3      0      0    mm/jam

2 laporan aktif · 0 sinyal bencana

[ Rute aman ]  [ Pantau area ini ]  [ Lihat laporan ]
```

- The one-sentence explanation turns the risk factors into plain words.
- Location off: header says "Area di tengah peta" and follows the map center.
- Risk unavailable: the risk block becomes "Data risiko belum tersedia" and the rest still shows.

### 7.2 `cell`

- Header "Area ini" + risk badge.
- Same plain-words explanation as `home`, and when it last changed ("Berubah 4 menit lalu").
- Up to 5 reports in the area, then "Lihat semua".
- Buttons: **Rute ke sini**, **Pantau area ini**.

### 7.3 `report`

```
───
[Banjir]  [Menunggu]                         ✕
Genangan setinggi 30 cm
Jl. Pasteur · 450 m · 12 menit lalu
Keparahan: Sedang · Berakhir dalam 2 j 14 m

[ foto ]

Butuh konfirmasi warga lain
▓▓▓▓▓▓▓░░░░░  4 konfirmasi · 1 bantahan

[ ✓ Konfirmasi ]  [ ✕ Bantah ]
[ Rute ]  [ Bagikan ]
```

- **Menunggu:** shows the progress bar and vote buttons.
- **Terverifikasi:** progress bar replaced by "Terverifikasi warga · 9 menit lalu"; voting stays open.
- **Kedaluwarsa** (only reachable from a notification): grayed, "Laporan ini sudah kedaluwarsa", no vote buttons.
- After voting: the chosen button shows as selected with "Kamu mengonfirmasi" / "Kamu membantah"; both buttons disabled.
- Own report: vote buttons replaced by "Laporan kamu".
- Signed out: tapping a vote button opens `sign_in`; after signing in, the vote is sent automatically.
- **Bagikan** opens the phone's share menu.

### 7.4 `disaster`

- Type, source (e.g. "PetaBencana"), when it started, active/selesai.
- The affected area is highlighted on the map.
- Button: **Rute menghindari area ini**.

### 7.5 `place`

- Name (or coordinates for a dropped pin), distance, risk badge of the area.
- Tourism destination: its category.
- Blood-donation site: opening hours and free slots if known.
- Buttons: **Rute**, **Pantau area ini**, **Laporkan di sini**.

### 7.6 `report_new`: quick report (chosen)

1. **Lapor** opens "Apa yang kamu lihat?": the location line ("Di lokasimu sekarang · Jl. Pasteur" + **Ubah**) and 5 large tiles (icon, name, short hint like "Genangan, air naik").
2. **One tap on a tile sends the report.** If signed out, the sign-in sheet appears first and the report sends right after.
3. **Duplicate check:** if the same category was reported within ~300 m, the sheet shows "Sudah ada yang melaporkan" with that report and two buttons: **Ya, sama. Pastikan laporan ini** (counts as a confirmation) or **Bukan, kirim laporan baru**.
4. **Sent:** "Laporan banjir terkirim · Warga di sekitar akan diminta memastikan." Below it, optional **Tambah detail**: severity (Ringan/Sedang/Berat) and **Tambah foto**, with the reason "bikin lebih cepat dipercaya". **Selesai** returns to the map, where the user's own report shows as a dashed blue pin.

### 7.6b `report_new`: full form (alternative, see §13)

The sheet holds the form; the map above shows a draggable pin for the location.

```
───
Buat laporan                                 ✕

Apa yang terjadi?
[ Banjir ] [ Kecelakaan ] [ Jalan rusak ]
[ Jalan ditutup ] [ Keramaian ]

Lokasi
◉ Jl. Dago No. 12 (lokasimu)    Geser peta untuk mengubah

Seberapa parah?
( Ringan ) ( Sedang ) ( Berat )

Keterangan (opsional)
[                                          ] 0/280

[ + Tambah foto ]

[        Kirim laporan        ]
Laporan akan menunggu konfirmasi warga lain.
```

- Steps go top to bottom on one sheet; no wizard.
- **Kirim laporan** is disabled until category, location and severity are set.
- Moving the map moves the location (pin stays centered, like picking a ride pickup point). The sheet shrinks to half while the map is being dragged so the user can see.
- On send: toast "Laporan terkirim, menunggu konfirmasi warga", new outlined pin appears, sheet switches to that `report`.
- Close or back with anything filled in asks "Buang laporan ini?" (**Buang** / **Lanjutkan mengisi**).
- Send fails: form stays filled, message above the button, **Coba lagi**.

### 7.7 `report_list`

- Rows: category icon, title, place · distance, time, status badge.
- Sorted nearest first. Uses the active filter chips.
- Scrolling to the end loads more.
- Tap a row: map flies to it, sheet opens `report`.
- Empty: "Belum ada laporan di area ini." With filters on: "Tidak ada laporan yang cocok dengan filter." + **Hapus filter**.

### 7.8 `route_plan` and `route_detail`

`route_plan`:

```
───
◉  Lokasi saya                              ⇅
◎  PMI Kota Bandung

┌ Disarankan ─────────────────────────────┐
│ 13 mnt · 5,4 km · [Aman]                │
│ Menghindari genangan di Jl. Pasteur     │
└─────────────────────────────────────────┘
┌─────────────────────────────────────────┐
│ 11 mnt · 4,8 km · [Waspada]             │
│ Melewati 1 area Waspada                 │
└─────────────────────────────────────────┘
```

- "Ke" is prefilled from where the user came from. Tapping a field opens search to change it; ⇅ swaps them.
- Up to 3 route cards; the recommended one first with **Disarankan**.
- All routes are drawn on the map; tapping a card or a line selects it. The map zooms to fit the whole route above the sheet.

`route_detail`:

- Why this route ("Menghindari genangan di Jl. Pasteur"), the areas it passes with their risk.
- **Buka di aplikasi navigasi**: hands off to the phone's maps app. KotaSiaga doesn't do turn-by-turn.
- If risk changes on the selected route while it's open: banner "Risiko di rute ini berubah" + **Perbarui rute**.

No route possible: "Rute aman belum tersedia" + **Buka di aplikasi navigasi** (without risk info).

### 7.9 `alerts` (needs account)

- Newest first. Each row: icon by type, short text ("Banjir terverifikasi di Rumah"), area name, time. Unread rows marked.
- Tap: opens the related `report`, `cell` or `disaster`, and marks it read.
- Top link: **Area pantauan**.
- Unread count shows as a badge on the avatar in the search bar.
- Empty: "Belum ada notifikasi. Pantau area untuk menerima peringatan." + **Pantau area**.

### 7.10 `subscriptions` (needs account)

- Rows: name ("Rumah"), radius, current risk badge.
- **Tambah area:** the map shows a circle at the center of the screen; user pans to place it, picks radius **500 m / 1 km / 2 km** (circle resizes live), types a name, taps **Simpan**.
- Starting from "Pantau area ini", the circle is already placed there.
- Delete: swipe left or ⋮ → Hapus, with toast "Area dihapus" + **Urungkan**.

### 7.11 `account`

- Signed in: Google name and photo, email, reputation level, number of own reports, **Keluar**.
- Links: Area pantauan, Notifikasi, Tentang aplikasi.
- Signed out: replaced by the `sign_in` content.

---

## 8. Sign-in (Google)

### 8.1 When it's needed

| Without an account | With an account, also |
|---|---|
| Map, risk, weather, reports, disasters, routes, search | Create reports, vote, watched areas, notifications |

### 8.2 Flow

```
user taps "Konfirmasi" (signed out)
  → sheet: sign_in
        "Masuk untuk mengonfirmasi laporan"
        [ G  Masuk dengan Google ]
        Nanti saja
  → Google window (system)
  → back in app, same map position
  → vote is sent, toast "Konfirmasi terkirim"
```

- The `sign_in` sheet's sentence names the action that triggered it ("…untuk mengirim laporan", "…untuk memantau area").
- After sign-in, the user lands exactly where they were and the action continues by itself.
- **Nanti saja** or cancelling the Google window returns to the previous sheet with nothing lost.
- Failure: toast "Gagal masuk. Coba lagi."
- Session ended (signed out by the system): only shows `sign_in` when the user next tries a gated action; no interrupting popup.
- **Keluar:** confirm "Keluar dari akun?", then the avatar becomes "Masuk" and account-only sheets close.

---

## 9. Live updates and status

The map changes by itself while the user watches. Updates never move the camera or close what the user has open.

| What happens | What the user sees |
|---|---|
| New report | Outlined pin appears with a small pop |
| Report verified | Pin fills in; an open `report` sheet updates its badge and progress |
| Report expired | Pin fades out; an open `report` sheet shows "Laporan ini sudah kedaluwarsa" |
| Risk changes | Area square changes; if it's the user's own area or a watched area, toast "Risiko di sekitarmu berubah: Waspada" |
| Weather changes | `home` line updates |
| Disaster starts / ends | Area appears / fades out |

**Status strip** (under the chips):

- "Diperbarui 10:15:03": time of the last update.
- **MOCK** chip when the app is showing generated demo data.
- Stale: "Sebagian data mungkin tidak terbaru".
- Offline: "Tidak ada koneksi. Menampilkan data terakhir."

Stale and offline replace the "Diperbarui" text (one line at a time, offline wins).

---

## 10. Empty, error and permission states

| Situation | What the user sees |
|---|---|
| Location not allowed | Map on Bandung; `home` follows the map center; ◎ opens "Izinkan lokasi di Pengaturan" + **Buka Pengaturan** |
| Risk data unavailable | Risk squares hidden; `home` says "Data risiko belum tersedia" |
| Routes unavailable | `route_plan` says "Rute aman belum tersedia", offers **Buka di aplikasi navigasi** |
| Tourism or blood-donation data unavailable | That layer's toggle shows "Data tidak tersedia" and is disabled |
| Too many requests | Current data stays; no message unless the user pressed a button, then "Coba lagi sebentar lagi" |
| Action failed (vote, send, save) | Inline message near the button + **Coba lagi**; user input is kept |

---

## 11. Accessibility

- Risk always has a word, never color alone.
- Tap targets at least 44×44 pt.
- Screen-reader labels on map items: "Laporan banjir, terverifikasi, 300 meter".
- Everything on the map is also reachable without map gestures (search and `report_list`).
- Follows the system text size; the sheet scrolls instead of cutting text.
- Moving things (pin pop, fades) respect the system's reduce-motion setting.

---

## 12. Decisions (from the user-needs pass)

These override anything above that says otherwise.

1. **Voting is a plain question, not "Konfirmasi/Bantah".** Each category has its own question and answers, e.g. Banjir: "Masih ada genangan di Jl. Pasteur?" → **Masih ada** / **Sudah surut**; Jalan ditutup: **Masih ditutup** / **Sudah dibuka**. "Sudah surut" on a verified report helps it end early.
2. **"Bantu pastikan" card.** When the user is within 500 m of an unconfirmed report they haven't answered, `home` shows the question card above the area summary. **Tidak tahu** hides it for that report.
3. **Only nearby people can confirm** (within 500 m). Farther away, the report says "Hanya warga dalam 500 m dari lokasi yang bisa ikut memastikan."
4. **Only Waspada and Bahaya areas are drawn.** Aman is the normal state and adds no color to the map.
5. **Pins are neutral (not risk-colored).** Filled = confirmed, dashed = waiting, dashed blue = mine. Risk color lives only on areas and badges, so the two never get confused.
6. **Location is by address when known**, falling back to "Titik di peta".
7. **Photo is optional**, offered after sending with the reason it helps.
8. **Reports are anonymous in public** ("Warga · Pelapor Aktif").
9. **Reputation is a level, not a number:** Warga Baru → Pelapor Aktif → Warga Terpercaya. It's shown on the user's own reports and in their contribution summary.
10. **Contribution feedback:** notifications for "Laporanmu terverifikasi" (with how many people saw it) and a weekly summary card at the top of `alerts`.
11. **Push notifications: yes, but few.** Only for: your report verified, *Bahaya* in a watched area, and being asked to confirm something near a watched area. Everything else stays in the in-app list.
12. **Tourism and blood-donation layers default off.** They're for people looking for those places, not for everyday use.
13. **The avatar is the door to notifications** (with an unread badge); there's no separate bell.

## 12b. Revisions after review (2026-11-22, second round)

These override earlier sections.

1. **No grid squares on the map.** The 500 m cells are an internal calculation. On the map, risk shows the way people see it on the ground:
   - a **soft-edged zone** (blurred, no hard border) over the affected area, with a small label ("Waspada · Genangan");
   - the **affected stretch of road** drawn in the risk color over the road, like a traffic layer.
2. **No "Menunggu".** Unconfirmed reports say what they're waiting for: **"Perlu dipastikan (1/3)"** in lists, and **"Dipastikan 1 dari 3 warga"** with a progress bar and one explaining sentence in the detail. Confirmed reports say **"Dipastikan 5 warga"**. The API enum `waiting`/`verified` is unchanged; this is only the on-screen wording.
3. **Routes start from search, like Google Maps.** The "Rute aman" button is removed from the home sheet and the report sheet. Flow: tap the search bar → recent places and matching reports → pick a place → the **place sheet** (name, address, **Rute** / Pantau / Bagikan, and "Kondisi menuju ke sana" with a risk label and one sentence) → **Rute** opens the route sheet with that place as the destination. Every recommended route is "safe" by default; there is no separate "safe route" mode to pick.
4. **Color:** see §13.

## 13. Design system

**Material Design 3, through React Native Paper.** No custom visual system. The color scheme is generated by Material Theme Builder from a blue seed (primary `#0061A4`) in place of the baseline purple. Purple felt off for a safety and maps app, and blue keeps the risk colors (amber, red) as the only warm colors on screen.

- Components: MD3 search bar, filter chips, FABs (map controls and the extended "Lapor" FAB), bottom sheet, list items, cards, segmented buttons, checkboxes and radios, snackbar, menu.
- Icons: Material Symbols (Outlined). Categories: Banjir `flood`, Kecelakaan `car_crash`, Jalan rusak `construction`, Jalan ditutup `block`, Keramaian `groups`.
- Type: the MD3 type scale in Roboto.
- The only additions are risk colors, as MD3 custom colors: Waspada uses an amber container, Bahaya uses the MD3 error container, Aman uses a green container. They always appear with their word.
- **Filter:** MD3 *filter chips* (a check mark appears when selected) on the map, led by a **Filter** chip (`tune` icon + count badge). The Filter chip opens a filter bottom sheet with checkboxes for incident type, radio buttons for status, **Atur ulang**, and **Tampilkan N laporan**. While filters are on, the map shows "Menampilkan N dari M laporan · Hapus".

## 14. Options

Confirmed by the user on 2026-11-22: **A (two-tap report), B (question card in the sheet), E (only Waspada/Bahaya drawn).** C and D stay as chosen below until someone says otherwise.

| # | Decision | Chosen in prototype | Alternative | Trade-off |
|---|---|---|---|---|
| A | How a report is made | **Two-tap**: tile → sent; details after | **Full form**: category, severity, note, photo, then send | Two-tap gets far more reports from people in a hurry; the form gets richer first reports but more people give up halfway |
| B | Where "Bantu pastikan" appears | **Card inside the bottom sheet** | **Banner at the top** under the search bar | The card is calm and doesn't cover the map; the banner is noticed more but covers part of the map and can feel pushy |
| C | When sign-in is asked | **At the moment of sending/confirming** (after choosing) | At first launch | Asking late means people try the app first; asking early means fewer interrupted flows but many people close the app |
| D | Who can confirm | **Within 500 m only** | Anyone | Nearby-only makes confirmations trustworthy; anyone means faster verification but invites guesses |
| E | Default risk on the map | **Only Waspada/Bahaya areas** | All areas including Aman | Showing only problems keeps the map readable; all areas reassures more but adds green everywhere |
