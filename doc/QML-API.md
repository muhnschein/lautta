# QML ⇄ Rust contract

How the Silica UI (`qml/`) talks to the core. The Qt layer (`crates/lautta-qt`) exposes
QObjects through `qmetaobject`; it holds no business logic (SPEC ARC-3). Every screen is a
board of the design canvas (`design/canvas/project/<Board>.dc.html`); the board names below
are the reference for each page.

## Rules

- `import Lautta 1.0` provides the types below. Qt 5.6 / QtQuick 2.6 / Silica only;
  allowed imports: SPEC HBR-3. No custom chrome (UI-1); theme sizes and colours (UI-2);
  portrait and landscape (UI-3); `allowedOrientations: Orientation.All` on every page.
- Items are identified by **URI strings** (`lautta://<location>/<percent-encoded path>`,
  SPEC LOC-7). Names are for display only (lossy for non-UTF-8 names; `nameIsLossy` role
  shows a badge, BRW-1). Never build URIs by string concatenation in QML; use
  `App.childUri(parentUri, name)`.
- All user-visible strings use `qsTrId("lautta-…")` with a `//%` engineering English
  comment (UI-7), plurals with `%n`. Ids are kebab-case and start with `lautta-`.
- Sizes and dates are formatted in QML: `Format.formatFileSize(bytes)`,
  `Format.formatDate(date, Formatter.…)` (Silica). Rust returns raw numbers and
  milliseconds since the epoch (`-1` = unknown).
- Structured values cross the boundary as **JSON strings**: properties and method
  results named `…Json` hold `JSON.stringify`-compatible text that QML parses with
  `JSON.parse`; structured arguments are passed as `JSON.stringify(value)`. Lists of URIs
  are JSON arrays of strings. List models use plain roles (string, number, bool).
- Errors from Rust reach QML as `{ kind: "<ErrorKind name>", message: "<detail>" }`
  objects or as `errorKind`/`errorMessage` properties; QML turns the kind into translated
  text with `components/ErrorText.js` (`ErrorText.message(kind, context)`, SPEC §20).
- Long work is async: methods return immediately; results arrive through signals or
  property changes. Nothing blocks navigation (UI-4).
- Destructive actions use `RemorseItem`/`RemorsePopup` before calling Rust (UI-5); the
  duration is `App.setting("remorse_seconds")`.
- Do not use `PushUpMenu` anywhere (design decision: pulley menus only).

## Singletons (`Lautta 1.0`)

| Name | Owner | Purpose |
|---|---|---|
| `App` | core infra | readiness, settings, clipboard, undo, URI helpers, open/share |
| `Bridge` | browse | netvfs bridge status, consent, servers, nearby, questions, hand-offs |
| `Operations` | operations | plans, conflicts, delete, compress/extract |
| `Transfers` | transfers | queue summary and control, cover data, keep-alive state |

### `App`

Properties: `ready: bool`, `startError: string`, `version: string`,
`settingsJson: string` (object, keys of `lautta_core::settings::Settings::to_map`),
`canUndo: bool` (OPS-9 banner), `undoText: string` (engineering kind: `rename`, `move`,
`trash`), `clipboardCount: int`, `clipboardCut: bool`.

Methods:
- `setting(key) → var` (bool, number or string; lists as JSON text), `setSetting(key,
  valueJson)` (value as JSON text, e.g. `JSON.stringify(true)`); `settingsJsonChanged()`
  signal. QML persists the settings JSON in dconf (`Nemo.Configuration`,
  `/apps/harbour-lautta/settings`) and calls `App.loadSettings(json)` at start (SPEC §18).
- `childUri(parentUri, name) → string`, `parentUri(uri) → string`, `nameOf(uri) → string`,
  `displayAddress(uri) → string` (file:// path or netvfs URL without secrets, LOC-7),
  `locationName(uri) → string`, `isLocal(uri) → bool`, `isRemote(uri) → bool` (on a
  server, LOC-7, also one that is gone; archives are neither: lines naming the location
  mark it, §15.5).
- `copy(uris)`, `cut(uris)`, `clearClipboard()`, `canPasteInto(uri) → bool`,
  `paste(destUri)` (→ `Operations` flow).
- `undo()`; signal `undone(ok)`.
- `categoryOf(nameOrUri) → string` (FileCategory icon name), `viewerFor(uri, mime) → string`
  (`image`, `text`, `markdown`, `audio`, `video`, `archive`, `pdf`, `external`).
- `previewRows(entries, sortKey, descending, foldersFirst, showHidden) → list` — the
  sample folder of the view options preview (BRW-4): `entries` is JSON
  `[{ name, dir, size, modified }]` (bytes, ms since the epoch), ordered and filtered by
  the core as a real folder with these view settings; one map per shown entry with
  `name`, `isDir`, `size` (bytes, -1 for a folder), `modified` (ms, -1 if unknown),
  `hidden`, `category`. Bad JSON gives an empty list.
- `open(uri)`: decides by viewer and pushes the viewer page (through
  `components/Open.js`); `external` → `openExternally(uri)`.
- `openExternally(uri)`: local → system handler; remote → copy to
  `~/Downloads/Lautta/Opened/` first (PRV-6) and then open; signal
  `externalOpenStarted(uri, transferId)`.

## Instantiable types by area

Each area owns its types and pages. Names are fixed here so areas can use each other.

### Browse (boards: Main, BrowsePulley, BrowseStandalone, BrowseServerMenu, BrowseVolume, ConnectServer, ServerQuestion, ServerAdhocMenu, FavouriteEdit, Recents, RecentsPulley, StateReconnecting, StateIdentity, Covers)

- `LocationsModel` — sections `favourites`, `device`, `android`, `volumes`, `servers`,
  `nearby` (roles: `section`, `uri`, `name`, `kind`, `icon`, `status`, `attention`,
  `colour`, `itemId`, `place`, `provider`, `host`, `free`, `total`, `fs`,
  `connection`, `connectionError`, `isFile` (a file favourite: opened, not browsed),
  `drive` (the bridge's id of a volume's drive, empty when the bridge does not mount it,
  LOC-3)); a `volume` with `status` `unmounted` is a drive that is not mounted: no `uri`,
  `total` its size, `free` -1;
  kinds `recents` and `deleted` are the app's own rows of `device` (`BrowseText.rowName`
  names them). `connection` is a server's connection state (NVB-4): `connected`,
  `connecting` (also while the bridge reconnects, NVB-12), `disconnected` (also recent
  ad-hoc servers), `failed`, or empty when the bridge reports none; `connectionError` is
  the netvfs error name of a `failed` one (`AuthFailed`, `Timeout`, …).
  `components/browse/RowStatus.js` `indicator(kind, status, connection, connectionError,
  drive)` turns them into the row's status icon (`on`, `busy`, `off`, `failed`,
  `warning`, `""` for none; servers and volumes, LOC-3), which
  `components/browse/RowStatusIcon.qml { indicator, highlighted }` draws at the right
  margin, no text (§15.2). `refresh()`
  rescans the device folders and volumes first (LOC-3); the model also follows location
  changes by itself, and a changed row changes in place.
- `FavouritesModel` (roles `itemId`, `uri`, `label`, `colour`, `place`, `isDir`, `icon`
  (the chosen theme icon name, empty for the default), `defaultIcon` (the icon source
  shown without one: the star, or a file's icon); `add` (a folder),
  `save(uri, label, colour, icon, isDir)` adds a folder or file or changes label, colour
  and icon of the favourite it is (an empty colour or icon is none, an unknown icon is
  none), `edit`, `remove`, `removeUri(uri)`, `moveTo`, `get`, `idOf`, `placeOf`,
  `iconChoices()` (the icon names, the star first); `components/browse/IconPicker.qml
  { icon, icons, defaultIcon, colour }`, signal `picked(icon)`).
- `RecentsModel` (ORG-2, newest first, no filter; `count`, `enabled`, `loaded`,
  `reload()`, `clear()`, `remove(itemId)`, signal `failed(kind, message)`; roles:
  `itemId`, `uri`, `name`, `kind`, `place`, `at`, `dateGroup`). `dateGroup` is the date
  section (§15.4): `today`, `yesterday`, `last7Days`, `last30Days`, `older`;
  `components/DateSectionHeader.qml { group }` shows it.
- `Bridge` singleton: `status` (`absent`, `connecting`, `consentUnknown`, `consentDenied`,
  `ready`, `reconnecting`), `requestConsent()`, `addServer(provider)`,
  `editAccount(locationUri)`, `connectAdHoc(url, secret, options)`, `forgetAdHoc(id)`,
  signal `question(id, kind, details)`, `answer(id, answer)`; `storageSettings` (the
  bridge can open Settings › Storage, NVB-13; changes with `statusChanged`) and
  `openStorageSettings()` (a failure arrives as `failed`). `BrowsePage`'s
  `storageSettings` follows it; tests set it. `mountDrive(drive, name)` mounts a drive
  (LOC-3) and opens it: `adhocConnected(locationUri)` carries the volume as it carries a
  connected ad-hoc server; `unmountDrive(drive, name)`; their failures arrive as `failed`
  with `name` as the subject.
- Pages: `pages/BrowsePage.qml` (initial page), `pages/RecentsPage.qml`,
  `dialogs/ConnectServerDialog.qml { url }`,
  `dialogs/ServerQuestionDialog.qml { questionId, kind, details }`,
  `pages/FavouritePage.qml { uri, isDir }` (saves each change; makes the item a
  favourite when it is none; `isDir` defaults to true, a file passes false),
  `cover/CoverPage.qml`.
- Browse page settings (BRW-12): `BrowseLayoutModel` (roles `section`, `kind`, `item`,
  `name`, `hidden`, `place` (the place in the section; no role is called `index`, which
  would hide the delegate's `index`; -1 for the switch of Volumes or Servers), `movable`,
  `nested` (a row of a section with such a switch); the switch is a row of kind `section`
  and item `section` that leads its section, `hidden` telling that the section is off
  (set with `setHidden(section, "section", hidden)`), and a section that is off lists
  no other row; Volumes switch always there, Servers switch whenever a bridge answers;
  `reload()` reads the rows and starts
  following what `LocationsModel` follows (a row that comes or goes arrives as an
  insertion or removal), except while a change of its own is not stored,
  `setHidden(section, item, hidden)`,
  `moveTo(section, item, index)`, `moveRow(fromRow, toRow) → bool` (list rows of one
  section whose rows move: the row moves at once, so a dragged row follows the finger,
  and the order is stored behind it), `restoreDefaults()`, `sectionAt(row)`,
  `placeAt(row)` (-1 outside the list and for a section's switch), `itemAt(row)`;
  changes arrive as row changes, moves, insertions and
  removals, not a reset); `pages/BrowseSettingsPage.qml` with
  `components/browse/BrowseLayoutDelegate.qml { layoutModel, layoutSection, layoutItem,
  layoutKind, layoutName, layoutHidden, layoutMovable, layoutNested }` (the page passes
  the roles; signal `toggled()`).

### Directory (boards: Directory, DirectoryPulley, DirectoryContext, DirectorySelect, DirectorySelectPulley, DirectoryGrid, DirectoryLandscape, PathMenu, DirectoryRemorse, ImageDeleteRemote, ViewOptions, NewItem, Rename, FolderPicker, FolderPickerPulley, StateEmpty, StateNoAccess, StateOffline, StateError, StateLarge)

- `DirectoryModel { uri }` — roles per BRW-1 (`name`, `nameIsLossy`, `uri`, `isDir`,
  `isSymlink`, `size`, `modified`, `created`, `mode`, `owner`, `group`, `hidden`,
  `readonly`, `mimeType`, `category`, `icon`, `thumbnailSource` (`file://` URL for
  `Nemo.Thumbnailer`, `image://lautta-thumb/` URL, or empty; PRV-1, PRV-2, PRV-14),
  `selected`, `transferState`, `inaccessible`, `itemCount` (items of a folder, `-1` until
  `requestCount` has made it, for files, and for folders that are not counted),
  `itemCountCapped` (the folder has more entries than were counted)); properties
  `loading`, `stale`, `large`, `offline`,
  `errorKind`, `errorMessage`, `count`, `selectedCount`, `sortKey`, `descending`,
  `foldersFirst`, `showHidden`, `viewMode`, `filterText`, `suggestGrid`,
  `capabilities` (list of flag strings), `locationName`, `title`, `folderCounts`
  (folders here have an item count: the folder is on this device or a drive, not on a
  server or in an archive);
  methods `refresh()`, `revisit()` (lists again unless watched or an archive, BRW-11),
  `select(uri)`, `toggle(uri)`, `selectAll()`, `clearSelection()`,
  `selectedUris() → list`, `setViewPrefs(map)` (shows this folder with other view
  settings without storing them, `App.setSetting` does that; a `viewMode` replaces a
  suggested grid), `indexOf(uri)`, `uriAt(index)` (empty outside the rows), `kindOf(uri)`
  (`folder`, `file` or empty, like `nameKind(name)` but by uri), `requestCount(uri)` (counts
  the items of that folder off the GUI thread, from its names, at most 10 000 entries; the
  `itemCount` role of its row changes when done; asking again for an unchanged folder does
  nothing, a changed modified time or `showHidden` counts anew). An open model follows
  every change of the global view settings at once (BRW-4).
- The grid (SPEC §15.3): `components/directory/GridMetrics.qml` (`cellHeight(cellWidth,
  captioned)`, `gap(captioned)`, `glyphSize(tileWidth, captioned)`, `nameLines(name,
  tileWidth)`; the one place for the sizes of DirectoryView's and ViewPreview's cells),
  `components/directory/GridCaption.qml { metrics, name, captions, emphasised, pressed }`
  (the name on up to two lines and two caption lines under a tile at 3 per row),
  `components/directory/GridName.js` (`lines(name, width, measure)`: the cut of a long name,
  its end kept, no Silica), `components/directory/DirFormat.js` (`modifiedWithTime(ms)`,
  `items(count, capped)`, `gridCaptions(isDir, size, modifiedMs, counted, count, capped)` →
  the caption lines). A grid cell asks `requestCount` for its folder when it appears.
- `PathModel { uri }` — ancestors (roles `uri`, `name`), `complete(prefix) → list`.
- Pages: `pages/DirectoryPage.qml { uri }`, `dialogs/FolderPickerDialog.qml { title,
  acceptText, startUri }` (property `selectedUri` after accept),
  `dialogs/NewItemDialog.qml { parentUri }` (a new folder: property `itemName` after
  accept, the caller calls `DirectoryModel.createFolder(name)`),
  `dialogs/RenameDialog.qml { uri, isDir }` (property `newName` after accept),
  `pages/ViewOptionsPage.qml { model }` (a page whose controls apply at once; `model`, the
  folder it was opened from, is absent when Settings opens it; its Thumbnails section sets
  `thumbnails_local`, `thumbnails_remote`,
  `thumbnail_max_remote_mb`, `thumbnail_cache_mb`),
  `components/directory/ViewPreview.qml { screenWidth, screenHeight }` (its live preview,
  half that size whatever it shows), `components/PathMenu.qml`.
- The item context menu is plain rows ending with *More actions*, which opens
  `components/directory/MoreActionsPage.qml { owner, info }` (the DirectoryView and the
  row's item): the item's rarer actions, one row each with its theme icon; a dialog an
  action opens (Rename's too) replaces the page (`DirectoryView.openDialog`). Which actions each holds,
  in a fixed order filtered by capabilities, and their icons, is in
  `components/directory/ContextActions.js` (`menu(ctx)`, `more(ctx)` →
  `[{ id, text, icon }]`, `icon(id)`).
- After `DirectoryModel.renamed(oldUri, newUri)` the DirectoryView shows a Silica remorse on
  the new name's row (OPS-9); every list row and grid cell holds a
  `components/directory/RenameRemorse.qml { owner, row, uri }` so that a row made anew
  while it runs shows the rest. Cancelling it calls `rename(newUri, oldName)`.
- When selection mode starts and ends, what its panel offers (`panelButtons(count,
  writable)`: Share, Copy, Cut, Delete, More actions, Stop selecting (`DockButton.key`:
  highlight key); `moreRows(count, ctx)`:
  the rows of the page behind More actions) and how a drag selects a range (`dragStart`,
  `dragTo`, `autoScroll`) is in `components/directory/Selection.js`;
  `components/directory/DragSelect.qml` runs the drag for the grid and list delegates (§15.3).
  The panel's More actions button calls `DirectoryView.moreSelection()`, which pushes
  `MoreActionsPage.qml { owner, info, selection: true, count }` (`info` the first selected
  item, `count` fixed at push): its rows run `DirectoryView.runSelectionAction(id)` on the
  whole selection.
- Which names New folder and Rename refuse is in `components/directory/Names.js`
  (`invalid(name)`).

### Operations (boards: PlanSummary, Conflict, Compress, Extract, RecentlyDeleted, RecentlyDeletedEmpty, ShareTarget, ArchiveView (open as location))

- `Operations` singleton: `copyTo(uris, destUri)`, `moveTo(uris, destUri)`,
  `remove(uris)` (after the remorse timer for a delete to the Trash, after
  `dialogs/DeleteDialog.qml` for a permanent one; signal `removed(trashedCount,
  transferId)`), `deletesPermanently(uris)` → bool (true unless every item goes to the
  Trash: the one place that decides between remorse and the dialog, OPS-8),
  `compress(uris, destUri, name, kind)`, `extract(archiveUri, destUri)`,
  `openArchive(uri)` → signal `archiveOpened(uri, rootUri)`;
  signals `needsSummary(planId, summary)` (`summary`: files, dirs, bytes, conflicts,
  renamed, kind, destination) → `startPlan(planId)` / `discardPlan(planId)`;
  `started(transferId)`; `failed(kind, message)`.
- `TrashModel` — the Trash in the app's data (OPS-8), `reload()` (also
  purges what is past the retention setting), `restore(id)`, `remove(id)`, `empty()`.
  Roles: `trashId`, `name`, `originalUri`, `folderUri`, `folderName`, `isDir`, `size`
  (-1 unknown), `trashedAt`, `daysLeft`, `mimeType` (from the original name, "" for
  folders and unknown types) and `thumbnailSource` (`file://` URL of the stored copy of
  an image or video, or `image://lautta-thumb/lautta://trash/<id>/<stored name>` for a
  PDF (PRV-14), while *Show thumbnails* is on, else ""; for `FileIcon`).
- Pages: `dialogs/PlanSummaryDialog.qml { planId, summary }`,
  `dialogs/ConflictDialog.qml { transferId, item, conflict }`,
  `dialogs/CompressDialog.qml { uris }`,
  `dialogs/ExtractDialog.qml { archiveUri }`, `dialogs/DeleteDialog.qml { uris }`
  (*Delete permanently?*; the caller deletes on `accepted`), `pages/TrashPage.qml`,
  `pages/ShareTargetPage.qml { resources }` (Sailfish.Share `ShareProvider`, INT-1).

### Transfers (boards: Transfers, TransfersPulley, TransferDetails, TransferDetailsPulley, TransfersRestored, Notifications, Covers (transfer cover))

- `Transfers` singleton: `activeCount`, `busy` (drives `KeepAlive`, XFR-5),
  `bytesDone`, `bytesTotal`, `rate`, `eta`, `pendingAtStart`, `pauseAll()`, `resumeAll()`,
  `pause(id)`, `resume(id)`, `cancel(id)`, `retryFailed(id)`, `moveUp(id)`,
  `moveDown(id)`, `moveToTop(id)`, `answer(id, item, choice, applyToAll)`,
  `clearHistory()`; signals `finished(id, ok, failures)`, `needsAnswer(id, item, conflict)`.
  Activity bar (XFR-13): `barMode` (`hidden`, `question`, `running`, `waiting`, `paused`,
  `failed`, `done`), `barTransferId`, `barUnfinished`, `dismissBar()` (swiped away),
  `barSeen()` (the Activity page shows the outcome); `components/transfers/ActivityBar.qml`.
- `TransfersModel` — what has not ended in queue order, then what ended, newest first
  (roles: `transferId`, `group`, `dateGroup`, `kind`, `title`, `state`, `waitReason`,
  `bytesDone`, `bytesTotal`, `rate`, `eta`, `itemsDone`, `itemsTotal`, `itemsFailed`,
  `direction`). `group` is what the task does (`active`, `waiting`, `paused`,
  `history`); `dateGroup` is its section (§15.4): `inProgress` until it ends, then
  `today`, `yesterday`, `last7Days`, `last30Days` or `older` by when it ended, shown by
  `components/DateSectionHeader.qml { group }`.
- `TransferItemsModel { transferId }`.
- Pages: `pages/TransfersPage.qml`, `pages/TransferDetailsPage.qml { transferId }`,
  `components/TransferNotifications.qml`
  (Nemo.Notifications, INT-2), cover content for transfers inside `cover/CoverPage.qml`
  (browse owns the file; transfers provides `cover/TransferCover.qml`).

### Viewers (boards: ImageViewer, ImageExif, TextViewer, TextViewerPulley, Markdown, PdfViewer, VideoPlayer, AudioPlayer, OpenRemote)

- `TextDocument { uri }` (text, truncated, validUtf8, size; read-only; PRV-13: `syntax`
  (the grammar's name, "" for plain text), `highlighted` (the first lines, or all, have
  colours; may turn true after `loaded`), `styledRow(text, line, start, colours)` (a
  row of `TextRows.rows` (its `text`, `line` and `start`) as Qt StyledText for a
  `Text.StyledText` label: every character escaped, whitespace kept, coloured with
  `colours` as its part of the line; a line without colours, or past the end, comes
  back escaped only)),
  `components/viewers/SyntaxColours.js` `palette(Theme)` (the `colours` for the
  ambience: `class=#rrggbb,…` for comment, keyword, operator, string, number,
  constant, type, function, variable, heading, markup, link, inserted, deleted),
  `MarkdownDocument { uri }` (html), `ExifModel { uri }`, `MediaSource { uri }` (QIODevice for QtMultimedia, PRV-8),
  image provider `image://lautta-thumb/<uri>?size=N` for thumbnails of remote images
  (PRV-2) and of PDFs, local and trashed too (PRV-14; fails with an error for anything
  else, which `Image` shows as status Error), and `image://lautta-file/<uri>` for remote full images.
- `PdfDocument { uri }` (PRV-12): `loading`, `loaded`, `pageCount`, `size`, `errorKind`
  (file not readable, ErrorText.js), `problem` (`invalid`, `encrypted`, `empty`,
  `tooLarge`), `wrongPassword` (the last `unlock` failed), `lastPage` (where reading
  stopped, from 0; -1 unknown), `heightFor(index, width)`, `reload()`,
  `unlock(password)` (opens an encrypted PDF; the password is not kept),
  `rememberPage(index)`; text search: `search(query, fromPage)` (from page `fromPage`,
  from 0, to the end, then from the first page; replaces the last search; a blank
  query searches nothing), `stopSearch()` (stops and forgets the hits), `searching`,
  `searchedPages` (all pages once a search has run through), `hitCount` (at most
  10 000), `currentHit` (from 1 in page order, 0 none; the first hit at or after
  `fromPage` until `nextHit()`/`previousHit()`, which go round), `currentHitPage` (from
  0, -1 none), `hitsJson(index)` (JSON array of `{x, y, width, height, current}` per
  hit area on page `index`, fractions of the page from its top left), `hitsRevision`
  (changes with the hits or the hit shown; bindings of `hitsJson` read it); all notify
  `searchChanged()`; `outline` (JSON `[{title, level, page}]`, page from 0 or -1),
  `requestLinks(index)` then `linksReady(linkPage, pageLinks)` (JSON `[{x, y, width,
  height, page, top, uri}]`: area as fractions of the shown page; page from 0 and `top`
  as a fraction of its height or -1; or page -1 and an http(s) `uri`). Image provider
  `image://lautta-pdf/<uri>?page=N` renders page N (from 0) at `sourceSize.width`
  (1080 without one), any location.
- Pages: `viewers/ImageViewer.qml { uri, folderUri }` (Jolla Gallery's viewer, adapted in
  `components/viewers/gallery`; `remove()` deletes the image shown; its *More* is
  `components/viewers/ImageMorePage.qml { uri }`, whose Rename signals `renameChosen()` for
  the viewer to rename), `viewers/TextViewer.qml { uri, wrap }`
  (`wrap` true by default; a `textBackdrop` rectangle fills the page under the text,
  PRV-4; its rows, tabs expanded and long lines split, come from
  `components/viewers/TextRows.js` `rows(text)`: `{ rows: [{ number, text, line,
  start }], longest, lines }`, `line` from 0 and `start` the row's first column in
  the line with tabs expanded, in UTF-16 units),
  `viewers/MarkdownViewer.qml { uri }`, `viewers/PdfViewer.qml { uri }`, `viewers/PdfContentsPage.qml { fileName, outline,
  pageCount, currentPage }` (signal `chosen(index)`), `viewers/MediaPlayer.qml { uri, video }`,
  `dialogs/OpenRemoteDialog.qml { uri }`, `dialogs/PdfPasswordDialog.qml { fileName, wrong }`,
  `dialogs/PdfLinkDialog.qml { url }`.

### Search, settings (boards: Search, SearchFilters, Settings, About)

- `SearchModel { rootUri }` — query/filters, results grouped by folder, `start()`,
  `cancel()`, `recentSearches`.
- Pages: `pages/SearchPage.qml { rootUri }`, `pages/SearchFiltersPage.qml`,
  `pages/SettingsPage.qml` (entries, `components/search/SettingsEntry.qml { iconSource,
  text, value }`, whose value lines come from `components/search/SettingsText.js`; they
  open `pages/BrowseSettingsPage.qml`, `pages/ViewOptionsPage.qml` without a model (the
  view settings and the thumbnails are not repeated, BRW-4), `pages/SearchSettingsPage.qml`,
  `pages/RecentsSettingsPage.qml`, `pages/DeletingSettingsPage.qml`,
  `pages/TransfersSettingsPage.qml`, `pages/StorageSettingsPage.qml` and
  `pages/AboutPage.qml`). A control bound to one setting is
  `components/search/SettingSwitch.qml`, `SettingCombo.qml` or `SettingSlider.qml`
  `{ settingKey }`.
- The search page has no pulley: its header, search field and filter summary (which
  opens `SearchFiltersPage`) sit above the results list, not in the list's header, so
  the field keeps the keyboard while each search resets the list.

## Shared QML (core infra owns)

- `harbour-lautta.qml` — `ApplicationWindow`, initial page `BrowsePage`, global handlers
  for `Operations.needsSummary`, `Transfers.needsAnswer`, `Bridge.question`, the undo
  banner and the app-wide `KeepAlive`.
- `components/ErrorText.js`, `components/Open.js`, `components/FileIcon.qml { category,
  isDir, thumbnailSource, mimeType, size, glyphSize }` (`glyphSize`: the side of the theme
  glyph in the tile, `AppIcon.iconSize`; read-only `thumbnailReady`, `thumbnailFailed`: the
  thumbnail shows / could not be made, so the category icon stays),
  `components/FileIcons.js` (icons every file view shares: `archive`),
  `components/IconRow.qml` (icon row for context menus),
  `components/UndoBanner.qml`, `components/BarBackground.qml` (the opaque backdrop of
  bottom bars, UI-10), `components/PlaceLabel.qml { before, text, remote, color,
  fontSize, horizontalAlignment }` (a line naming a location: a remote one gets the small
  server icon before its name, §15.5).

## Tests

- QML: `tests/qml/tst_*.qml` are instantiated by `harbour-lautta --qml-check` in the SDK
  target (real Silica, real types); failures are QML errors, warnings naming the file and
  `console.error` calls. Every page gets a test that instantiates it with realistic
  properties inside an `ApplicationWindow`.
- Rust: facades keep logic in `lautta-core`; what remains in `lautta-qt` is tested on the
  host with Qt 5.15 (`QT_QPA_PLATFORM=offscreen`) through `lautta_qt::testing`.
