# Changelog

## Unreleased

- Browse rows have even heights. Servers, memory cards and USB drives show their state as a glowing dot. It pulses while a server connects or a drive mounts or unmounts, and turns red or orange on failure or warning.
- Browse shows no item counts any more.
- Memory cards and USB drives can be mounted and unmounted from Browse, including drives Sailfish OS does not mount on its own. This needs netvfs with drive mounting.
- A locked encrypted memory card or USB drive shows on Browse as a dimmed row with a padlock. Tapping it opens Settings › Storage to unlock it. This needs netvfs that lists locked drives.
- Settings › Browse page edits the Browse page in place, with a live preview. It follows drives and servers as they come and go. Volumes and Servers can each be hidden as a whole, and come back with their earlier choices. "Hide from Browse" is gone from the menus.
- Recents is a row on Browse. Activity and Recents are grouped by date.
- Deleted files go to a Trash in the app's own data instead of a folder in each location. Deleting for good asks first. The Trash shows thumbnails, and its note sits at the top.
- Files can be favourites. A favourite is edited on a page of its own, with a choice of icons.
- More actions replaces the Details page. In selection mode the bar is the same for one item or several, and Compress, Rename and Add to favourites live behind More actions. Stop selecting sits on a highlighted key.
- The folder pulley reads, from the bottom: New, Select, Search, View options, Activity. "New folder" is now "New".
- View options is a settings page with a live preview, and open folders follow it at once.
- Settings are split into pages. The thumbnail cache size applies without a restart. A size limit replaces the metered-network limit for server thumbnails.
- PDFs get thumbnails of their first page, under the same thumbnail switches as images. For PDFs on the phone the system draws them, as it does for photos and videos.
- In grid view, holding and dragging selects a range. Bars at the bottom are opaque.
- The grid has one size, so View options offers just grid or list. Its icons are bigger, names take up to two lines and keep their extension, each row is only as tall as its tallest tile, and each tile shows, right under its name, the date and size, or how many items a folder on the phone or a drive holds, counting up to 1000.
- File icons are Lautta's own drawings, sharp in the grid and in every list at any screen scale. An icon turns the highlight colour with its name when pressed or selected.
- Video thumbnails show the video icon on a dark disc, at the size and place the icon has without a thumbnail, so they stand apart from photos.
- A folder's header stays in place while its contents scroll, and the pulley opens above it. A folder opens at its top, and going back returns to where you were. Starting or ending selection no longer makes the header jump.
- Closing the selection bar no longer flashes its empty state as it slides away.
- Counts read correctly in English: 1 item, 1 day, 1 task, not 1 items, 1 days, 1 tasks.
- Search puts the field and filters above the results.
- Pasting cut items moves them after a remorse timer: tap it to cancel. The undo banner is gone.
- More actions can move the selected items into a new folder made where they are.
- The text viewer always wraps lines, colours code by its language, and puts the text on a backdrop for contrast. Markdown previews wrap code blocks.
- The image viewer works like Jolla Gallery's. Its More button is a circle with three dots, and More › Rename renames.
- PDFs open at the top with the whole header in view.
- A PDF that trips the PDF renderer shows as unreadable instead of closing Lautta.
- Photos show the date they were taken, in the system's formats.
- Settings › Storage opens from Lautta where the system allows it.
