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
| `Operations` | operations | plans, conflicts, delete, compress/extract, permissions |
| `Transfers` | transfers | queue summary and control, cover data, keep-alive state |

### `App`

Properties: `ready: bool`, `startError: string`, `version: string`,
`settingsJson: string` (object, keys of `lautta_core::settings::Settings::to_map`),
`canUndo: bool` (OPS-9 banner), `undoText: string` (engineering kind: `rename`, `move`,
`trash`), `clipboardCount: int`, `clipboardCut: bool`, `crashReports: int`.

Methods:
- `setting(key) → var` (bool, number or string; lists as JSON text), `setSetting(key,
  valueJson)` (value as JSON text, e.g. `JSON.stringify(true)`); `settingsJsonChanged()`
  signal. QML persists the settings JSON in dconf (`Nemo.Configuration`,
  `/apps/harbour-lautta/settings`) and calls `App.loadSettings(json)` at start (SPEC §18).
- `childUri(parentUri, name) → string`, `parentUri(uri) → string`, `nameOf(uri) → string`,
  `displayAddress(uri) → string` (file:// path or netvfs URL without secrets, LOC-7),
  `locationName(uri) → string`, `isLocal(uri) → bool`.
- `copy(uris)`, `cut(uris)`, `clearClipboard()`, `canPasteInto(uri) → bool`,
  `paste(destUri)` (→ `Operations` flow).
- `undo()`; signal `undone(ok)`.
- `categoryOf(nameOrUri) → string` (FileCategory icon name), `viewerFor(uri, mime) → string`
  (`image`, `text`, `markdown`, `audio`, `video`, `archive`, `sqlite`, `hex`, `external`).
- `open(uri)`: decides by viewer and pushes the viewer page (through
  `components/Open.js`); `external` → `openExternally(uri)`.
- `openExternally(uri)`: local → system handler; remote → copy to
  `~/Downloads/Lautta/Opened/` first (PRV-6) and then open; signal
  `externalOpenStarted(uri, transferId)`.
- `crashReport() → string`, `dismissCrashReports()`.

## Instantiable types by area

Each area owns its types and pages. Names are fixed here so areas can use each other.

### Browse (boards: Main, BrowsePulley, BrowseStandalone, BrowseServerMenu, BrowseVolume, ConnectServer, ServerQuestion, ServerAdhocMenu, FavouriteEdit, TagPage, TagPagePulley, TagAssign, Recents, RecentsPulley, RecentsFilter, StateReconnecting, StateIdentity, LocationSettings, Covers)

- `LocationsModel` — sections `favourites`, `device`, `android`, `volumes`, `servers`,
  `nearby`, `tags` (roles: `section`, `uri`, `name`, `kind`, `icon`, `status`,
  `attention`, `colour`, `count`, `itemId`).
- `FavouritesModel`, `TagsModel`, `TaggedItemsModel { tagId }`, `RecentsModel { filter }`.
- `Bridge` singleton: `status` (`absent`, `connecting`, `tooOld`, `consentUnknown`,
  `consentDenied`, `ready`, `reconnecting`), `requestConsent()`, `addServer(provider)`,
  `editAccount(locationUri)`, `connectAdHoc(url, secret, options)`, `forgetAdHoc(id)`,
  `disconnect(id)`, signal `question(id, kind, details)`, `answer(id, answer)`.
- Pages: `pages/BrowsePage.qml` (initial page), `pages/RecentsPage.qml`,
  `pages/TagPage.qml { tagId }`, `dialogs/ConnectServerDialog.qml { url }`,
  `dialogs/ServerQuestionDialog.qml { questionId, kind, details }`,
  `dialogs/FavouriteDialog.qml { uri, favouriteId }`, `dialogs/TagAssignDialog.qml { uris }`,
  `pages/LocationSettingsPage.qml { locationId }`, `cover/CoverPage.qml`.

### Directory (boards: Directory, DirectoryPulley, DirectoryContext, DirectorySelect, DirectorySelectPulley, DirectoryGrid, DirectoryLandscape, TwoPane, PathMenu, DirectoryRemorse, ImageDeleteRemote, ViewOptions, NewItem, Rename, FolderPicker, FolderPickerPulley, StateEmpty, StateNoAccess, StateOffline, StateError, StateLarge, ContextMenuOrder (order only))

- `DirectoryModel { uri }` — roles per BRW-1 (`name`, `nameIsLossy`, `uri`, `isDir`,
  `isSymlink`, `size`, `modified`, `created`, `mode`, `owner`, `group`, `hidden`,
  `readonly`, `mimeType`, `category`, `icon`, `thumbnailSource`, `selected`,
  `transferState`); properties `loading`, `stale`, `large`, `offline`,
  `errorKind`, `errorMessage`, `count`, `selectedCount`, `sortKey`, `descending`,
  `foldersFirst`, `showHidden`, `viewMode`, `filterText`, `chips`, `suggestGrid`,
  `capabilities` (list of flag strings), `locationName`, `title`;
  methods `refresh()`, `select(uri)`, `toggle(uri)`, `selectAll()`, `clearSelection()`,
  `selectedUris() → list`, `setViewPrefs(map)`, `indexOf(uri)`.
- `PathModel { uri }` — ancestors (roles `uri`, `name`), `complete(prefix) → list`.
- Pages: `pages/DirectoryPage.qml { uri }`, `dialogs/FolderPickerDialog.qml { title,
  acceptText, startUri }` (property `selectedUri` after accept),
  `dialogs/NewItemDialog.qml { parentUri }`, `dialogs/RenameDialog.qml { uri }`,
  `pages/ViewOptionsPage.qml { model }`, `components/PathMenu.qml`.
- Context menus follow the design: an icon row first (`components/IconRow.qml`) and a
  short list; order from `JSON.parse(App.setting("context_menu"))`, filtered by capabilities.

### Operations (boards: PlanSummary, Conflict, BulkRename, Info, Permissions, Compress, Extract, RecentlyDeleted, RecentlyDeletedEmpty, ShareTarget, ArchiveView (open as location))

- `Operations` singleton: `copyTo(uris, destUri)`, `moveTo(uris, destUri)`,
  `remove(uris)` (after remorse; signal `removed(trashedCount, transferId)`),
  `compress(uris, destUri, name, kind)`, `extract(archiveUri, destUri)`,
  `openArchive(uri)` → signal `archiveOpened(uri, rootUri)`;
  signals `needsSummary(planId, summary)` (`summary`: files, dirs, bytes, conflicts,
  renamed, kind, destination) → `startPlan(planId)` / `discardPlan(planId)`;
  `started(transferId)`; `failed(kind, message)`.
- `BulkRenameModel { uris }` — rules editing and live preview (`old`, `new`, `status`),
  `apply()`.
- `InfoModel { uri }` — details, checksum (`computeChecksum(algo)`), `tags`, `setModified()`.
- `PermissionsModel { uri }` — rwx grid, octal, recursive with file/folder masks, `apply()`.
- `TrashModel` — Recently deleted list, `restore(id)`, `remove(id)`, `empty()`.
- Pages: `dialogs/PlanSummaryDialog.qml { planId, summary }`,
  `dialogs/ConflictDialog.qml { transferId, item, conflict }`,
  `pages/BulkRenamePage.qml { uris }`, `pages/InfoPage.qml { uri }`,
  `pages/PermissionsPage.qml { uri }`, `dialogs/CompressDialog.qml { uris }`,
  `dialogs/ExtractDialog.qml { archiveUri }`, `pages/RecentlyDeletedPage.qml`,
  `pages/ShareTargetPage.qml { resources }` (Sailfish.Share `ShareProvider`, INT-1).

### Transfers (boards: Transfers, TransfersPulley, TransferDetails, TransferDetailsPulley, TransfersRestored, EditConflict, Notifications, Covers (transfer cover))

- `Transfers` singleton: `activeCount`, `busy` (drives `KeepAlive`, XFR-5),
  `bytesDone`, `bytesTotal`, `rate`, `eta`, `pendingAtStart`, `pauseAll()`, `resumeAll()`,
  `pause(id)`, `resume(id)`, `cancel(id)`, `retryFailed(id)`, `moveUp(id)`,
  `moveDown(id)`, `moveToTop(id)`, `answer(id, item, choice, applyToAll)`,
  `clearHistory()`; signals `finished(id, ok, failures)`, `needsAnswer(id, item, conflict)`.
- `TransfersModel` — groups `active`, `waiting`, `paused`, `edited`, `history`
  (roles: `transferId`, `group`, `kind`, `title`, `state`, `waitReason`, `bytesDone`,
  `bytesTotal`, `rate`, `eta`, `itemsDone`, `itemsTotal`, `itemsFailed`, `direction`).
- `TransferItemsModel { transferId }`, `WorkingCopiesModel`.
- Pages: `pages/TransfersPage.qml`, `pages/TransferDetailsPage.qml { transferId }`,
  `dialogs/EditConflictDialog.qml { copyId }`, `components/TransferNotifications.qml`
  (Nemo.Notifications, INT-2), cover content for transfers inside `cover/CoverPage.qml`
  (browse owns the file; transfers provides `cover/TransferCover.qml`).

### Viewers (boards: ImageViewer, ImageExif, TextViewer, TextViewerPulley, TextEditor, Markdown, VideoPlayer, AudioPlayer, SqliteViewer, HexViewer, OpenRemote)

- `TextDocument { uri }` (text, truncated, validUtf8, lineEnding, `save(text)`),
  `MarkdownDocument { uri }` (html), `HexModel { uri }`, `SqliteModel { uri }`,
  `ExifModel { uri }`, `MediaSource { uri }` (QIODevice for QtMultimedia, PRV-8),
  image provider `image://lautta-thumb/<uri>?size=N` for remote thumbnails (PRV-2) and
  `image://lautta-file/<uri>` for remote full images.
- Pages: `viewers/ImageViewer.qml { uri, folderUri }`, `viewers/TextViewer.qml { uri }`,
  `viewers/TextEditor.qml { uri }`, `viewers/MarkdownViewer.qml { uri }`,
  `viewers/MediaPlayer.qml { uri, video }`, `viewers/SqliteViewer.qml { uri }`,
  `viewers/HexViewer.qml { uri }`, `dialogs/OpenRemoteDialog.qml { uri }`.

### Search, compare, settings (boards: Search, SearchFilters, SearchPulley, CompareSetup, CompareResults, Settings, ContextMenuOrder, About, CrashReport)

- `SearchModel { rootUri }` — query/filters, results grouped by folder, `start()`,
  `cancel()`, `recentSearches`.
- `CompareModel { leftUri, rightUri }` — items with status, exclusions, `syncPreview(mode)`,
  `sync(mode)`; saved pairs via `SyncPairsModel`.
- Pages: `pages/SearchPage.qml { rootUri }`, `pages/SearchFiltersPage.qml`,
  `pages/CompareSetupPage.qml { leftUri }`, `pages/CompareResultsPage.qml`,
  `pages/SettingsPage.qml`, `pages/ContextMenuOrderPage.qml`, `pages/AboutPage.qml`,
  `pages/CrashReportPage.qml`.

## Shared QML (core infra owns)

- `harbour-lautta.qml` — `ApplicationWindow`, initial page `BrowsePage`, global handlers
  for `Operations.needsSummary`, `Transfers.needsAnswer`, `Bridge.question`, the undo
  banner and the app-wide `KeepAlive`.
- `components/ErrorText.js`, `components/Open.js`, `components/FileIcon.qml { category,
  isDir, thumbnailSource }`, `components/IconRow.qml` (icon row for context menus),
  `components/UndoBanner.qml`.

## Tests

- QML: `tests/qml/tst_*.qml` are instantiated by `harbour-lautta --qml-check` in the SDK
  target (real Silica, real types); failures are QML errors, warnings naming the file and
  `console.error` calls. Every page gets a test that instantiates it with realistic
  properties inside an `ApplicationWindow`.
- Rust: facades keep logic in `lautta-core`; what remains in `lautta-qt` is tested on the
  host with Qt 5.15 (`QT_QPA_PLATFORM=offscreen`) through `lautta_qt::testing`.
