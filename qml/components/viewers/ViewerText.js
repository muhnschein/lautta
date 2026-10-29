// SPDX-License-Identifier: LGPL-2.1-or-later
// Small text helpers of the viewers (headers and details).
.pragma library
.import Sailfish.Silica 1.0 as Silica

// "1:42" or "1:02:03" from milliseconds.
function clock(ms) {
    var total = Math.max(0, Math.floor((ms || 0) / 1000))
    var s = total % 60
    var m = Math.floor(total / 60) % 60
    var h = Math.floor(total / 3600)
    var two = function (n) { return (n < 10 ? "0" : "") + n }
    return h > 0 ? h + ":" + two(m) + ":" + two(s) : m + ":" + two(s)
}

// "60.15° N, 24.95° E"
function coordinates(lat, lon) {
    var f = function (v) { return Math.abs(v).toFixed(2) }
    return f(lat) + "° " + (lat < 0 ? "S" : "N") + ", " + f(lon) + "° " + (lon < 0 ? "W" : "E")
}

// The EXIF orientation as a phrase; the viewer always applies it.
function orientation(o) {
    switch (o) {
    case 3:
        //% "Rotated 180° (applied)"
        return qsTrId("lautta-viewers-orient-180")
    case 6:
        //% "Rotated 90° (applied)"
        return qsTrId("lautta-viewers-orient-90")
    case 8:
        //% "Rotated 270° (applied)"
        return qsTrId("lautta-viewers-orient-270")
    case 2:
    case 4:
    case 5:
    case 7:
        //% "Mirrored (applied)"
        return qsTrId("lautta-viewers-orient-mirrored")
    default:
        return ""
    }
}

// "Taken" row: the EXIF time "2026-09-12 18:42:11", local time without a
// zone, in the system's date and time formats like every date the app
// shows (§18); text in another shape is shown as recorded.
function taken(raw) {
    var parts = /^(\d{4})-(\d{2})-(\d{2}) (\d{2}):(\d{2}):(\d{2})$/.exec(raw || "")
    if (!parts)
        return raw || ""
    var n = parts.map(Number)
    var date = new Date(n[1], n[2] - 1, n[3], n[4], n[5], n[6])
    return Silica.Format.formatDate(date, Silica.Formatter.DateMedium) + " "
        + Silica.Format.formatDate(date, Silica.Formatter.TimeValue)
}

// "Exposure" row: "1/640 s · f/1.8 · ISO 50", missing parts left out.
function exposureLine(info) {
    var parts = []
    if (info.exposure)
        parts.push(info.exposure)
    if (info.aperture)
        parts.push(info.aperture)
    if (info.iso)
        parts.push("ISO " + info.iso)
    return parts.join(" · ")
}

// "Jolla C2": the model, with the make in front unless the model has it.
function camera(info) {
    var make = info.make || ""
    var model = info.model || ""
    if (model.toLowerCase().indexOf(make.toLowerCase()) === 0)
        return model
    return (make + " " + model).trim()
}
