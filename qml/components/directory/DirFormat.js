// SPDX-License-Identifier: LGPL-2.1-or-later
// Text of directory rows: "size · modified", and of grid cells
// (SPEC §15.3, boards Directory and DirectoryGrid).
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
        return Silica.Format.formatDate(date, Silica.Formatter.TimepointRelative)
    var yesterday = new Date(now.getFullYear(), now.getMonth(), now.getDate() - 1)
    if (_sameDay(date, yesterday))
        //% "Yesterday"
        return qsTrId("lautta-dir-yesterday")
    if (date.getFullYear() === now.getFullYear())
        return Silica.Format.formatDate(date, Silica.Formatter.DateMediumWithoutYear)
    return Silica.Format.formatDate(date, Silica.Formatter.DateMedium)
}

// modified() with the time of day after a date: "Yesterday, 21:40", "12 Oct,
// 10:02". Today has its time in modified() already.
function modifiedWithTime(ms) {
    var day = modified(ms)
    var date = new Date(ms)
    if (day.length === 0 || _sameDay(date, new Date()))
        return day
    //% "%1, %2"
    return qsTrId("lautta-dir-date-time").arg(day).arg(Silica.Format.formatDate(date, Silica.Formatter.TimeValue))
}

// "1 item", "14 items", "More than 10000 items" for a count that stopped at
// its cap; empty while there is none (`count` < 0).
function items(count, capped) {
    if (count < 0)
        return ""
    if (capped)
        //% "More than %n items"
        return qsTrId("lautta-dir-grid-items-more", count)
    // Engineering English has one plural form: one item has its own string.
    if (count === 1)
        //% "1 item"
        return qsTrId("lautta-dir-grid-item")
    //% "%n items"
    return qsTrId("lautta-dir-grid-items", count)
}

// The caption lines under a grid cell, none of them empty: a
// file has its modified time and its size; a folder its item count when the
// folder view counts (`counted`: this device and drives), else its modified
// time (servers, archives: counting would list them). A folder not counted
// yet, or that cannot be read, has none.
function gridCaptions(isDir, size, modifiedMs, counted, count, capped) {
    var lines = isDir ? [counted ? items(count, capped) : modifiedWithTime(modifiedMs)]
                      : [modifiedWithTime(modifiedMs), size >= 0 ? Silica.Format.formatFileSize(size) : ""]
    return lines.filter(function (line) { return line.length > 0 })
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
        //% "Waiting in Activity"
        return qsTrId("lautta-dir-transfer-waiting")
    }
}

function transferPercent(state) {
    var percent = parseInt(state.split(":")[1] || "0", 10)
    return isNaN(percent) ? 0 : Math.max(0, Math.min(100, percent)) / 100
}
