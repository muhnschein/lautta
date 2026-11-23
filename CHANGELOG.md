# Changelog

## Unreleased

- Recents show thumbnails, and show as a grid when *View options* say Grid, with each day group starting its own row. Text files on the Home page's Recents tiles and in the Recents grid show their first lines, as in folders ([#60](https://github.com/muhnschein/lautta/issues/60)).
- Photos open faster in the image viewer, big ones most: the picture starts loading as your finger touches it, loads at screen size and first, and shows the camera's preview while it loads, or the folder's thumbnail when the photo has none. Pictures you swiped past come back at once ([#41](https://github.com/muhnschein/lautta/issues/41)).
- PDF thumbnails of files on this device show the top of the first page, not its middle.
- New items scroll into view and light up briefly: folders you make (also in the folder picker), pasted or moved items, files you compress, extracted files, scans and renamed items; in grid view the name and details light up too ([#113](https://github.com/muhnschein/lautta/issues/113)).
- A folder listed again right after a change no longer shows as empty for a moment.
- Videos from servers stream and seek instead of downloading first ([#100](https://github.com/muhnschein/lautta/issues/100)).
- Streaming videos no longer freeze the screen every second or so, and start and seek faster.
- Servers follow what you allow Lautta in netvfs: full access, read and add, read only, or add only, per server ([#104](https://github.com/muhnschein/lautta/pull/104)). Folders show the level and offer only what it allows. Needs netvfs 2.0.
- WebDAV and FTPS servers work like SFTP and SMB: accounts from *Settings › Accounts* show up as locations (needs netvfs with WebDAV and FTPS).
- Lautta never takes a server address, password or one-time code any more: every server is an account in *Settings › Accounts*. *Connect to server* and its recent servers are gone. Tapping a server found nearby offers to set it up as an account. Needs netvfs 2.0.
- Markdown is nicer to read: headings in the ambience colour, check boxes for task lists, quotes with a bar, code on a tinted panel with syntax colours, and calmer tables, on the same backdrop as the text viewer. Plain `.txt` files open in the reading font, with *View source* for the line view. Big files no longer freeze the app while they open ([#50](https://github.com/muhnschein/lautta/issues/50)).
- Settings is tidier: no previews in *View options* and *Browse page*, and the Settings list shows names only. *About* shows the Lautta icon and no longer lists permissions ([#117](https://github.com/muhnschein/lautta/issues/117)).
- Memory cards and USB drives without a label show as "Memory card" or "USB drive" instead of a string of letters and digits ([#123](https://github.com/muhnschein/lautta/issues/123)).
- Code, logs and other monospaced text use JetBrains Mono, in the text viewer, in grid previews and in Markdown code.
- Compress and Extract are simpler ([#115](https://github.com/muhnschein/lautta/issues/115), [#116](https://github.com/muhnschein/lautta/issues/116)). Compress asks only for a name and a format: Zip, 7z, tar.xz or tar.gz. Extract starts at once from *More actions* of a compressed file or from the pulley inside one. Results land in the same folder; one item comes out as is, several in a new folder named after the compressed file, and nothing is overwritten. Both show only where Lautta may add files.
- Zip and 7z files can have a password (AES-256): Compress asks for it twice and says when it is easy to guess. 7z hides the file names too; Zip cannot. Locked Zip and 7z files open and extract after asking for the password on the same *Locked file* page that PDFs use. The password stays in memory only while the file is open, in use by a task or on the clipboard, and is never stored ([#115](https://github.com/muhnschein/lautta/issues/115), [#116](https://github.com/muhnschein/lautta/issues/116)). Zips locked the old way (ZipCrypto) still don't open.
- The music player shows the cover and the title, artist, album, year and track number ([#111](https://github.com/muhnschein/lautta/issues/111)). The cover comes from the file, else from a cover.jpg, folder.jpg or front.jpg beside it. Audio files show their cover as thumbnail in folders too.
- Sharing a file from a server shows "Getting …" while it downloads, then the share sheet. Before, nothing showed until the download was done, so a big file looked like the share was lost ([#126](https://github.com/muhnschein/lautta/issues/126)).
- When a server goes away, the offline banner says when the folder shown was listed instead of "Showing the copy from ." A listing cut short by a lost connection shows its error instead of the offline banner ([#106](https://github.com/muhnschein/lautta/issues/106)).
- *Checksums* in *More actions* of a file: SHA-256, MD5, SHA-1, SHA-512 and more (SHA-3, BLAKE2b, BLAKE3, CRC32, xxHash). Tap one to calculate it; tap the value to copy it. When a checksum list such as SHA256SUMS in the same folder names the file, Lautta says whether it matches. A checksum list itself offers *Verify files*: it checks every file the list names ([#40](https://github.com/muhnschein/lautta/issues/40)).
- Browse starts with your three newest recent files as tiles; *All recents* opens the whole list, and a long press on a tile removes it from recents. The tiles replace the *Recents* row and can be switched off in *Settings › Browse page*. *On this device* now shows the phone's free space ([#169](https://github.com/muhnschein/lautta/issues/169)).
- The cover shows what a tap brings back: the folder's first files as small tiles, free space and your newest recent files on Browse, the open file in a viewer, and Trash and Recents as tiles. While tasks run, the whole cover fills from the left with their progress ([#121](https://github.com/muhnschein/lautta/issues/121)).
- While a photo without a camera preview opens, its folder thumbnail fills the photo's whole shape, softly blurred, instead of a square between black bars, so the photo fades in without a jump ([#176](https://github.com/muhnschein/lautta/issues/176)).
- The photo at the top of an image's *Details* shows at once instead of loading again ([#174](https://github.com/muhnschein/lautta/issues/174)).

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
