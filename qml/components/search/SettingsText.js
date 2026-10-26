// SPDX-License-Identifier: LGPL-2.1-or-later
// Texts of the settings pages (SPEC §18): the line under each entry of the
// Settings page, which tells what is chosen on the page it opens, and the
// labels of the day and size choices. No Silica here, so that the host
// tests can check it; sizes come formatted.
.pragma library

// "30 days" for each number of days.
function days(list) {
    return list.map(function (n) {
        //% "%n days"
        return qsTrId("lautta-settings-days", n)
    })
}

// "Up to 20 MB" for each size in MB.
function megabytes(list) {
    return list.map(function (n) {
        //% "Up to %1 MB"
        return qsTrId("lautta-settings-up-to-mb").arg(n)
    })
}

function viewName(viewMode) {
    if (viewMode !== "grid")
        //% "List"
        return qsTrId("lautta-settings-summary-list")
    //% "Grid"
    return qsTrId("lautta-settings-summary-grid")
}

function sortName(sortKey) {
    switch (sortKey) {
    case "size":
        //: How folders are sorted, in "List, sorted by size"
        //% "size"
        return qsTrId("lautta-settings-summary-by-size")
    case "modified":
        //: How folders are sorted, in "List, sorted by date modified"
        //% "date modified"
        return qsTrId("lautta-settings-summary-by-modified")
    case "type":
        //: How folders are sorted, in "List, sorted by type"
        //% "type"
        return qsTrId("lautta-settings-summary-by-type")
    default:
        //: How folders are sorted, in "List, sorted by name"
        //% "name"
        return qsTrId("lautta-settings-summary-by-name")
    }
}

// View options (BRW-4): "Grid, sorted by size".
function viewOptions(viewMode, sortKey) {
    //: %1 is how folders show ("List", "Grid"), %2 their order ("name")
    //% "%1, sorted by %2"
    return qsTrId("lautta-settings-summary-view").arg(viewName(viewMode)).arg(sortName(sortKey))
}

// Deleting (OPS-8): where deletes go and for how long.
function deleting(trash, retentionDays) {
    return trash
            //% "Kept in Trash for %n days"
            ? qsTrId("lautta-settings-summary-trash", retentionDays)
            //% "Deleted permanently"
            : qsTrId("lautta-settings-summary-no-trash")
}

// Transfers (§15): how many run at once on this device.
function transfers(localConcurrency) {
    //% "%n at once"
    return qsTrId("lautta-settings-summary-transfers", localConcurrency)
}

// Search (SRC-3): how deep a search on a server goes.
function search(remoteDepth) {
    //% "%n folders deep on servers"
    return qsTrId("lautta-settings-summary-search", remoteDepth)
}

// Recents (ORG-2): on or off.
function recents(enabled) {
    return enabled
            //% "Remembered"
            ? qsTrId("lautta-settings-summary-recents-on")
            //% "Not remembered"
            : qsTrId("lautta-settings-summary-recents-off")
}

// Storage (SEC-5): how much the caches hold, `size` formatted.
function storage(size) {
    //% "%1 cached"
    return qsTrId("lautta-settings-summary-storage").arg(size)
}
