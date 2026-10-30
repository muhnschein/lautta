// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0

SectionHeader {
    property string group

    objectName: "dateSection"
    text: {
        switch (group) {
        case "inProgress":
            //% "In progress"
            return qsTrId("lautta-date-in-progress")
        case "today":
            //% "Today"
            return qsTrId("lautta-date-today")
        case "yesterday":
            //% "Yesterday"
            return qsTrId("lautta-date-yesterday")
        case "last7Days":
            //% "Last 7 days"
            return qsTrId("lautta-date-last-7-days")
        case "last30Days":
            //% "Last 30 days"
            return qsTrId("lautta-date-last-30-days")
        default:
            //% "Older"
            return qsTrId("lautta-date-older")
        }
    }
}
