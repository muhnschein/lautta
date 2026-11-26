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

function scopeChoices(fromFolder, folderName, locations) {
    var out = []
    if (fromFolder)
        out.push({ "value": "folder", "label": folderName, "icon": "image://theme/icon-m-folder" })
    out.push({ "value": "device", "label": qsTrId("lautta-search-group-device"),
               "icon": "image://theme/icon-m-device" })
    var servers = false
    for (var i = 0; i < locations.length; ++i) {
        out.push({ "value": locations[i].value, "label": locations[i].label, "icon": locations[i].icon })
        servers = servers || locations[i].kind === "server"
    }
    if (servers)
        //% "Everywhere"
        out.push({ "value": "everywhere", "label": qsTrId("lautta-search-scope-everywhere"),
                   "icon": "image://theme/icon-m-website" })
    return out
}

function hasChoice(choices, value) {
    for (var i = 0; i < choices.length; ++i) {
        if (choices[i].value === value)
            return true
    }
    return false
}

function scopeIcon(scope, choices) {
    for (var i = 0; i < choices.length; ++i) {
        if (choices[i].value === scope)
            return choices[i].icon
    }
    return "image://theme/icon-m-folder"
}

function labelOf(choices, value) {
    for (var i = 0; i < choices.length; ++i) {
        if (choices[i].value === value)
            return choices[i].label
    }
    return ""
}
