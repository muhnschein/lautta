// SPDX-License-Identifier: LGPL-2.1-or-later
// Text helpers of the search and compare pages.
.pragma library
.import Sailfish.Silica 1.0 as Silica

function escapeHtml(s) {
    return s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;")
}

// A hit's name as styled text with the matching part in the highlight
// colour (design: Search).
function highlightedName(name, start, length, color) {
    if (length <= 0)
        return escapeHtml(name)
    return escapeHtml(name.substr(0, start))
        + "<font color=\"" + color + "\">" + escapeHtml(name.substr(start, length)) + "</font>"
        + escapeHtml(name.substr(start + length))
}

// "4.2 MB · 3 Oct"; unknown parts are left out.
function sizeAndDate(size, modified, isDir) {
    var parts = []
    if (!isDir && size >= 0)
        parts.push(Silica.Format.formatFileSize(size))
    if (modified >= 0)
        parts.push(Silica.Format.formatDate(new Date(modified), Silica.Formatter.DateMedium))
    return parts.join(" · ")
}

// Sizes in the preview lines, "82 MB".
function bytes(n) {
    return Silica.Format.formatFileSize(n)
}
