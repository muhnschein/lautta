// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0

Item {
    id: root

    objectName: "coverLanes"
    clip: true

    HighlightImage {
        objectName: "lanesFaint"
        anchors.fill: parent
        source: Qt.resolvedUrl("lanes-faint.svg")
        sourceSize.width: width
        sourceSize.height: height
        fillMode: Image.PreserveAspectCrop
        color: Theme.primaryColor
    }

    HighlightImage {
        objectName: "lanesLit"
        anchors.fill: parent
        source: Qt.resolvedUrl("lanes-lit.svg")
        sourceSize.width: width
        sourceSize.height: height
        fillMode: Image.PreserveAspectCrop
        color: Theme.highlightColor
    }
}
