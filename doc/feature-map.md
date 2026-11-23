# Lautta feature map

Shared names for Lautta's screens, features and flows. Each feature: how user reaches it (*Entry*) and what on screen proves it worked (*Proof*). Flows: `doc/flows.md`; feature ↔ flow ↔ screen: `doc/flow-mapping.md`; flows as phone screenshots: `doc/canvas/index.html` (open in browser).

IDs are stable handles for chat, issues, PRs, tests: `SCR-*` screen, `F-*` feature, `FL-*` flow (`doc/flows.md`). Name in **bold** = canonical term; use it in code comments, PRs and talk. Mode: **S** standalone (no netvfs), **B** needs netvfs bridge.

## 1. Vocabulary

| Term | Means | Not |
|---|---|---|
| **Browse** | start page: sections Favourites, On this device, Android, Volumes, Servers, Nearby | "home", "main" |
| **location** | root Lautta can list: user folder, Android folder, volume, server, archive | "place" (UI word for a location's name line), "drive" |
| **user folder** | sandbox-granted `~/Documents`, `~/Pictures`, … ("On this device") | "home folder" |
| **Android folder** | `~/android_storage/…` folder (section Android) | |
| **volume** | memory card or USB drive under `/run/media` | "drive" is the netvfs/udisks object behind it |
| **drive** | udisks2 device netvfs mounts/unmounts; unmounted or locked drive shows as volume row | |
| **server** | netvfs location: account from Settings → Accounts (SFTP, SMB, WebDAV, FTPS); Lautta takes no address or password | "share", "NAS" |
| **Nearby** | servers netvfs discovers on LAN (mDNS), section on Browse; tap offers set-up as account | |
| **bridge** | netvfs-bridge, p2p D-Bus peer; present → bridge mode | "daemon" |
| **consent** | netvfs's first ask whether Lautta may use network locations; netvfs notification | "permission" (that is sailjail) |
| **access level** | what netvfs lets Lautta do per server: Full access, Read and add, Read only, Add only; not allowed = not listed | |
| **attention** | server state needing user: `auth-failed`, `server-identity-changed` | |
| **folder page** | `DirectoryPage`: one folder's listing | "directory view" in UI talk |
| **list / grid** | folder view modes (grid 3 or 4 per row) | |
| **path crumbs** | folder header, left of folder name: house (Browse), then folders above, each a tap target; what doesn't fit folds into "…" | "breadcrumbs", "path menu" |
| **pulley** | Silica pull-down menu at page top | "pull-down", "menu" |
| **context menu** | long-press menu on a row | |
| **More actions** | page listing the rest of item actions | |
| **selection mode** | multi-select state; **selection bar** docked at bottom | "edit mode" |
| **drag-select** | long-press then drag across items to select range | |
| **clipboard** | copied or cut items waiting for **Paste**; **clipboard panel** at bottom | |
| **remorse** | Silica countdown with undo before an action runs (trash, move, remove favourite, rename) | "undo toast" |
| **task** | one queued job (copy, move, delete, compress, extract, upload, download) | "transfer" in code (`Transfers`) |
| **Activity** | page listing tasks; **activity bar** = slim bar for running task on Browse/folder page | "transfers page", "queue" |
| **plan summary** | dialog before a large operation: items, size, free space, conflicts, options | |
| **conflict** | name clash at destination; **conflict dialog** resolves (Merge, Keep both, Skip, Replace, Replace if newer, Resume) | |
| **Trash** | Lautta's own trash for user-folder deletes ("Recently deleted" in design) | system trash |
| **Recents** | list of files opened, edited or transferred | "history" |
| **favourite** | pinned folder or file on Browse with label, colour, icon | "bookmark" |
| **viewer** | built-in page for image, video, audio, text, Markdown, PDF | "preview" (grid tile thumbnail) |
| **thumbnail** | grid/list image of file; **text thumb** = first lines of text file | |
| **working copy** | local copy of remote file for another app (`Downloads/Lautta/Opened`, 24 h) | "cache" |
| **archive** | zip, tar(.gz/.bz2/.xz/.zst), 7z opened read-only as location; on screen "compressed file" | "archive" on screen |
| **scan** | camera → pages → one PDF | |
| **share target** | Lautta in Sailfish share sheet: "Save here" folder browser | share *out* = Share action |
| **cover** | app cover: current folder, or task progress | |
| **offline listing** | cached listing shown when server unreachable | |
| **large folder** | over threshold: name order, thumbnails off, Stop loading | |

## 2. Screens

| ID | Screen (canonical) | QML | Mode | Reached from |
|---|---|---|---|---|
| SCR-01 | Browse | `pages/BrowsePage.qml` | S | app start |
| SCR-02 | Start error | `pages/StartErrorPage.qml` | S | start, data can't open |
| SCR-03 | Folder page (list) | `pages/DirectoryPage.qml` + `directory/DirectoryView.qml` | S | any folder row, favourite, path crumb, search result, Trash "Show original folder" |
| SCR-04 | Folder page (grid) | same, grid mode | S | View options, auto when mostly photos |
| SCR-05 | Folded path | `directory/PathCrumbs.qml` | S | tap "…" in folder header |
| SCR-06 | Folder pulley | `directory/DirectoryMenu.qml` | S | pull folder page |
| SCR-07 | Item context menu | `directory/FileContextMenu.qml` | S | long-press list row |
| SCR-08 | More actions | `directory/MoreActionsPage.qml` | S | context menu, selection bar |
| SCR-09 | Selection mode + selection bar | `directory/SelectionPanel.qml` | S | pulley Select, grid long-press |
| SCR-10 | Clipboard panel | `directory/ClipboardPanel.qml` | S | after Copy/Cut |
| SCR-11 | Folder states (empty, no access, error, offline, large, grid hint) | `DirectoryView.qml`, `InfoBanner` | S/B | folder page |
| SCR-12 | Add | `dialogs/AddDialog.qml` | S | pulley Add |
| SCR-13 | New folder | `dialogs/NewItemDialog.qml` | S | Add, Move to new folder, share target pulley |
| SCR-14 | Rename | `dialogs/RenameDialog.qml` | S | context menu, More actions, image viewer |
| SCR-15 | Delete permanently | `dialogs/DeleteDialog.qml` | S | delete on volume, server, Trash off, Trash "Delete now" |
| SCR-16 | Compress | `dialogs/CompressDialog.qml` | S | More actions, selection More, image More |
| SCR-18 | Share target: Locations | `pages/ShareTargetPage.qml` | S | house in share target's path band |
| SCR-19 | Plan summary | `dialogs/PlanSummaryDialog.qml` | S | large copy/move/extract |
| SCR-20 | Conflict | `dialogs/ConflictDialog.qml` | S | plan summary, activity bar Answer, Activity row, notification |
| SCR-21 | Activity | `pages/TransfersPage.qml` | S | Browse/folder pulley, activity bar Show, cover action, notification |
| SCR-22 | Task details | `pages/TransferDetailsPage.qml` | S | Activity row |
| SCR-23 | Activity bar | `transfers/ActivityBar.qml` | S | Browse, folder page while task runs |
| SCR-24 | View options | `pages/ViewOptionsPage.qml` | S | folder pulley, Settings |
| SCR-25 | Search | `pages/SearchPage.qml` | S | folder pulley Search |
| SCR-26 | Search filters | `pages/SearchFiltersPage.qml` | S | Search summary row |
| SCR-27 | Image viewer | `viewers/ImageViewer.qml` (+ `gallery/`) | S | tap image |
| SCR-28 | Image details | `viewers/ImageDetailsPage.qml` | S | image viewer (i) |
| SCR-29 | Image More | `viewers/ImageMorePage.qml` | S | image viewer (…) |
| SCR-30 | Media player (video, audio) | `viewers/MediaPlayer.qml` | S/B | tap video or audio |
| SCR-31 | Text viewer | `viewers/TextViewer.qml` | S | tap text/code file, Markdown or plain text "View source" |
| SCR-32 | Markdown viewer | `viewers/MarkdownViewer.qml` | S | tap `.md` (`.markdown`, `.mkd`, `.mdown`, `.mdwn`, `.markdn`) or `.txt` |
| SCR-33 | PDF viewer | `viewers/PdfViewer.qml` | S | tap PDF |
| SCR-34 | PDF contents | `viewers/PdfContentsPage.qml` | S | PDF pulley "Contents and pages" |
| SCR-35 | Locked file | `dialogs/LockedFileDialog.qml` | S | locked PDF, pulley Unlock; locked compressed file on open or Extract |
| SCR-36 | PDF link | `dialogs/PdfLinkDialog.qml` | S | tap web link in PDF |
| SCR-37 | Open remote (Getting file) | `dialogs/OpenRemoteDialog.qml` | B | open/share remote file without viewer |
| SCR-38 | Recents | `pages/RecentsPage.qml` | S | Browse tile All recents |
| SCR-39 | Trash | `pages/TrashPage.qml` | S | Browse row Trash |
| SCR-40 | Edit favourite | `pages/FavouritePage.qml` | S | Add to favourites (row, item, folder pulley, image More), Edit favourite |
| SCR-41 | Settings | `pages/SettingsPage.qml` | S | Browse pulley |
| SCR-42 | Settings › Browse page | `pages/BrowseSettingsPage.qml` | S | Settings |
| SCR-43 | Settings › Search | `pages/SearchSettingsPage.qml` | S | Settings |
| SCR-44 | Settings › Recents | `pages/RecentsSettingsPage.qml` | S | Settings |
| SCR-45 | Settings › Deleting | `pages/DeletingSettingsPage.qml` | S | Settings |
| SCR-46 | Settings › Transfers | `pages/TransfersSettingsPage.qml` | S | Settings |
| SCR-47 | Settings › Storage | `pages/StorageSettingsPage.qml` | S | Settings |
| SCR-48 | About | `pages/AboutPage.qml` | S | Settings |
| SCR-51 | Server attention | `browse/ServerAttentionPage.qml` | B | tap server row with attention |
| SCR-52 | Share target ("Save here") | `pages/ShareTargetPage.qml` | S | Sailfish share sheet → Lautta |
| SCR-53 | Scan camera | `pages/ScanPage.qml` | S | Add → Scan document |
| SCR-54 | Scan page view | `pages/ScanPageView.qml` | S | scan thumbnails, saved-pages strip |
| SCR-55 | Scan adjust (crop, turn) | `dialogs/ScanAdjustDialog.qml` | S | scan page view |
| SCR-56 | Scan save | `dialogs/ScanSaveDialog.qml` | S | scan Done |
| SCR-57 | Scan saving | `pages/ScanSavingPage.qml` | S | scan save accept |
| SCR-58 | Cover (page / tasks) | `cover/CoverPage.qml`, `cover/TransferCover.qml` | S | app in background |
| SCR-59 | Notifications | `components/TransferNotifications.qml` | S | tasks in background |
| SCR-60 | Nearby setup | `dialogs/NearbySetupDialog.qml` | B | tap Nearby row |
| SCR-61 | Checksums | `pages/ChecksumsPage.qml` | S | More actions, image More |
| SCR-63 | Checksum | `pages/ChecksumPage.qml` | S | tap algorithm |
| SCR-64 | Verify files | `pages/VerifyFilesPage.qml` | S | More actions of checksum list |

Retired, never reuse: SCR-49 Connect to server, SCR-50 Server question.

Browse rows (inside SCR-01): Recents tiles, favourite, user folder, Android folder, Trash, volume (mounted, unmounted, locked), server, Nearby server, consent banner, bridge-unavailable banner. Row status dot: connected, not connected, connecting, failed, needs user.

## 3. Features

Format: **F-ID Name** (mode) — what. *Entry:* where user starts it. *Proof:* what screen shows. *Code:* anchor.

### 3.1 Browse and locations

- **F-BRW-01 Browse start page** (S) — sections of all locations. *Entry:* app start. *Proof:* Browse lists sections. *Code:* `BrowsePage.qml`, `app_browse.rs`.
- **F-BRW-02 User folders** (S) — Documents, Downloads, Pictures, Music, Videos, Public, Playlists. *Entry:* Browse "On this device". *Proof:* folder page.
- **F-BRW-03 Android folders** (S) — `android_storage` folders. *Entry:* Browse "Android" (only when present).
- **F-BRW-04 Volumes** (S) — mounted memory cards, USB drives with free/total and file system line. *Entry:* Browse "Volumes".
- **F-BRW-05 Mount / Unmount drive** (B) — *Entry:* tap unmounted volume, or menu Mount; menu Unmount. *Proof:* row turns mounted/disappears from mounted state.
- **F-BRW-06 Locked drive → Storage settings** (B) — *Entry:* tap locked volume, menu "Open Storage settings". *Proof:* Sailfish Settings › Storage opens (bridge feature `storage-settings`).
- **F-BRW-07 Browse layout** (S) — hide sections (Recents, Volumes, Servers) and rows, reorder rows by drag, Restore defaults. *Entry:* Settings › Browse page. *Code:* `BrowseSettingsPage.qml`, `browse_layout.rs`.
- **F-BRW-09 Rescan volumes** (S) — Browse refreshes rows when shown and when app returns to front (sandbox can miss mounts). *Proof:* new card shows without restart.
- **F-BRW-10 Activity from Browse** (S) — pulley "Activity (n)". *Proof:* SCR-21.
- **F-BRW-11 Phone free space** (S) — "N free of M" at left end of "On this device" header: file system user folders are on; read when Browse shows. *Code:* `BrowsePage.qml`, `app_browse.rs` (`device_space`).

### 3.2 Servers and bridge (B)

- **F-SRV-01 Bridge detection** — socket present → server sections; absent → no trace. *Proof:* Servers section shown/hidden; About › Network locations status.
- **F-SRV-02 Consent banner** — "Network locations need your permission", Ask again. *Entry:* bridge present, access not decided.
- **F-SRV-03 Bridge unavailable banner** — "Retry" while reconnecting; listed servers stay.
- **F-SRV-04 Account servers** — SFTP, SMB, WebDAV, FTPS accounts from Settings › Accounts with *Files* on and an access level other than not allowed; row dot shows state; opening connects. *Entry:* Servers rows.
- **F-SRV-05 Add server** — pulley "Add server" opens Settings › Accounts (netvfs). *Proof:* Sailfish Settings opens.
- **F-SRV-06 Edit server / Update sign-in / Review server identity** — row menu, hands off to Settings.
- **F-SRV-07 Server attention page** — "Sign-in failed" or "Server identity changed" with Update sign-in / Review in Settings. *Entry:* tap row with attention.
- **F-SRV-11 Nearby** — mDNS-found servers while Browse is in front; tap → SCR-60 "This server isn't set up yet …", Set up opens Settings' add-account flow for that provider. *Proof:* "Nearby" section rows; Settings opens.
- **F-SRV-12 Offline listing** — cached listing with banner "Offline. Showing the copy from …", Retry. *Code:* `dircache.rs`.
- **F-SRV-13 Reconnect** — backoff, rows show connecting; running tasks wait then resume.
- **F-SRV-14 Remote thumbnails** — pictures, PDFs and audio covers from servers, size cap; audio reads tag area only. *Entry:* View options.
- **F-SRV-15 Remote streaming** — video/audio stream with seek ("Streaming from …"), else download first. *Code:* `MediaPlayer.qml`, `app_viewers.rs`.
- **F-SRV-16 Open remote in other app** — working copy to `Downloads/Lautta/Opened`, removed after 24 h. *Entry:* Open with / Share on remote file. *Proof:* SCR-37 "Getting …".
- **F-SRV-17 Access levels** — folder on a server with less than full access shows lock + level under header ("Read only", "Read and add", "Add only") and offers only what level allows: Read and add adds, changes nothing (no Cut, Rename, Delete); Read only adds nothing; Add only lists folders only ("Files here are hidden"), no Select, conflicts keep both. Folder picker greys out servers it can't write. Level narrowed mid-task → task fails "Lautta may now only … on …". *Entry:* open server with narrowed level. *Code:* `Access.js`, `ContextActions.js`, `access.rs`.

Retired, never reuse: F-SRV-08 Server question, F-SRV-09 Connect to server, F-SRV-10 Recent ad-hoc servers.

### 3.3 Folder page

- **F-DIR-01 Folder listing** — list with name, size/date line; folders show item counts; new items (made, pasted, extracted, compressed, scanned, renamed) scroll into view and light up briefly. *Code:* `DirectoryView.qml`, `app_directory.rs`.
- **F-DIR-02 Grid view** — 3/4 per row, captions, thumbnails (audio: cover), text thumbs; new tile's name and details briefly in ambience colour. *Entry:* View options › View.
- **F-DIR-03 Grid hint** — "Shown as grid because most items are photos", Use list.
- **F-DIR-04 Sort and filter** — sort by Name/Size/Modified/Type, Descending, Folders first, Show hidden files. *Entry:* View options (folder pulley or Settings).
- **F-DIR-05 Thumbnails switch** — Show thumbnails (untrusted-file warning), Thumbnails from servers, largest server file, cache size.
- **F-DIR-06 Path crumbs** — header shows house › folders above › folder name. Tap folder: back to its page; no page of it on stack → it replaces pages inside it. Tap house: back to Browse. Too long → farthest fold into "…": context menu under header, nearest first, as many as fit. Hidden while selecting. *Code:* `PathCrumbs.qml`, `PathFold.js`, `PathNav.js`.
- **F-DIR-07 Folder states** — empty ("Pull down to create a folder or paste"), hidden (Add only: "Files here are hidden"), no access, sign-in failed (Update sign-in), not found, can't reach (Retry, Details).
- **F-DIR-08 Large folder** — "N loaded · large folder: name order, thumbnails off", Stop.
- **F-DIR-09 Live updates** — inotify refresh of local folders. *Code:* `watch.rs`.
- **F-DIR-10 Listing cache** — cached folder visible at once, refreshed after. *Code:* `dircache.rs`; Storage › Keep stored listings.
- **F-DIR-11 Folder cover** — folder name top right, first items as caption-less tiles (thumbnail, first lines of a text file, or type glyph), three per row, last row fading; empty folder: "No files". Same for Trash and Recents under their titles. *Code:* `CoverGrid.qml`.

### 3.4 Item actions

Context menu order (list long-press): Share (files) · Copy · Cut · Rename · Delete · More actions. More actions: Verify files (checksum list) · Add to favourites · Extract (compressed file) · Compress · Checksums (files) · Move to new folder. Read-only location hides Cut, Rename, Delete, Move to new folder; Extract and Compress only where folder takes new files (place and folder permissions allow adding). *Code:* `ContextActions.js`.

- **F-ITM-01 Open item** — tap: folder → folder page; file → viewer by type (§3.8); archive → archive location; unknown → other app (local) or Open remote (server).
- **F-ITM-02 Share (out)** — Sailfish share sheet; remote file first fetched (SCR-37).
- **F-ITM-03 Copy / Cut** — to clipboard. *Proof:* clipboard panel.
- **F-ITM-04 Rename** — dialog, then remorse "File renamed" with undo.
- **F-ITM-05 Delete** — user folders with Trash on: remorse "Moving to trash"; volumes, servers, Trash off: SCR-15 "Delete permanently?".
- **F-ITM-06 Add to favourites** — from context More, folder pulley (open folder), Browse row, image More. *Proof:* SCR-40 "Added to favourites".
- **F-ITM-07 Compress** — Name, Format list (Zip, 7z, tar.xz, tar.gz, one line each, picked row lit), Password + Confirm password for Zip and 7z (AES-256; 7z hides names too; empty = none; mismatch greys Compress; guessable one: label "Weak password" in orange, never refused); file lands beside items, taken name gets " 2". *Proof:* notice "Compressed file created", new file lights up.
- **F-ITM-08 Move to new folder** — new folder dialog, then move.
- **F-ITM-09 Open with** — other app for local file (viewer pulley, image toolbar).
- **F-ITM-10 Checksums** — one file: SCR-61 sections Common (SHA-256, MD5, SHA-1, SHA-512) and More (SHA-224, SHA-384, SHA3-256, SHA3-512, BLAKE2b, BLAKE3, CRC32, XXH64, XXH128). Tap one → SCR-63 progress, then value; nothing runs before tap; Back stops it. Checksum list in same folder naming file → "Matches X" / "Doesn't match X". *Proof:* value; tap → "Copied". *Code:* `checksum.rs`, `app_checksum.rs`.
- **F-ITM-11 Verify files** — checksum list (`SHA256SUMS`, `x.sha256`, `.md5`, `.sfv`, BSD tags, clear-signed; signature not checked): SCR-64 row per listed file: Matches, Doesn't match, Missing, Not checked (outside folder, link, folder); problems first. *Proof:* rows. *Code:* `checksum/list.rs`.

### 3.5 Selection

- **F-SEL-01 Selection mode** — pulley Select; header "N selected"; tap toggles. Hard to enter by accident; "Stop selecting" leaves.
- **F-SEL-02 Drag-select** — long-press and drag across rows/tiles (grid: long-press starts it).
- **F-SEL-03 Select all / Clear selection** — pulley in selection mode.
- **F-SEL-04 Selection bar** — Share · Copy · Cut · Delete · More · Stop selecting. More: Rename (one item), Verify files (one checksum list), Add to favourites, Extract (one compressed file), Compress, Checksums (one file), Move to new folder (many: Compress, Move to new folder).

### 3.6 Clipboard and operations

- **F-OPS-01 Paste** — pulley "Paste N items" or clipboard panel "Paste here"; Cut → remorse "Moving …" then move. Panel: "N items copied/cut from …", Clear clipboard.
- **F-OPS-02 Plan summary** — before ops over threshold (Settings › Transfers › Ask before large operations): items, size, free on destination, conflicts, names changed (Use suggested names for all), options (Keep modification times, Keep permissions, Verify with checksums), Start.
- **F-OPS-03 Conflict resolution** — Merge (folders), Keep both ("Saves as …"), Skip, Replace, Replace if newer, Resume (unfinished copies); "Do this for the remaining N conflicts". Add only: Keep both only, "Lautta may only add files here."
- **F-OPS-04 Not-enough-space check** — error names place and sizes.
- **F-OPS-05 Extract** — no page: task starts (locked file not open: SCR-35 asks first). One top item lands as is beside compressed file, several go into new folder named after it; taken name gets " 2", nothing merged or replaced. *Entry:* More actions of compressed file, pulley inside one. *Proof:* task in Activity, result lights up in folder.
- **F-OPS-06 Archive location** — compressed file opens read-only as folder, header line "Compressed file · read only". Locked one (Zip AES, 7z AES): SCR-35 asks password, wrong one asks again; open while a page shows it, its items sit on clipboard or a task reads it, then closed and password gone (memory only, never stored). *Code:* `provider/archive.rs`, `app_archives.rs`.
- **F-OPS-07 Background notice** — "Running in the background" when task starts.

### 3.7 Activity (tasks)

- **F-XFR-01 Activity page** — sections In progress, Today, Yesterday, Last 7 days, Last 30 days, Older; header counts running/paused/stopped.
- **F-XFR-02 Task row menu** — Pause, Resume, Move to the top of the queue, Retry failed, Cancel (remorse).
- **F-XFR-03 Task details** — From, Destination, Started, Verify, Checksum, Size, Items; per-item Retry; pulley Cancel, Move up, Retry, Pause/Resume.
- **F-XFR-04 Pause all / Resume all / Clear activity history** — Activity pulley.
- **F-XFR-05 Restored tasks** — "N tasks stopped when Lautta closed. Nothing was lost." Resume; auto-resume setting.
- **F-XFR-06 Activity bar** — running task line on Browse/folder; "Answer" for conflicts, "Show" opens Activity.
- **F-XFR-07 Waiting states** — Waiting for network / memory card / connection / your answer, Queued, Paused at n%.
- **F-XFR-08 Concurrency and lanes** — Transfers at once on this device, Connections per server.
- **F-XFR-09 Notifications** — progress "N tasks · n%", finished, failed (tap to retry), conflicts, "Lautta needs an answer"; tap opens Activity or the conflict.
- **F-XFR-10 Transfer cover** — "N tasks" or "N paused" top right, percent and rate in the middle, whole cover fills see-through from the left with progress; cover actions Pause/Resume all and Open Activity.
- **F-XFR-11 Resume on start / keep history** — Settings › Transfers.
- **F-XFR-12 Item status in folder** — row line "Uploading to … · n%", "Downloading from …", "Waiting in Activity".

### 3.8 Viewers

- **F-VWR-01 Image viewer** — Jolla Gallery viewer: swipe for next/previous in folder, pinch zoom around fingers, swipe down to close; toolbar Delete, Open with, Share, More; (i) Details.
- **F-VWR-02 Image details** — Camera, Lens, Exposure, Taken, Dimensions, Size, Location, Orientation (EXIF; "applied" rotation note).
- **F-VWR-03 Image More** — Rename, Add to favourites, Compress (folder takes new files).
- **F-VWR-04 Media player** — video and audio; play/pause, back to start, +30 s, seek bar, tap toggles controls; remote stream or download. Audio: cover (from file, else folder `cover`/`folder`/`front` .jpg/.png, else note icon) and title, artist, album · year, track; no title tag → file name; blurred cover behind; landscape cover left.
- **F-VWR-05 Text viewer** — code, logs, config, data: syntax colours (syntect), read-only line "Read-only · type", first 1 MB, non-UTF-8 warning; pulley Open with, Share.
- **F-VWR-06 Markdown viewer** — Markdown rendered in ambience colours (highlight headings, ☐/☑ tasks, quote bar, code on tinted panel with syntax colours, ruled tables), `.txt` as prose in reading font; text viewer's backdrop; shown in blocks, only those on screen laid out; pulley View source, Share, Open with; first 1 MB.
- **F-VWR-07 PDF viewer** — pages, zoom, page count, reopen at last page, links (internal jump, web link dialog), Back to page n; pulley Unlock, Open with, Share, Search, Contents and pages.
- **F-VWR-08 PDF search** — search field with up/down hits, "n of m · page p", "Not found in N pages".
- **F-VWR-09 PDF contents / go to page** — outline list, "Go to page, 1 to N".
- **F-VWR-10 PDF password** — SCR-35 "Locked file", wrong-password retry.
- **F-VWR-11 External fallback** — unknown types open in other app; remote via working copy (F-SRV-16).

### 3.9 Search

- **F-SRC-01 Search in folder** — from folder pulley; type part of name or pattern (`*.pdf`); live results "Searching… N results", Stop; result opens item.
- **F-SRC-02 Search filters** — Name match Contains/Pattern, Type (Any, Folders, Images, Videos, Audio, Documents, Compressed files, Text files), Size, Modified, Include hidden files, Search depth on servers, Recent searches.
- **F-SRC-03 Server search depth** — Settings › Search ("N folders deep on servers"; device searches all).

### 3.10 Scan

- **F-SCN-01 Scan document** — Add → Scan document → camera page (flash, shutter, Done); "Camera not available" state. Camera only while page in front.
- **F-SCN-02 Page edge detection and crop** — quad outline, adjust corners, turn left/right (ScanAdjust).
- **F-SCN-03 Page view** — colour Original/Grayscale/Black and white, adjust, turn, retake, remove (remorse "Removing page"), "n of m".
- **F-SCN-04 Save as PDF** — name ("Scan …"), page strip (drag to reorder, tap to view), "Save N pages as PDF"; Saving page.

### 3.11 Favourites, Recents, Trash

- **F-FAV-01 Favourites section** — folders and files pinned on Browse; tap opens.
- **F-FAV-02 Edit favourite** — Label, Folder, Colour (Amber, Green, Red, Blue, Lime, Purple, Pink, Grey), Icon.
- **F-FAV-03 Remove favourite** — row menu, remorse "Removing favourite".
- **F-FAV-04 Reorder favourites** — Settings › Browse page drag.
- **F-RCT-01 Recents** — files opened, edited, transferred, under date groups; thumbnails as folders have them; list or grid as View options' View, in grid each date group starts its own row of tiles (name, place). Tap opens (transferred → its folder); menu Remove from recents; pulley Clear recents.
- **F-RCT-02 Recents off** — Settings › Recents "Remember recent files"; Browse tiles hide.
- **F-RCT-03 Recents on Browse** — section Recents above Favourites: three newest files as tiles (thumbnail, a text file's first lines, or type glyph; name), fourth tile "All recents" opens SCR-38. Tap opens (transferred → its folder); long press: Remove from recents. No section while Recents are off or empty; switch "Newest recents" in Settings › Browse page. *Code:* `RecentStrip.qml`, `app_browse.rs` (`recent_tiles`).
- **F-TRH-01 Trash** — items from user folders, "Kept for N days", "N days left", "From folder".
- **F-TRH-02 Restore / Delete now / Show original folder** — row menu.
- **F-TRH-03 Empty trash** — pulley, remorse.
- **F-TRH-04 Trash settings** — Move deleted items to Trash, Keep items in Trash (days), Time to undo (remorse seconds).

### 3.12 Share in, cover, notifications, app

- **F-SHR-01 Share target** — Lautta in share sheet; dialog "Save here" over a folder view: "Save N files" (one file: its name), path band with crumbs (F-DIR-06), folders open in place, files dimmed, list or grid as View options say, pulley New folder. Opens folder saved to last, first time Downloads. House → Locations: last three folders saved to, then device folders, volumes, servers. Save here off at Locations and where folder takes no files; unreadable files skipped, told in one line. *Code:* `ShareTargetPage.qml`, `directory/DirectoryView.qml` (`picking`), `directory/PathBand.qml`, `operations/Share.js`, `OperationsHandlers.qml`.
- **F-APP-01 Cover** — shows what a tap opens: page on top, else nearest page below that tells a cover. Browse: no name; free space of phone and mounted volumes (four at most) as icon with "N GB free", two to a row, phone alone on one centred line; below, newest recents as tiles (none: places). Folder, Trash, Recents: F-DIR-11. Viewer: file fills cover, name on dark fade; no picture: type glyph. Built only while cover shows; no timers. Transfer cover while tasks run. *Code:* `CoverPage.qml`, `CoverBrowse.qml`, `CoverFile.qml`.
- **F-APP-02 Start error** — "Lautta can't open its data".
- **F-APP-03 Settings** — Browsing (Browse page, View options, Search, Recents), Files (Deleting, Transfers, Storage), About Lautta.
- **F-APP-04 Storage** — Keep stored listings, Clear cache, Clear app data ("Removes queue, history, favourites and everything in Trash"; refuses while tasks run).
- **F-APP-05 About** — app icon, version, claim, network locations status, access "Set per server" and where to change it, licence, source code.
- **F-APP-06 Background notices** — Notice toasts for errors (`ErrorText.js`) and starts.

## 4. Feature × entry point (reach-from-everywhere check)

AGENTS.md: feature reachable from several places → all of them. ✓ = offered, — = not offered.

| Feature | Context menu | Selection bar | More actions | Folder pulley | Browse row | Image viewer | Text/MD/PDF viewer | Share target |
|---|---|---|---|---|---|---|---|---|
| Share | ✓ (files) | ✓ | — | — | — | ✓ | ✓ | n/a |
| Copy / Cut | ✓ | ✓ | — | — | — | — | — | — |
| Rename | ✓ | via More (1 item) | ✓ (selection) | — | — | ✓ (More) | — | — |
| Delete | ✓ | ✓ | — | — | — | ✓ | — | — |
| Add to favourites | via More | via More | ✓ | ✓ | ✓ (folder, volume) | ✓ (More) | — | — |
| Compress | via More | via More | ✓ | — | — | ✓ (More) | — | — |
| Extract | via More | via More (1 item) | ✓ | ✓ (inside compressed file) | — | — | — | — |
| Move to new folder | via More | via More | ✓ | — | — | — | — | — |
| Checksums | via More (files) | via More (1 file) | ✓ | — | — | ✓ (More) | — | — |
| Verify files | via More (checksum list) | via More (1 list) | ✓ | — | — | — | — | — |
| Open with | — | — | — | — | — | ✓ | ✓ | — |

