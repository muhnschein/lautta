// SPDX-License-Identifier: LGPL-2.1-or-later
.pragma library

function name(word) {
    switch (word) {
    case "read":
        //% "Read only"
        return qsTrId("lautta-access-read")
    case "read-add":
        //% "Read and add"
        return qsTrId("lautta-access-read-add")
    case "add":
        //% "Add only"
        return qsTrId("lautta-access-add")
    }
    return ""
}
