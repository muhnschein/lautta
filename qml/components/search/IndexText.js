// SPDX-License-Identifier: LGPL-2.1-or-later
.pragma library

var rank = { "failed": 0, "crawling": 1, "pending": 2, "off": 3, "ready": 4 }

function rows(lines) {
    var device = null
    var out = []
    for (var i = 0; i < lines.length; ++i) {
        var l = lines[i]
        if (l.kind === "device") {
            if (!device) {
                device = { "kind": "device", "location": "", "name": "", "names": 0, "state": "ready",
                           "checkedMs": l.checkedMs, "wanted": true }
                out.push(device)
            }
            device.names += l.names
            if (rank[l.state] < rank[device.state])
                device.state = l.state
            if (l.checkedMs < device.checkedMs)
                device.checkedMs = l.checkedMs
        } else {
            out.push({ "kind": l.kind, "location": l.location, "name": l.name, "names": l.names, "state": l.state,
                       "checkedMs": l.checkedMs, "wanted": l.wanted })
        }
    }
    return out
}

function name(row) {
    //% "This device"
    return row.kind === "device" ? qsTrId("lautta-search-group-device") : row.name
}

function status(row) {
    switch (row.state) {
    case "ready":
        //% "%n names · up to date"
        return qsTrId("lautta-index-ready", row.names)
    case "crawling":
        //% "Updating · %n names so far"
        return qsTrId("lautta-index-crawling", row.names)
    case "failed":
        //% "Not reachable · %n names from the last update"
        return qsTrId("lautta-index-failed", row.names)
    case "off":
        //% "Not indexed: each search walks the server"
        return qsTrId("lautta-index-off")
    default:
        //% "Waiting to start"
        return qsTrId("lautta-index-pending")
    }
}
