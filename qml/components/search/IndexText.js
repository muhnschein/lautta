// SPDX-License-Identifier: LGPL-2.1-or-later
.pragma library

var rank = { "nospace": 0, "failed": 0, "crawling": 1, "pending": 2, "off": 3, "ready": 4 }

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
        //% "Up to date"
        return qsTrId("lautta-index-state-ready")
    case "crawling":
        //% "Updating"
        return qsTrId("lautta-index-state-updating")
    case "failed":
        //% "Not reachable"
        return qsTrId("lautta-index-state-unreachable")
    case "nospace":
        //% "Not enough space"
        return qsTrId("lautta-index-state-no-space")
    case "wifi":
        //% "Waiting for Wi-Fi"
        return qsTrId("lautta-index-state-wifi")
    case "off":
        //% "Not indexed"
        return qsTrId("lautta-index-state-off")
    default:
        //% "Waiting"
        return qsTrId("lautta-index-state-waiting")
    }
}
