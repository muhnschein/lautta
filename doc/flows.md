# Lautta user flows

Every user flow: goal → steps → screens. IDs as `doc/feature-map.md`: `FL-*` flow, `SCR-*` screen, `F-*` feature.

Retired, never reuse: FL-31 Connect to a server once.

## Start

### FL-01 Start the app (both modes)

Open Lautta and see every location.

Features: F-BRW-01 Browse start page, F-BRW-02 User folders, F-BRW-03 Android folders, F-BRW-04 Volumes, F-SRV-01 Bridge detection, F-FAV-01 Favourites section, F-APP-02 Start error

| # | Screen | Step |
|---|---|---|
| 1 | SCR-01 Browse | Launch: Browse with Favourites, On this device, Volumes, Servers |
| 2 | SCR-01 Browse | Scroll: Volumes and Servers |
| 3 | SCR-01 Browse | End of list |
| 4 | SCR-01 Browse | Standalone: no Servers, no trace of netvfs |
| 5 | SCR-02 Start error | Data can't open: Start error |

## Browse

### FL-02 Browse into folders (standalone)

Go down into a folder and back.

Features: F-BRW-02 User folders, F-DIR-01 Folder listing, F-DIR-11 Folder cover, F-ITM-01 Open item

| # | Screen | Step |
|---|---|---|
| 1 | SCR-01 Browse | Tap a user folder |
| 2 | SCR-03 Folder page (list) | Folder page |
| 3 | SCR-03 Folder page (list) | Tap subfolder: one level down |
| 4 | SCR-03 Folder page (list) | Mixed folder as list |
| 5 | SCR-03 Folder page (list) | Back by swipe right |

### FL-03 Jump with the path menu (standalone)

Go to a parent folder, type a path, copy the address.

Features: F-DIR-06 Path menu, F-BRW-08 Copy address, F-ITM-06 Add to favourites

| # | Screen | Step |
|---|---|---|
| 1 | SCR-03 Folder page (list) | Tap folder header |
| 2 | SCR-05 Path menu | Path menu: crumbs, Edit path, Copy address, Add to favourites |

### FL-04 Change how a folder looks (standalone)

Sort, show hidden files, switch list/grid, thumbnails.

Features: F-DIR-02 Grid view, F-DIR-03 Grid hint, F-DIR-04 Sort and filter, F-DIR-05 Thumbnails switch

| # | Screen | Step |
|---|---|---|
| 1 | SCR-06 Folder pulley | Pull: View options |
| 2 | SCR-24 View options | View options |
| 3 | SCR-24 View options | Thumbnails part |
| 4 | SCR-04 Folder page (grid) | Photo folder opens as grid, hint offers list |

### FL-05 Big folder (standalone)

Open a folder of 20 000+ items.

Features: F-DIR-08 Large folder

| # | Screen | Step |
|---|---|---|
| 1 | SCR-11 Folder states (empty, no access, error, offline, large, grid hint, archive banner) | Large folder banner: name order, thumbnails off, Stop |

## View

### FL-06 Look at a photo (standalone)

Open, swipe through, see details, act on it.

Features: F-VWR-01 Image viewer, F-VWR-02 Image details, F-VWR-03 Image More, F-ITM-02 Share (out), F-ITM-05 Delete, F-ITM-09 Open with

| # | Screen | Step |
|---|---|---|
| 1 | SCR-03 Folder page (list) | Tap photo |
| 2 | SCR-27 Image viewer | Image viewer: (i) details, close, first-use hint |
| 3 | SCR-27 Image viewer | Tap: chrome hides, photo only |
| 4 | SCR-27 Image viewer | Chrome back: (i), close; Delete, Rename, Share, More |
| 5 | SCR-28 Image details | Details (EXIF) |
| 6 | SCR-27 Image viewer | Swipe down: back to folder |

### FL-07 Play video or audio (standalone)

Play a clip or a sound file.

Features: F-VWR-04 Media player

| # | Screen | Step |
|---|---|---|
| 1 | SCR-30 Media player (video, audio) | Audio |
| 2 | SCR-30 Media player (video, audio) | Audio controls |
| 3 | SCR-30 Media player (video, audio) | Video |
| 4 | SCR-30 Media player (video, audio) | Tap: controls |

### FL-08 Read text, code, Markdown (standalone)

Read a text file; see Markdown rendered or as source.

Features: F-VWR-05 Text viewer, F-VWR-06 Markdown viewer

| # | Screen | Step |
|---|---|---|
| 1 | SCR-31 Text viewer | Text (first 1 MB) |
| 2 | SCR-31 Text viewer | Code with syntax colours |
| 3 | SCR-32 Markdown viewer | Markdown rendered |
| 4 | SCR-32 Markdown viewer | Pull: View source, Share, Open with |
| 5 | SCR-31 Text viewer | Markdown source |

### FL-09 Read a PDF (standalone)

Read, jump via contents, search text.

Features: F-VWR-07 PDF viewer, F-VWR-08 PDF search, F-VWR-09 PDF contents / go to page, F-VWR-10 PDF password

| # | Screen | Step |
|---|---|---|
| 1 | SCR-33 PDF viewer | PDF viewer |
| 2 | SCR-33 PDF viewer | PDF viewer (from grid) |
| 3 | SCR-33 PDF viewer | Pull: Unlock, Open with, Share, Search, Contents and pages |
| 4 | SCR-34 PDF contents | Contents and pages: this PDF has no table of contents |
| 5 | SCR-33 PDF viewer | Search the text: hits |
| 6 | SCR-33 PDF viewer | PDF with a web link and an outline |
| 7 | SCR-36 PDF link | Tap web link: Link to a website, Open, Copy link |
| 8 | SCR-34 PDF contents | Contents: outline entries with page numbers, Go to page |
| 9 | SCR-33 PDF viewer | Tap outline entry: jumps to page 2 |
| 10 | SCR-35 PDF password | Locked PDF: password asked on open |
| 11 | SCR-35 PDF password | Wrong password. Try again. |
| 12 | SCR-33 PDF viewer | Unlocked |

## Organise

### FL-10 Create a folder (standalone)

Make an empty folder here.

Features: F-DIR-07 Folder states

| # | Screen | Step |
|---|---|---|
| 1 | SCR-06 Folder pulley | Pull: Add |
| 2 | SCR-12 Add | Add: New folder or Scan document |
| 3 | SCR-12 Add | New folder picked |
| 4 | SCR-13 New folder | Name it |
| 5 | SCR-13 New folder | New name typed |
| 6 | SCR-03 Folder page (list) | Folder made, scrolled into view, briefly highlighted |
| 7 | SCR-11 Folder states (empty, no access, error, offline, large, grid hint, archive banner) | Empty folder state |

### FL-11 Rename (standalone)

Rename an item, undo if wrong.

Features: F-ITM-04 Rename

| # | Screen | Step |
|---|---|---|
| 1 | SCR-07 Item context menu | Long-press: menu |
| 2 | SCR-14 Rename | Rename dialog |
| 3 | SCR-14 Rename | New name typed |
| 4 | SCR-03 Folder page (list) | Remorse: renamed, tap to undo |

### FL-12 Delete (standalone)

Delete to Trash with undo, or for good on volumes and servers.

Features: F-ITM-05 Delete, F-TRH-04 Trash settings

| # | Screen | Step |
|---|---|---|
| 1 | SCR-07 Item context menu | Long-press: Share, Copy, Cut, Rename, Delete, More actions |
| 2 | SCR-03 Folder page (list) | Remorse: Moving to trash |
| 3 | SCR-15 Delete permanently | Volume or server: Delete permanently? |

### FL-13 Copy or move with the clipboard (standalone)

Copy/Cut here, Paste there.

Features: F-ITM-03 Copy / Cut, F-OPS-01 Paste, F-OPS-07 Background notice

| # | Screen | Step |
|---|---|---|
| 1 | SCR-07 Item context menu | Long-press: Copy or Cut |
| 2 | SCR-10 Clipboard panel | Clipboard panel: Paste here, Clear |
| 3 | SCR-06 Folder pulley | Pull: Paste N items |
| 4 | SCR-10 Clipboard panel | Cut: panel |
| 5 | SCR-03 Folder page (list) | Target folder |
| 6 | SCR-03 Folder page (list) | Remorse: Moving … |
| 7 | SCR-03 Folder page (list) | Moved |

### FL-14 Large operation with conflicts (standalone)

Review plan, settle conflicts, start.

Features: F-OPS-02 Plan summary, F-OPS-03 Conflict resolution, F-OPS-04 Not-enough-space check

| # | Screen | Step |
|---|---|---|
| 1 | SCR-07 Item context menu | Long-press folder: Copy |
| 2 | SCR-10 Clipboard panel | Clipboard panel: 1 item copied |
| 3 | SCR-06 Folder pulley | In target folder, pull: Paste |
| 4 | SCR-19 Plan summary | Large paste: plan summary: items, size, free, conflicts, options |
| 5 | SCR-20 Conflict | Conflict: Merge, Keep both, Skip, Replace, … |
| 6 | SCR-20 Conflict | Picked Keep both |
| 7 | SCR-19 Plan summary | Summary after answer |
| 8 | SCR-23 Activity bar | Started: activity bar |
| 9 | SCR-03 Folder page (list) | Done |

### FL-15 Act on many items (standalone)

Select several items and act once.

Features: F-SEL-01 Selection mode, F-SEL-02 Drag-select, F-SEL-03 Select all / Clear selection, F-SEL-04 Selection bar

| # | Screen | Step |
|---|---|---|
| 1 | SCR-06 Folder pulley | Pull: Select |
| 2 | SCR-09 Selection mode + selection bar | Selection mode, nothing yet |
| 3 | SCR-09 Selection mode + selection bar | Two selected: bar Share, Copy, Cut, Delete, More, Stop |
| 4 | SCR-08 More actions | More for selection |
| 5 | SCR-09 Selection mode + selection bar | Pull: Select all, Clear selection |
| 6 | SCR-09 Selection mode + selection bar | Grid: long-press |
| 7 | SCR-09 Selection mode + selection bar | Grid: drag-select |

### FL-16 Compress (standalone)

Pack items into Zip or tar.gz.

Features: F-ITM-07 Compress, F-ITM-08 Move to new folder

| # | Screen | Step |
|---|---|---|
| 1 | SCR-07 Item context menu | Long-press file |
| 2 | SCR-08 More actions | More actions: Add to favourites, Compress, Move to new folder |
| 3 | SCR-16 Compress | Compress: name, format, Save to |

### FL-17 Open an archive and extract (standalone)

Look inside an archive, extract all.

Features: F-OPS-05 Extract, F-OPS-06 Archive location

| # | Screen | Step |
|---|---|---|
| 1 | SCR-11 Folder states (empty, no access, error, offline, large, grid hint, archive banner) | Archive as read-only folder, Extract all |
| 2 | SCR-17 Extract | Extract: contents, into, destination |
| 3 | SCR-17 Extract | Options |
| 4 | SCR-03 Folder page (list) | Extracted folder opens |

## Tasks

### FL-18 Watch and steer tasks (standalone)

See tasks, open details, pause, resume, retry.

Features: F-XFR-01 Activity page, F-XFR-02 Task row menu, F-XFR-03 Task details, F-XFR-04 Pause all / Resume all / Clear activity history, F-XFR-06 Activity bar, F-BRW-10 Activity from Browse

| # | Screen | Step |
|---|---|---|
| 1 | SCR-06 Folder pulley | Pull: Activity |
| 2 | SCR-21 Activity | Activity empty |
| 3 | SCR-21 Activity | Activity with tasks |
| 4 | SCR-22 Task details | Task details: done of total, destination, started, verify, items |
| 5 | SCR-21 Activity | Pull: Clear history, Resume all, Pause all |

## Find

### FL-19 Find a file (both modes)

Search this folder and below.

Features: F-SRC-01 Search in folder, F-SRC-02 Search filters, F-SRC-03 Server search depth

| # | Screen | Step |
|---|---|---|
| 1 | SCR-06 Folder pulley | Pull: Search |
| 2 | SCR-25 Search | Search page |
| 3 | SCR-25 Search | Results |
| 4 | SCR-26 Search filters | Filters |

### FL-20 Restore from Trash (standalone)

Get a deleted file back.

Features: F-TRH-01 Trash, F-TRH-02 Restore / Delete now / Show original folder, F-TRH-03 Empty trash

| # | Screen | Step |
|---|---|---|
| 1 | SCR-39 Trash | Trash |
| 2 | SCR-39 Trash | Menu: Restore, Delete now, Show original folder |
| 3 | SCR-39 Trash | Restored |
| 4 | SCR-39 Trash | Pull: Empty trash |

### FL-21 Reopen a recent file (standalone)

Open something used lately.

Features: F-RCT-01 Recents

| # | Screen | Step |
|---|---|---|
| 1 | SCR-38 Recents | Recents |
| 2 | SCR-38 Recents | Menu: Remove from recents |

### FL-22 Favourites (standalone)

Pin a folder, name and colour it, remove it.

Features: F-FAV-01 Favourites section, F-FAV-02 Edit favourite, F-FAV-03 Remove favourite, F-FAV-04 Reorder favourites, F-ITM-06 Add to favourites

| # | Screen | Step |
|---|---|---|
| 1 | SCR-05 Path menu | Path menu: Add to favourites |
| 2 | SCR-01 Browse | Favourite menu: Edit, Copy address, Remove |
| 3 | SCR-40 Edit favourite | Edit favourite: label, colour |
| 4 | SCR-40 Edit favourite | Icon |

## Create

### FL-23 Scan a document (standalone)

Camera pages into one PDF.

Features: F-SCN-01 Scan document, F-SCN-02 Page edge detection and crop, F-SCN-03 Page view, F-SCN-04 Save as PDF

| # | Screen | Step |
|---|---|---|
| 1 | SCR-12 Add | Pull Add: New folder, Scan document |
| 2 | SCR-53 Scan camera | Camera page |
| 3 | SCR-53 Scan camera | Shutter: page flies into pages button, count |
| 4 | SCR-54 Scan page view | Pages button: page view, n of N, colour, adjust, turn, retake, remove |
| 5 | SCR-54 Scan page view | Grayscale |
| 6 | SCR-55 Scan adjust (crop, turn) | Adjust: crop corners, turn |
| 7 | SCR-53 Scan camera | Back to camera with 2 pages |
| 8 | SCR-56 Scan save | Done: Save 2 pages as PDF, name, page strip |
| 9 | SCR-57 Scan saving | Saving |
| 10 | SCR-03 Folder page (list) | Saved PDF in folder |

## Share

### FL-24 Share files out (standalone)

Send files to another app.

Features: F-ITM-02 Share (out), F-ITM-09 Open with, F-SRV-16 Open remote in other app

| # | Screen | Step |
|---|---|---|
| 1 | SCR-07 Item context menu | Long-press (server file): Share, Copy, Cut, Rename, Delete, More actions |
| 2 | SCR-09 Selection mode + selection bar | Server folder, long-press: selection bar, Share first |
| 3 | SCR-37 Open remote (Getting file) | Remote file: Getting … first |
| 4 | SCR-07 Item context menu | Sailfish share sheet |

### FL-25 Save files shared to Lautta (standalone)

Pick Lautta in a share sheet, choose a folder.

Features: F-SHR-01 Share target

| # | Screen | Step |
|---|---|---|
| 1 | SCR-52 Share target ("Save to") | Save to: suggested folders |
| 2 | SCR-18 Folder picker | Another folder…: folder picker |
| 3 | SCR-03 Folder page (list) | Saved |

## Settings

### FL-26 Settings (standalone)

Tune Lautta.

Features: F-APP-03 Settings, F-APP-04 Storage, F-APP-05 About, F-BRW-07 Browse layout, F-RCT-02 Recents off, F-TRH-04 Trash settings, F-XFR-08 Concurrency and lanes, F-XFR-11 Resume on start / keep history, F-SRC-03 Server search depth, F-SRV-05 Add server, F-FAV-04 Reorder favourites, F-DIR-10 Listing cache

| # | Screen | Step |
|---|---|---|
| 1 | SCR-01 Browse | Pull: Settings, Add server, Activity |
| 2 | SCR-41 Settings | Settings |
| 3 | SCR-42 Settings › Browse page | Browse page: hide, reorder |
| 4 | SCR-24 View options | View options |
| 5 | SCR-43 Settings › Search | Search |
| 6 | SCR-44 Settings › Recents | Recents |
| 7 | SCR-45 Settings › Deleting | Deleting |
| 8 | SCR-46 Settings › Transfers | Transfers |
| 9 | SCR-47 Settings › Storage | Storage |
| 10 | SCR-48 About | About |
| 11 | SCR-48 About | About, standalone |
| 12 | SCR-01 Browse | Standalone pulley: Settings, Activity |

## Locations

### FL-27 Memory cards and USB drives (both modes)

Browse, mount, unmount, unlock.

Features: F-BRW-04 Volumes, F-BRW-05 Mount / Unmount drive, F-BRW-06 Locked drive → Storage settings

| # | Screen | Step |
|---|---|---|
| 1 | SCR-01 Browse | Mounted volume menu: Add to favourites, Copy address, Unmount, Open Storage settings |
| 2 | SCR-01 Browse | Unmounted: Mount |
| 3 | SCR-01 Browse | Mounted |

## Servers

### FL-28 Allow network locations (bridge)

Give Lautta netvfs access.

Features: F-SRV-02 Consent banner, F-SRV-01 Bridge detection

| # | Screen | Step |
|---|---|---|
| 1 | SCR-01 Browse | Servers section: Network locations need your permission, Ask again |
| 2 | SCR-59 Notifications | Events: netvfs asks which servers Lautta may use |
| 3 | SCR-59 Notifications | Tap note: netvfs dialog, level per server |
| 4 | SCR-48 About | About: Waiting for your permission |
| 5 | SCR-01 Browse | Allowed: servers listed |

### FL-29 Open a saved server (bridge)

Browse an account server.

Features: F-SRV-04 Account servers, F-SRV-06 Edit server / Update sign-in / Review server identity, F-SRV-14 Remote thumbnails

| # | Screen | Step |
|---|---|---|
| 1 | SCR-01 Browse | Servers section |
| 2 | SCR-01 Browse | Server row menu: Edit server, Copy address |
| 3 | SCR-01 Browse | SMB server menu |
| 4 | SCR-03 Folder page (list) | Server root |
| 5 | SCR-03 Folder page (list) | Server folder |

### FL-30 Fix a server that needs you (bridge)

Sign-in failed or identity changed: go fix in Settings.

Features: F-SRV-07 Server attention page, F-SRV-06 Edit server / Update sign-in / Review server identity

| # | Screen | Step |
|---|---|---|
| 1 | SCR-03 Folder page (list) | Opening: identity changed |
| 2 | SCR-01 Browse | Row shows attention |
| 3 | SCR-51 Server attention | Server attention page |
| 4 | SCR-01 Browse | Menu: Review server identity |
| 5 | SCR-51 Server attention | Sign-in failed (no stored password) |

### FL-32 Work with files on a server (bridge)

View, stream, upload.

Features: F-SRV-14 Remote thumbnails, F-SRV-15 Remote streaming, F-SRV-16 Open remote in other app, F-XFR-12 Item status in folder

| # | Screen | Step |
|---|---|---|
| 1 | SCR-04 Folder page (grid) | Server folder: photos as grid, thumbnails |
| 2 | SCR-27 Image viewer | Remote image |
| 3 | SCR-30 Media player (video, audio) | Video streams from server |
| 4 | SCR-10 Clipboard panel | Clipboard from this device, on server |
| 5 | SCR-06 Folder pulley | Pull: Paste 1 item |

### FL-33 Server goes away (bridge)

Keep reading cached listing; see state.

Features: F-SRV-12 Offline listing, F-SRV-13 Reconnect

| # | Screen | Step |
|---|---|---|
| 1 | SCR-11 Folder states (empty, no access, error, offline, large, grid hint, archive banner) | Offline banner: stored copy, Retry |
| 2 | SCR-01 Browse | Server row after drop |

### FL-36 Set up a nearby server (bridge)

Server found on the network becomes an account.

Features: F-SRV-11 Nearby, F-SRV-05 Add server

| # | Screen | Step |
|---|---|---|
| 1 | SCR-01 Browse | Nearby: server found on the network |
| 2 | SCR-60 Nearby setup | Tap: This server isn't set up yet …, Set up |
| 3 | SCR-60 Nearby setup | Set up: Settings › Accounts opens its add-account flow |

### FL-37 Work on a server with limited access (bridge)

See what Lautta may do on a server and get only those actions.

Features: F-SRV-17 Access levels, F-DIR-07 Folder states, F-OPS-03 Conflict resolution

| # | Screen | Step |
|---|---|---|
| 1 | SCR-03 Folder page (list) | Read only: lock and Read only under header |
| 2 | SCR-06 Folder pulley | Read only: pulley has no Paste, no Add |
| 3 | SCR-07 Item context menu | Read and add: long-press has no Cut, Rename, Delete |
| 4 | SCR-11 Folder states (empty, no access, error, offline, large, grid hint, archive banner) | Add only: folders listed, Files here are hidden |
| 5 | SCR-11 Folder states (empty, no access, error, offline, large, grid hint, archive banner) | Paste a file whose name is taken: no question, upload done, server keeps both (readme 2.txt) |
| 6 | SCR-03 Folder page (list) | Full access: 1 GB upload running |
| 7 | SCR-03 Folder page (list) | Level narrowed to Read only meanwhile: lock shows, upload Failed |
| 8 | SCR-21 Activity | Activity: upload Failed, Retry failed |

## System

### FL-34 App in background (standalone)

See state on the cover.

Features: F-APP-01 Cover, F-XFR-10 Transfer cover, F-XFR-09 Notifications

| # | Screen | Step |
|---|---|---|
| 1 | SCR-58 Cover (folder / tasks) | Cover on Browse page: Lautta |
| 2 | SCR-58 Cover (folder / tasks) | Cover in a folder: name, item count |
| 3 | SCR-58 Cover (folder / tasks) | Task running: count, percent, rate; actions Pause all, Activity |
| 4 | SCR-21 Activity | Activity pulley: Pause all |
| 5 | SCR-58 Cover (folder / tasks) | Tasks paused: n paused, action Resume all |
| 6 | SCR-21 Activity | Tap cover: back in Activity |
| 7 | SCR-58 Cover (folder / tasks) | Long-press Home: close Lautta while a task runs |
| 8 | SCR-59 Notifications | Closed with a task running: pop-up "1 task stopped", resumes on next open |

### FL-35 Follow tasks from Events (standalone)

Leave Lautta while tasks run; get told when they need you.

Features: F-XFR-09 Notifications, F-XFR-10 Transfer cover, F-XFR-05 Restored tasks, F-APP-01 Cover

| # | Screen | Step |
|---|---|---|
| 1 | SCR-23 Activity bar | Start a copy, leave: Running in the background |
| 2 | SCR-59 Notifications | Events: n task · percent · time left, progress bar |
| 3 | SCR-59 Notifications | Finished in background: pop-up and Events, Tasks finished |
| 4 | SCR-59 Notifications | Conflict in background: Lautta needs an answer, n conflict in task |
| 5 | SCR-20 Conflict | Tap note: Conflict |
| 6 | SCR-59 Notifications | Item failed: task failed, n items failed. Tap to retry. |
| 7 | SCR-59 Notifications | Closed with tasks: n tasks stopped |
