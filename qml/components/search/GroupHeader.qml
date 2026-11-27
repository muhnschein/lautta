// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import "SearchGroups.js" as SearchGroups

Item {
    id: header

    property var group: null

    width: parent ? parent.width : 0
    height: Theme.itemSizeSmall

    Row {
        objectName: "searchGroupStatus"
        x: Theme.horizontalPageMargin
        width: groupName.x - x - Theme.paddingMedium
        anchors {
            verticalCenter: parent.verticalCenter
            verticalCenterOffset: Theme.paddingMedium / 2
        }
        spacing: Theme.paddingSmall

        BusyIndicator {
            size: BusyIndicatorSize.ExtraSmall
            running: visible
            visible: header.group !== null && header.group.state === "searching"
            anchors.verticalCenter: parent.verticalCenter
        }
        Label {
            objectName: "searchGroupStatusText"
            width: parent.width
            text: SearchGroups.status(header.group)
            truncationMode: TruncationMode.Fade
            font.pixelSize: Theme.fontSizeExtraSmall
            color: SearchGroups.failed(header.group) ? Theme.errorColor : Theme.secondaryColor
            anchors.verticalCenter: parent.verticalCenter
        }
    }

    Label {
        id: groupName

        objectName: "searchGroupName"
        anchors {
            right: parent.right
            rightMargin: Theme.horizontalPageMargin
            verticalCenter: parent.verticalCenter
            verticalCenterOffset: Theme.paddingMedium / 2
        }
        text: SearchGroups.name(header.group)
        color: Theme.highlightColor
        font.pixelSize: Theme.fontSizeSmall
    }
}
