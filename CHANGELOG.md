# Changelog

## Unreleased

- Videos from servers stream and seek instead of downloading first ([#100](https://github.com/muhnschein/lautta/issues/100)).
- Streaming videos no longer freeze the screen every second or so, and start and seek faster.
- Servers follow what you allow Lautta in netvfs: full access, read and add, read only, or add only, per server ([#104](https://github.com/muhnschein/lautta/pull/104)). Folders show the level and offer only what it allows. Needs netvfs 2.0.
- WebDAV and FTPS servers work like SFTP and SMB: accounts from *Settings › Accounts* show up as locations (needs netvfs with WebDAV and FTPS).
- Lautta never takes a server address, password or one-time code any more: every server is an account in *Settings › Accounts*. *Connect to server* and its recent servers are gone. Tapping a server found nearby offers to set it up as an account. Needs netvfs 2.0.
- Settings is tidier: no previews in *View options* and *Browse page*, and the Settings list shows names only. *About* shows the Lautta icon and no longer lists permissions ([#117](https://github.com/muhnschein/lautta/issues/117)).
- Code, logs and other monospaced text use JetBrains Mono, in the text viewer and in grid previews.
- The music player shows the cover and the title, artist, album, year and track number ([#111](https://github.com/muhnschein/lautta/issues/111)). The cover comes from the file, else from a cover.jpg, folder.jpg or front.jpg beside it. Audio files show their cover as thumbnail in folders too.

## 1.0.0

First release.

- Browse, copy, move, rename and delete files on the phone, memory cards and USB drives, and on SFTP and SMB servers with netvfs ([#1](https://github.com/muhnschein/lautta/pull/1)).
- Built-in PDF viewer ([#12](https://github.com/muhnschein/lautta/pull/12)), with password, page count and reopen at the last page ([#14](https://github.com/muhnschein/lautta/pull/14)), contents, go to page and links ([#16](https://github.com/muhnschein/lautta/pull/16)), and text search ([#18](https://github.com/muhnschein/lautta/pull/18)).
- Images zoom around your fingers ([#7](https://github.com/muhnschein/lautta/pull/7)).
- Running tasks show in an Activity bar ([#11](https://github.com/muhnschein/lautta/pull/11)).
- Grid view with 3 or 4 items per row ([#15](https://github.com/muhnschein/lautta/pull/15)).
- Hide and reorder what Browse shows ([#17](https://github.com/muhnschein/lautta/pull/17)).
- Selection mode is hard to enter by accident and easy to leave ([#19](https://github.com/muhnschein/lautta/pull/19)).
- Item menus are plain Silica rows ([#20](https://github.com/muhnschein/lautta/pull/20)).
- Five rounds of device feedback reshape Browse, folders, Trash and the viewers ([#25](https://github.com/muhnschein/lautta/pull/25)).
- Text files show their first lines in the grid ([#48](https://github.com/muhnschein/lautta/pull/48)).
- Servers show a server icon ([#58](https://github.com/muhnschein/lautta/pull/58)).
- Folders inside an archive offer Extract all ([#64](https://github.com/muhnschein/lautta/pull/64)).
- New app icon: a folder holding a page ([#91](https://github.com/muhnschein/lautta/pull/91), [#93](https://github.com/muhnschein/lautta/pull/93)).
- Scan documents into a PDF from a folder's Add menu ([#92](https://github.com/muhnschein/lautta/pull/92)).
- The licence text ships with the app ([#97](https://github.com/muhnschein/lautta/pull/97)).
