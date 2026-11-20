# Changelog

## Unreleased

- New items scroll into view and light up briefly: folders you make (also in the folder picker), pasted or moved items, archives you compress, extracted files, scans and renamed items ([#113](https://github.com/muhnschein/lautta/issues/113)).
- A folder listed again right after a change no longer shows as empty for a moment.
- Videos from servers stream and seek instead of downloading first ([#100](https://github.com/muhnschein/lautta/issues/100)).
- Streaming videos no longer freeze the screen every second or so, and start and seek faster.
- Servers follow what you allow Lautta in netvfs: full access, read and add, read only, or add only, per server ([#104](https://github.com/muhnschein/lautta/pull/104)). Folders show the level and offer only what it allows. Needs netvfs 2.0.
- WebDAV and FTPS servers work like SFTP and SMB: accounts from *Settings › Accounts* show up as locations (needs netvfs with WebDAV and FTPS).
- Lautta never takes a server address, password or one-time code any more: every server is an account in *Settings › Accounts*. *Connect to server* and its recent servers are gone. Tapping a server found nearby offers to set it up as an account. Needs netvfs 2.0.

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
