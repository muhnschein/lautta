# Lautta feature ↔ flow ↔ screen

Which flows (`doc/flows.md`) reach each feature and screen (`doc/feature-map.md`).

## Features

| Feature | Mode | Flows | Screens |
|---|---|---|---|
| F-BRW-01 Browse start page | S | FL-01 | SCR-01, SCR-02 |
| F-BRW-02 User folders | S | FL-01, FL-02 | SCR-01, SCR-02, SCR-03 |
| F-BRW-03 Android folders | S | FL-01 | SCR-01, SCR-02 |
| F-BRW-04 Volumes | S | FL-01, FL-27 | SCR-01, SCR-02 |
| F-BRW-05 Mount / Unmount drive | B | FL-27 | SCR-01 |
| F-BRW-06 Locked drive → Storage settings | B | FL-27 | SCR-01 |
| F-BRW-07 Browse layout | S | FL-26 | SCR-01, SCR-24, SCR-41, SCR-42, SCR-43, SCR-44, SCR-45, SCR-46, SCR-47, SCR-48 |
| F-BRW-09 Rescan volumes | S | — | — |
| F-BRW-10 Activity from Browse | S | FL-18 | SCR-06, SCR-21, SCR-22 |
| F-BRW-11 Phone free space | S | FL-21 | SCR-01, SCR-38 |
| F-SRV-01 Bridge detection | B | FL-01, FL-28 | SCR-01, SCR-02, SCR-48, SCR-59 |
| F-SRV-02 Consent banner | B | FL-28 | SCR-01, SCR-48, SCR-59 |
| F-SRV-03 Bridge unavailable banner | B | — | — |
| F-SRV-04 Account servers | B | FL-29 | SCR-01, SCR-03 |
| F-SRV-05 Add server | B | FL-26, FL-36 | SCR-01, SCR-24, SCR-41, SCR-42, SCR-43, SCR-44, SCR-45, SCR-46, SCR-47, SCR-48, SCR-60 |
| F-SRV-06 Edit server / Update sign-in / Review server identity | B | FL-29, FL-30 | SCR-01, SCR-03, SCR-51 |
| F-SRV-07 Server attention page | B | FL-30 | SCR-01, SCR-03, SCR-51 |
| F-SRV-11 Nearby | B | FL-36 | SCR-01, SCR-60 |
| F-SRV-12 Offline listing | B | FL-33 | SCR-01, SCR-11 |
| F-SRV-13 Reconnect | B | FL-33 | SCR-01, SCR-11 |
| F-SRV-14 Remote thumbnails | B | FL-29, FL-32 | SCR-01, SCR-03, SCR-04, SCR-06, SCR-10, SCR-27, SCR-30 |
| F-SRV-15 Remote streaming | B | FL-32 | SCR-04, SCR-06, SCR-10, SCR-27, SCR-30 |
| F-SRV-16 Open remote in other app | B | FL-24, FL-32 | SCR-04, SCR-06, SCR-07, SCR-09, SCR-10, SCR-27, SCR-30, SCR-37 |
| F-SRV-17 Access levels | B | FL-37 | SCR-03, SCR-06, SCR-07, SCR-11, SCR-21 |
| F-DIR-01 Folder listing | S | FL-02 | SCR-01, SCR-03 |
| F-DIR-02 Grid view | S | FL-04 | SCR-04, SCR-06, SCR-24 |
| F-DIR-03 Grid hint | S | FL-04 | SCR-04, SCR-06, SCR-24 |
| F-DIR-04 Sort and filter | S | FL-04 | SCR-04, SCR-06, SCR-24 |
| F-DIR-05 Thumbnails switch | S | FL-04 | SCR-04, SCR-06, SCR-24 |
| F-DIR-06 Path crumbs | S | FL-03 | SCR-01, SCR-03, SCR-05 |
| F-DIR-07 Folder states | S | FL-10, FL-37 | SCR-03, SCR-06, SCR-07, SCR-11, SCR-12, SCR-13, SCR-21 |
| F-DIR-08 Large folder | S | FL-05 | SCR-11 |
| F-DIR-09 Live updates | S | — | — |
| F-DIR-10 Listing cache | S | FL-26 | SCR-01, SCR-24, SCR-41, SCR-42, SCR-43, SCR-44, SCR-45, SCR-46, SCR-47, SCR-48 |
| F-DIR-11 Folder cover | S | FL-02 | SCR-01, SCR-03 |
| F-ITM-01 Open item | S | FL-02 | SCR-01, SCR-03 |
| F-ITM-02 Share (out) | S | FL-06, FL-24 | SCR-03, SCR-07, SCR-09, SCR-27, SCR-28, SCR-37 |
| F-ITM-03 Copy / Cut | S | FL-13 | SCR-03, SCR-06, SCR-07, SCR-10 |
| F-ITM-04 Rename | S | FL-11 | SCR-03, SCR-07, SCR-14 |
| F-ITM-05 Delete | S | FL-06, FL-12 | SCR-03, SCR-07, SCR-15, SCR-27, SCR-28 |
| F-ITM-06 Add to favourites | S | FL-22 | SCR-01, SCR-06, SCR-40 |
| F-ITM-07 Compress | S | FL-16 | SCR-03, SCR-07, SCR-08, SCR-16 |
| F-ITM-08 Move to new folder | S | FL-16 | SCR-03, SCR-07, SCR-08, SCR-16 |
| F-ITM-09 Open with | S | FL-06, FL-24 | SCR-03, SCR-07, SCR-09, SCR-27, SCR-28, SCR-37 |
| F-ITM-10 Checksums | S | FL-38 | SCR-07, SCR-08, SCR-61, SCR-63, SCR-64 |
| F-ITM-11 Verify files | S | FL-38 | SCR-07, SCR-08, SCR-61, SCR-63, SCR-64 |
| F-SEL-01 Selection mode | S | FL-15 | SCR-06, SCR-08, SCR-09 |
| F-SEL-02 Drag-select | S | FL-15 | SCR-06, SCR-08, SCR-09 |
| F-SEL-03 Select all / Clear selection | S | FL-15 | SCR-06, SCR-08, SCR-09 |
| F-SEL-04 Selection bar | S | FL-15 | SCR-06, SCR-08, SCR-09 |
| F-OPS-01 Paste | S | FL-13 | SCR-03, SCR-06, SCR-07, SCR-10 |
| F-OPS-02 Plan summary | S | FL-14 | SCR-03, SCR-06, SCR-07, SCR-10, SCR-19, SCR-20, SCR-23 |
| F-OPS-03 Conflict resolution | S | FL-14, FL-37 | SCR-03, SCR-06, SCR-07, SCR-10, SCR-11, SCR-19, SCR-20, SCR-21, SCR-23 |
| F-OPS-04 Not-enough-space check | S | FL-14 | SCR-03, SCR-06, SCR-07, SCR-10, SCR-19, SCR-20, SCR-23 |
| F-OPS-05 Extract | S | FL-17 | SCR-03, SCR-06, SCR-08, SCR-35 |
| F-OPS-06 Archive location | S | FL-17 | SCR-03, SCR-06, SCR-08, SCR-35 |
| F-OPS-07 Background notice | S | FL-13 | SCR-03, SCR-06, SCR-07, SCR-10 |
| F-XFR-01 Activity page | S | FL-18 | SCR-06, SCR-21, SCR-22 |
| F-XFR-02 Task row menu | S | FL-18 | SCR-06, SCR-21, SCR-22 |
| F-XFR-03 Task details | S | FL-18 | SCR-06, SCR-21, SCR-22 |
| F-XFR-04 Pause all / Resume all / Clear activity history | S | FL-18 | SCR-06, SCR-21, SCR-22 |
| F-XFR-05 Restored tasks | S | FL-35 | SCR-20, SCR-23, SCR-59 |
| F-XFR-06 Activity bar | S | FL-18 | SCR-06, SCR-21, SCR-22 |
| F-XFR-07 Waiting states | S | — | — |
| F-XFR-08 Concurrency and lanes | S | FL-26 | SCR-01, SCR-24, SCR-41, SCR-42, SCR-43, SCR-44, SCR-45, SCR-46, SCR-47, SCR-48 |
| F-XFR-09 Notifications | S | FL-34, FL-35 | SCR-20, SCR-21, SCR-23, SCR-58, SCR-59 |
| F-XFR-10 Transfer cover | S | FL-34, FL-35 | SCR-20, SCR-21, SCR-23, SCR-58, SCR-59 |
| F-XFR-11 Resume on start / keep history | S | FL-26 | SCR-01, SCR-24, SCR-41, SCR-42, SCR-43, SCR-44, SCR-45, SCR-46, SCR-47, SCR-48 |
| F-XFR-12 Item status in folder | S | FL-32 | SCR-04, SCR-06, SCR-10, SCR-27, SCR-30 |
| F-VWR-01 Image viewer | S | FL-06 | SCR-03, SCR-27, SCR-28 |
| F-VWR-02 Image details | S | FL-06 | SCR-03, SCR-27, SCR-28 |
| F-VWR-03 Image More | S | FL-06 | SCR-03, SCR-27, SCR-28 |
| F-VWR-04 Media player | S | FL-07 | SCR-30 |
| F-VWR-05 Text viewer | S | FL-08 | SCR-31, SCR-32 |
| F-VWR-06 Markdown viewer | S | FL-08 | SCR-31, SCR-32 |
| F-VWR-07 PDF viewer | S | FL-09 | SCR-33, SCR-34, SCR-35, SCR-36 |
| F-VWR-08 PDF search | S | FL-09 | SCR-33, SCR-34, SCR-35, SCR-36 |
| F-VWR-09 PDF contents / go to page | S | FL-09 | SCR-33, SCR-34, SCR-35, SCR-36 |
| F-VWR-10 PDF password | S | FL-09 | SCR-33, SCR-34, SCR-35, SCR-36 |
| F-VWR-11 External fallback | S | — | — |
| F-SRC-01 Search in folder | S | FL-19 | SCR-06, SCR-25, SCR-26 |
| F-SRC-02 Search filters | S | FL-19 | SCR-06, SCR-25, SCR-26 |
| F-SRC-03 Server search depth | S | FL-19, FL-26 | SCR-01, SCR-06, SCR-24, SCR-25, SCR-26, SCR-41, SCR-42, SCR-43, SCR-44, SCR-45, SCR-46, SCR-47, SCR-48 |
| F-SCN-01 Scan document | S | FL-23 | SCR-03, SCR-12, SCR-53, SCR-54, SCR-55, SCR-56, SCR-57 |
| F-SCN-02 Page edge detection and crop | S | FL-23 | SCR-03, SCR-12, SCR-53, SCR-54, SCR-55, SCR-56, SCR-57 |
| F-SCN-03 Page view | S | FL-23 | SCR-03, SCR-12, SCR-53, SCR-54, SCR-55, SCR-56, SCR-57 |
| F-SCN-04 Save as PDF | S | FL-23 | SCR-03, SCR-12, SCR-53, SCR-54, SCR-55, SCR-56, SCR-57 |
| F-FAV-01 Favourites section | S | FL-01, FL-22 | SCR-01, SCR-02, SCR-06, SCR-40 |
| F-FAV-02 Edit favourite | S | FL-22 | SCR-01, SCR-06, SCR-40 |
| F-FAV-03 Remove favourite | S | FL-22 | SCR-01, SCR-06, SCR-40 |
| F-FAV-04 Reorder favourites | S | FL-22, FL-26 | SCR-01, SCR-06, SCR-24, SCR-40, SCR-41, SCR-42, SCR-43, SCR-44, SCR-45, SCR-46, SCR-47, SCR-48 |
| F-RCT-01 Recents | S | FL-21 | SCR-01, SCR-38 |
| F-RCT-02 Recents off | S | FL-26 | SCR-01, SCR-24, SCR-41, SCR-42, SCR-43, SCR-44, SCR-45, SCR-46, SCR-47, SCR-48 |
| F-TRH-01 Trash | S | FL-20 | SCR-39 |
| F-TRH-02 Restore / Delete now / Show original folder | S | FL-20 | SCR-39 |
| F-TRH-03 Empty trash | S | FL-20 | SCR-39 |
| F-TRH-04 Trash settings | S | FL-12, FL-26 | SCR-01, SCR-03, SCR-07, SCR-15, SCR-24, SCR-41, SCR-42, SCR-43, SCR-44, SCR-45, SCR-46, SCR-47, SCR-48 |
| F-SHR-01 Share target | S | FL-25 | SCR-03, SCR-13, SCR-18, SCR-52 |
| F-APP-01 Cover | S | FL-34, FL-35 | SCR-20, SCR-21, SCR-23, SCR-58, SCR-59 |
| F-APP-02 Start error | S | FL-01 | SCR-01, SCR-02 |
| F-APP-03 Settings | S | FL-26 | SCR-01, SCR-24, SCR-41, SCR-42, SCR-43, SCR-44, SCR-45, SCR-46, SCR-47, SCR-48 |
| F-APP-04 Storage | S | FL-26 | SCR-01, SCR-24, SCR-41, SCR-42, SCR-43, SCR-44, SCR-45, SCR-46, SCR-47, SCR-48 |
| F-APP-05 About | S | FL-26 | SCR-01, SCR-24, SCR-41, SCR-42, SCR-43, SCR-44, SCR-45, SCR-46, SCR-47, SCR-48 |
| F-APP-06 Background notices | S | — | — |

## Screens

| Screen | Flows |
|---|---|
| SCR-01 Browse | FL-01, FL-02, FL-03, FL-21, FL-22, FL-26, FL-27, FL-28, FL-29, FL-30, FL-33, FL-36 |
| SCR-02 Start error | FL-01 |
| SCR-03 Folder page (list) | FL-02, FL-03, FL-06, FL-10, FL-11, FL-12, FL-13, FL-14, FL-16, FL-17, FL-23, FL-25, FL-29, FL-30, FL-37 |
| SCR-04 Folder page (grid) | FL-04, FL-32 |
| SCR-05 Folded path | FL-03 |
| SCR-06 Folder pulley | FL-04, FL-10, FL-13, FL-14, FL-15, FL-17, FL-18, FL-19, FL-22, FL-32, FL-37 |
| SCR-07 Item context menu | FL-11, FL-12, FL-13, FL-14, FL-16, FL-24, FL-37, FL-38 |
| SCR-08 More actions | FL-15, FL-16, FL-17, FL-38 |
| SCR-09 Selection mode + selection bar | FL-15, FL-24 |
| SCR-10 Clipboard panel | FL-13, FL-14, FL-32 |
| SCR-11 Folder states (empty, no access, error, offline, large, grid hint) | FL-05, FL-10, FL-33, FL-37 |
| SCR-12 Add | FL-10, FL-23 |
| SCR-13 New folder | FL-10, FL-25 |
| SCR-14 Rename | FL-11 |
| SCR-15 Delete permanently | FL-12 |
| SCR-16 Compress | FL-16 |
| SCR-18 Share target: Locations | FL-25 |
| SCR-19 Plan summary | FL-14 |
| SCR-20 Conflict | FL-14, FL-35 |
| SCR-21 Activity | FL-18, FL-34, FL-37 |
| SCR-22 Task details | FL-18 |
| SCR-23 Activity bar | FL-14, FL-35 |
| SCR-24 View options | FL-04, FL-26 |
| SCR-25 Search | FL-19 |
| SCR-26 Search filters | FL-19 |
| SCR-27 Image viewer | FL-06, FL-32 |
| SCR-28 Image details | FL-06 |
| SCR-29 Image More | — |
| SCR-30 Media player (video, audio) | FL-07, FL-32 |
| SCR-31 Text viewer | FL-08 |
| SCR-32 Markdown viewer | FL-08 |
| SCR-33 PDF viewer | FL-09 |
| SCR-34 PDF contents | FL-09 |
| SCR-35 Locked file | FL-09, FL-17 |
| SCR-36 PDF link | FL-09 |
| SCR-37 Open remote (Getting file) | FL-24 |
| SCR-38 Recents | FL-21 |
| SCR-39 Trash | FL-20 |
| SCR-40 Edit favourite | FL-22 |
| SCR-41 Settings | FL-26 |
| SCR-42 Settings › Browse page | FL-26 |
| SCR-43 Settings › Search | FL-26 |
| SCR-44 Settings › Recents | FL-26 |
| SCR-45 Settings › Deleting | FL-26 |
| SCR-46 Settings › Transfers | FL-26 |
| SCR-47 Settings › Storage | FL-26 |
| SCR-48 About | FL-26, FL-28 |
| SCR-51 Server attention | FL-30 |
| SCR-52 Share target ("Save here") | FL-25 |
| SCR-53 Scan camera | FL-23 |
| SCR-54 Scan page view | FL-23 |
| SCR-55 Scan adjust (crop, turn) | FL-23 |
| SCR-56 Scan save | FL-23 |
| SCR-57 Scan saving | FL-23 |
| SCR-58 Cover (page / tasks) | FL-34 |
| SCR-59 Notifications | FL-28, FL-34, FL-35 |
| SCR-60 Nearby setup | FL-36 |
| SCR-61 Checksums | FL-38 |
| SCR-63 Checksum | FL-38 |
| SCR-64 Verify files | FL-38 |

Features in no flow: background behaviour, nothing to tap (F-BRW-09, F-SRV-03, F-DIR-09, F-XFR-07, F-VWR-11, F-APP-06).
Screens in no flow: reached only from states flows don't set up (SCR-29).
