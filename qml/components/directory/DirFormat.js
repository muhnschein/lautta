// SPDX-License-Identifier: LGPL-2.1-or-later
// Text of directory rows: "size · modified" (SPEC §15.3, board Directory).
.pragma library
.import Sailfish.Silica 1.0 as Silica

function _sameDay(a, b) {
    return a.getFullYear() === b.getFullYear() && a.getMonth() === b.getMonth() && a.getDate() === b.getDate()
}

// "Today 09:14", "Yesterday", "12 Sep", "12 Sep 2024"; ms < 0 means unknown.
function modified(ms) {
    if (ms === undefined || ms < 0)
        return ""
    var date = new Date(ms)
    var now = new Date()
    if (_sameDay(date, now))
        return Silica.Format.formatDate(date, Silica.Formatter.TimepointRelativeCurrentDay)
    var yesterday = new Date(now.getFullYear(), now.getMonth(), now.getDate() - 1)
    if (_sameDay(date, yesterday))
        //% "Yesterday"
        return qsTrId("lautta-dir-yesterday")
    if (date.getFullYear() === now.getFullYear())
        return Silica.Format.formatDate(date, Silica.Formatter.DateMediumWithoutYear)
    return Silica.Format.formatDate(date, Silica.Formatter.DateMedium)
}

function subtitle(isDir, isSymlink, size, modifiedMs) {
    var parts = []
    if (isSymlink)
        //% "Link"
        parts.push(qsTrId("lautta-dir-link"))
    if (!isDir && size >= 0)
        parts.push(Silica.Format.formatFileSize(size))
    var when = modified(modifiedMs)
    if (when.length > 0)
        parts.push(when)
    return parts.join(" · ")
}

// Transfer state of a row, "uploading:42" (kind:percent), for the
// subtitle and the thin progress line under it.
function transferText(state, locationName) {
    var kind = state.split(":")[0]
    var percent = parseInt(state.split(":")[1] || "0", 10)
    switch (kind) {
    case "uploading":
        //% "Uploading to %1 · %2%"
        return qsTrId("lautta-dir-uploading").arg(locationName).arg(percent)
    case "downloading":
        //% "Downloading from %1 · %2%"
        return qsTrId("lautta-dir-downloading").arg(locationName).arg(percent)
    default:
        //% "Waiting in Transfers"
        return qsTrId("lautta-dir-transfer-waiting")
    }
}

function transferPercent(state) {
    var percent = parseInt(state.split(":")[1] || "0", 10)
    return isNaN(percent) ? 0 : Math.max(0, Math.min(100, percent)) / 100
}
