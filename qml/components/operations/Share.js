// SPDX-License-Identifier: LGPL-2.1-or-later
.pragma library

var recentMax = 3

// Most recent first, no repeats.
function remember(recent, uri) {
    var out = [uri]
    for (var i = 0; i < recent.length && out.length < recentMax; ++i)
        if (recent[i] !== uri)
            out.push(recent[i])
    return out
}

function within(uri, places) {
    return places.some(function (p) { return uri.indexOf(p.uri) === 0 })
}

// `places`: writable locations now; a folder on a removed server or card drops out.
function usable(recent, places) {
    return recent.filter(function (uri) { return typeof uri === "string" && within(uri, places) })
}

// Last folder saved to, else Downloads, else the first place.
function start(recent, places) {
    if (recent.length > 0)
        return recent[0]
    for (var i = 0; i < places.length; ++i)
        if (places[i].uri.indexOf("user-downloads") >= 0)
            return places[i].uri
    return places.length > 0 ? places[0].uri : ""
}

// `skipped`: names of files Lautta can't read.
function note(readable, skipped) {
    //% "Save %n files"
    var save = readable > 0 ? qsTrId("lautta-share-count", readable) : ""
    var lost = skipped.length === 1
               //% "%1 can't be read, skipped"
               ? qsTrId("lautta-share-skipped-one").arg(skipped[0])
               //% "%n files can't be read, skipped"
               : skipped.length > 1 ? qsTrId("lautta-share-skipped-many", skipped.length) : ""
    return [save, lost].filter(function (s) { return s.length > 0 }).join(" · ")
}

function locationIcon(kind) {
    switch (kind) {
    case "volume": return "image://theme/icon-m-sd-card"
    case "server": return "server"
    }
    return "file-folder"
}
