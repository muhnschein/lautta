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
