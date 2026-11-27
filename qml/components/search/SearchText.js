// SPDX-License-Identifier: LGPL-2.1-or-later
.pragma library
.import Sailfish.Silica 1.0 as Silica

function sizeAndDate(size, modified, isDir) {
    var parts = []
    if (!isDir && size >= 0)
        parts.push(Silica.Format.formatFileSize(size))
    if (modified >= 0)
        parts.push(Silica.Format.formatDate(new Date(modified), Silica.Formatter.DateMedium))
    return parts.join(" · ")
}
