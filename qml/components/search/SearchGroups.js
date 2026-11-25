// SPDX-License-Identifier: LGPL-2.1-or-later
.pragma library

var unreachable = ["TimedOut", "NetworkUnreachable", "ConnectionLost", "BridgeUnavailable",
                   "TooManyConnections", "RateLimited", "LocationGone"]
var attention = ["AuthFailed", "ServerIdentityUnknown", "ServerIdentityChanged", "PasswordRequired",
                 "SecurityPolicy"]

function find(groups, key) {
    for (var i = 0; i < groups.length; ++i) {
        if (groups[i].key === key)
            return groups[i]
    }
    return null
}

function withoutRows(groups) {
    return groups.filter(function (g) { return !(g.rows > 0) })
}

function name(group) {
    if (!group)
        return ""
    //% "This device"
    return group.kind === "device" ? qsTrId("lautta-search-group-device") : group.name
}

function failed(group) {
    return !!group && group.state === "failed"
}

function status(group) {
    if (!group)
        return ""
    switch (group.state) {
    case "searching":
        return group.folders > 0
               //% "Searching · %n folders"
               ? qsTrId("lautta-search-group-searching-folders", group.folders)
               //% "Searching"
               : qsTrId("lautta-search-group-searching")
    case "stopped":
        //% "Stopped"
        return qsTrId("lautta-search-group-stopped")
    case "failed":
        if (unreachable.indexOf(group.errorKind) >= 0)
            //% "Not reachable"
            return qsTrId("lautta-search-group-unreachable")
        if (group.errorKind === "PermissionDenied")
            //% "Not allowed"
            return qsTrId("lautta-search-group-denied")
        if (attention.indexOf(group.errorKind) >= 0)
            //% "Needs attention"
            return qsTrId("lautta-search-group-attention")
        //% "Search failed"
        return qsTrId("lautta-search-failed")
    default:
        return group.hits > 0
               //% "%n results"
               ? qsTrId("lautta-search-group-found", group.hits)
               //% "Nothing found"
               : qsTrId("lautta-search-group-none")
    }
}

function scopeIcon(scope) {
    switch (scope) {
    case "device":
        return "image://theme/icon-m-device"
    case "everywhere":
        return "image://theme/icon-m-website"
    default:
        return "image://theme/icon-m-folder"
    }
}

function scopeName(scope, folderName) {
    switch (scope) {
    case "device":
        return qsTrId("lautta-search-group-device")
    case "everywhere":
        //% "Everywhere"
        return qsTrId("lautta-search-scope-everywhere")
    default:
        return folderName
    }
}

function toggled(list, value) {
    var out = list.slice()
    var at = out.indexOf(value)
    if (at >= 0)
        out.splice(at, 1)
    else
        out.push(value)
    return out
}
