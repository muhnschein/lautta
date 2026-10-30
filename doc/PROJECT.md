# Lautta

*File manager for Sailfish OS, for Jolla's Harbour store.*

## What this is

- Usable store app today: user's folders, SD cards, USB drives, archives, Trash. Browse, preview, open, share, copy, move.
- Showcase for platform file access: with netvfs installed, SFTP and SMB servers from Settings → Accounts become ordinary locations, plus persistent, resumable transfer queue.

## What this isn't

- **No network code.** Remote access only through netvfs bridge. App has no `Internet` permission.
- **No paths handed to netvfs.** App opens local files itself, passes file descriptors.
- **No helpers, daemons, background work.** One sandboxed process; unfinished transfers resume at next start.
- **No root mode, account setup, share links, cloud OAuth.** Accounts live in Settings.
- **No other store, no other architecture.** Harbour, aarch64, Sailfish OS 5.2+.

## Two modes

| | Standalone (Harbour default) | With netvfs bridge |
|---|---|---|
| Locations | user folders, Android folders, removable media, archives | + netvfs accounts with *Files* on, ad-hoc servers, Nearby |
| Network | none | none in app; bridge does it |
| Credentials | none | saved accounts: never seen by app; ad-hoc secrets passed once, wiped |
| UI | no *Servers*, no *Connect to server*, no trace of bridge | full |
| Detection | bridge socket absent | socket present, handshake succeeds |

Standalone is what Harbour reviewers and most users see: must be useful and polished alone. Store text names network locations neutrally ("works with network locations provided by netvfs, if installed"), never as main feature.

### With netvfs

- Discovery: socket `~/.local/share/org.netvfs/lautta/netvfs/bridge.sock`, checked at start, back in foreground, on inotify create. Absent → standalone, no message. App never creates it.
- Connection: p2p D-Bus (`zbus`, unix socket, fd passing), `org.netvfs.Bridge1`. `Hello` first, then `GetFeatures`: optional parts (`storage-settings`, `drives`) offered only while bridge names them; unknown names ignored.
- Consent: until `granted`, *Servers* shows one row asking to allow access in netvfs's notification, with *Ask again*. `denied` hides section, reason in *About*.
- Accounts handed off: *Add server*, *Edit server*, *Update sign-in*, *Review server identity* open Settings via bridge. App never edits accounts, pins, secrets; never opens or closes connections.
- Ad-hoc servers: *Connect to server* takes `sftp://` or `smb://` URL and secret if needed; buffer zeroized after call. Identity prompts and unencrypted-connection consent arrive as `Question` signals, answered in app (for accounts: in Settings).
- Server row status: one dot, no text, no protocol or host. Connected = highlight; not connected = primary; connecting = pulsing (only on screen, app in front); failed = red; sign-in or identity needs user = orange. Not connected stays browsable; opening connects.
- Names and paths cross as bytes (`ay`): netvfs names may be non-UTF-8, D-Bus strings may not.
- Local data never by path: uploads pass read-only fd, downloads a writable fd to app-created temp file, archive streams a pipe end.
- Every request carries lane hint (`interactive`, `bulk`, `stream`); bridge owns connection pools.
- Jobs report progress (≤ 4 Hz) and finish. Intent persisted in queue before start: crash or bridge restart never loses transfer. Temp names `.<name>.lautta-<shortid>.part`, final `Rename` separate.
- Bridge lost → server rows show connecting, reconnect with backoff 1, 2, 4, 8, then 15 s while in foreground; running transfers wait, then resume.

## Architecture

```
Silica QML
        |  models, facades, image providers
lautta-qt (qmetaobject-rs; no logic)
        |
lautta-core (no Qt): providers (local, archive, netvfs),
planner, transfer queue, caches, SQLite
        |  p2p D-Bus + fd passing, unix socket
netvfs-bridge (not part of this app) -> SFTP / SMB servers
```

- QObjects on GUI thread only; core answers via queued callbacks.
- Providers behind one trait; UI and planner see `Capabilities`, never provider names.
- Blocking file system calls on `spawn_blocking`, so slow SD card never stalls runtime.

## Platform

- Sailfish OS 5.2+, aarch64, Qt 5.6, Silica. Rust 1.95, cross-built against 5.2 SDK.
- Harbour validator is authority; CI runs it on every RPM. Rules the design leans on:
  - Files only under `/usr/bin/harbour-lautta`, `/usr/share/harbour-lautta/`, desktop file, icons.
  - Links only: libc, libm, libpthread, libdl, librt, libgcc_s, libstdc++, Qt5 Core/Gui/Qml/Quick/Multimedia, libsailfishapp, libsqlite3, libz, liblzma, libbz2. Rest static Rust, no RPATH.
  - QML imports only from Harbour's allowed list; private modules `Lautta.*`.
  - `X-Nemo-Application-Type=no-invoker`: bridge serves only peer running registered `/usr/bin/harbour-lautta`, booster would be another binary. Validator warning accepted in `harbour-warnings.md`.

## Sandbox

```ini
[X-Sailjail]
OrganizationName=org.netvfs
ApplicationName=lautta
Permissions=UserDirs;RemovableMedia;Audio
ExecDBus=harbour-lautta
```

| Permission | Why | Grants |
|---|---|---|
| `UserDirs` | user's files, their thumbnails | `~/Documents`, `~/Downloads`, `~/Pictures`, `~/Music`, `~/Playlists`, `~/Videos`, `~/Public`, `~/android_storage/{Documents,Download,Pictures,DCIM,Music,Podcasts,Movies}`; via `Pictures`/`Videos` also `Thumbnails`: `thumbnaild-pdf`, `thumbnaild-video`, `~/.cache/org.nemomobile/thumbnails`, D-Bus `org.nemomobile.Thumbnailer` |
| `RemovableMedia` | SD cards, USB drives | `/run/media`, UDisks2 signals |
| `Audio` | sound in audio/video preview | PulseAudio, GStreamer cache |

Not requested, on purpose: `Internet` (app opens no sockets; netvfs does), `Accounts`, `Secrets` (credentials stay in netvfs), `MediaIndexing`, `Compatibility`.

Implicit: `~/.local/share/org.netvfs/lautta`, `~/.cache/org.netvfs/lautta`, `~/.config/org.netvfs/lautta`, D-Bus name `org.netvfs.lautta` (notification actions, `ExecDBus` activation).

File system seen (firejail `whitelist`): tmpfs on `$HOME`, each granted folder and app's own folders bind-mounted one by one. No folder on `$HOME`'s device: `rename(2)` between two fails `EXDEV` even on one ext4, so moves between folders, and into Trash, are copies. `/run/media` is host's, but sandbox has own mount namespace: inotify on it can miss a mount, so volumes are also rescanned.

## Pages

```
Browse ─┬─ Directory ─┬─ Directory …
        │             ├─ Viewers
        │             ├─ Folder picker
        │             └─ Search results
        ├─ Recents
        ├─ Trash
        ├─ Activity ── Transfer details
        ├─ Connect to server (nearby, recent)
        └─ Settings / About
```

## Performance budgets

Reference: current Jolla phone, aarch64.

| Budget |
|---|
| Cached folder visible ≤ 100 ms after tap; local folder of 1 000 entries ≤ 150 ms |
| First remote batch rendered ≤ 3 × RTT + 100 ms on warm bridge connection |
| No frame > 32 ms scrolling 20 000-entry list (thumbnails off) |
| RSS ≤ 150 MB with 20 000-entry folder open |
| Launch to Browse ≤ 1.0 s warm, ≤ 2.0 s cold |
| Local copy ≥ 90 % of `cp` throughput on same storage |
| Browsing latency within ± 20 % while 2 bulk transfers run |
| 0 % CPU, no timers while idle in background |

Remote throughput is netvfs's budget; app must not be bottleneck (fd passing, no copies through app for local↔remote).

## Roadmap

| Milestone | Scope | Exit criteria |
|---|---|---|
| M0 skeleton | workspace, SailfishApp bootstrap from Rust, rpmvalidator in CI, Browse with user folders | validator clean; launches on device |
| M1 local MVP | directory page, sort/filter, cache, file operations, clipboard, folder picker, Trash, image/text viewers, local thumbnails, local transfers, cover | Harbour submission of standalone 1.0 candidate; folder, scroll, launch budgets met |
| M2 bridge | netvfs provider, consent, accounts and ad-hoc servers, remote browse and operations, local↔remote transfers with resume, remote thumbnails | contract tests with netvfs green; device test with SFTP and SMB |
| M3 depth | streaming, share target, search, archives, Nearby | parity list below done |
| M4 power | server copy wherever available | — |

Standalone releases ship to Harbour from M1; bridge features ship only once the netvfs bridge they need is released.

### Parity

- Harbour File Browser (in sandbox): browse ✓, search ✓, share/open externally ✓, previews of images, audio, video, archives, PDFs ✓, multi-select ✓, bulk copy/move ✓, new folders ✓, hidden files ✓, edit/enter paths ✓, quick filter ✓, shortcuts ✓. Not: database previews (*Open with*), links, bulk rename, permissions, new empty files, per-folder view settings, multiple windows, root mode.
- Transmit/Cyberduck (bridge): saved servers ✓ (Settings), quick connect ✓, dual-location copy ✓, queue with concurrency ✓, resume ✓, server-side copy ✓, preserve timestamps ✓, Bonjour discovery ✓. Not: external edit with auto-upload, sync/compare, recursive permissions.
- iOS Files: Browse/Recents ✓, Favourites ✓, Connect to server ✓, quick look ✓, Recently deleted ✓ (Trash), compress/uncompress ✓. Not: Tags.

## Words on screen

- `qsTrId()` with `//%` engineering English; counts as one `%n` plural.
- Errors name thing and place, offer next step: "Not enough space on *Destination* (needs 4.1 GB, 1.2 GB free)." → *Choose another folder*. Same text for every location kind.
