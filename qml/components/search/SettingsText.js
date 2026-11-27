// SPDX-License-Identifier: LGPL-2.1-or-later
.pragma library

function days(list) {
    return list.map(function (n) {
        //% "%n days"
        return qsTrId("lautta-settings-days", n)
    })
}

function megabytes(list) {
    return list.map(function (n) {
        //% "Up to %1 MB"
        return qsTrId("lautta-settings-up-to-mb").arg(n)
    })
}

function largeOperation(items, bytes) {
    if (bytes >= 1000000000)
        //% "Over %1 items or %2 GB"
        return qsTrId("lautta-settings-large-gb").arg(items).arg(Math.round(bytes / 100000000) / 10)
    //% "Over %1 items or %2 MB"
    return qsTrId("lautta-settings-large-mb").arg(items).arg(Math.round(bytes / 1000000))
}
