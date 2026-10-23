// SPDX-License-Identifier: LGPL-2.1-or-later
// Icons of kinds of files that every place showing them shares (UI-2), so an
// archive looks the same in a folder, in search, in the operations pages and
// on the selection panel's Compress (OPS-12).
.pragma library

// The platform's archive icon: Silica's Theme.iconForMimeType gives it to zip
// and tar files. icon-m-file-compressed is not one: it is the zipper badge the
// Sailfish file manager lays over the corner of a file's icon, so on its own
// it sits off centre.
var archive = "image://theme/icon-m-file-archive-folder"
