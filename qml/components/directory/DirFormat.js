// SPDX-License-Identifier: LGPL-2.1-or-later
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

function modifiedWithTime(ms) {
    var day = modified(ms)
    var date = new Date(ms)
    if (day.length === 0 || _sameDay(date, new Date()))
        return day
    //% "%1, %2"
    return qsTrId("lautta-dir-date-time").arg(day).arg(Silica.Format.formatDate(date, Silica.Formatter.TimeValue))
}

function items(count, capped) {
    if (count < 0)
        return ""
    if (capped)
        //% "More than %n items"
        return qsTrId("lautta-dir-grid-items-more", count)
    //% "%n items"
    return qsTrId("lautta-dir-grid-items", count)
}

var _days = { "until": 0 }

function _dayBounds() {
    var now = Date.now()
    if (now < _days.until && now >= _days.today)
        return _days
    var d = new Date(now)
    var y = d.getFullYear(), m = d.getMonth(), day = d.getDate()
    _days = {
        "today": new Date(y, m, day).getTime(),
        "until": new Date(y, m, day + 1).getTime(),
        "yesterday": new Date(y, m, day - 1).getTime(),
        "yearStart": new Date(y, 0, 1).getTime(),
        "yearEnd": new Date(y + 1, 0, 1).getTime()
    }
    return _days
}

function _modifiedFast(ms, withTime) {
    if (ms === undefined || ms < 0)
        return ""
    var b = _dayBounds()
    var date = new Date(ms)
    if (ms >= b.today && ms < b.until)
        return Silica.Format.formatDate(date, Silica.Formatter.TimepointRelative)
    var day
    if (ms >= b.yesterday && ms < b.today)
        day = qsTrId("lautta-dir-yesterday")
    else if (ms >= b.yearStart && ms < b.yearEnd)
        day = Silica.Format.formatDate(date, Silica.Formatter.DateMediumWithoutYear)
    else
        day = Silica.Format.formatDate(date, Silica.Formatter.DateMedium)
    if (!withTime)
        return day
    return qsTrId("lautta-dir-date-time").arg(day).arg(Silica.Format.formatDate(date, Silica.Formatter.TimeValue))
}

function gridCaptionsFast(isDir, size, modifiedMs, counted, count, capped) {
    if (isDir)
        return counted ? [items(count, capped)] : [_modifiedFast(modifiedMs, true)]
    var when = _modifiedFast(modifiedMs, true)
    return size >= 0 ? (when.length > 0 ? [when, Silica.Format.formatFileSize(size)] : [Silica.Format.formatFileSize(size)])
                     : (when.length > 0 ? [when] : [])
}

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

// "uploading:42" (kind:percent).
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
