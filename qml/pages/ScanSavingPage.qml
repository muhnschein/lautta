// SPDX-License-Identifier: LGPL-2.1-or-later Writes a scan as PDF into its folder. Done: back to
// the folder. Failed: back to the save dialog, pages kept.
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0

Page {
    id: page

    property string parentUri
    // Folder view the scan belongs to; told when the scan is left.
    property Item owner
    property QtObject scan
    property string name
    property bool started
    property bool handedOver

    function failed(kind) {
        handedOver = true
        pageStack.replace(Qt.resolvedUrl("../dialogs/ScanSaveDialog.qml"), {
            "parentUri": parentUri, "owner": owner, "scan": scan, "name": name, "failure": kind
        })
    }

    allowedOrientations: Orientation.All
    backNavigation: false

    onStatusChanged: {
        if (status === PageStatus.Active && !started && scan) {
            started = true
            scan.save(parentUri, name)
        }
    }
    Component.onDestruction: {
        if (scan && owner && !handedOver)
            owner.endScan(scan, false)
    }

    Connections {
        target: page.started ? page.scan : null
        onSaved: {
            page.handedOver = true
            page.owner.endScan(page.scan, true)
            pageStack.pop()
        }
        onFailed: page.failed(kind)
    }

    ProgressBar {
        objectName: "saveProgress"
        anchors.verticalCenter: parent.verticalCenter
        width: parent.width
        minimumValue: 0
        maximumValue: 1
        value: page.scan ? page.scan.progress : 0
        //% "Saving %1"
        label: qsTrId("lautta-scan-saving").arg(page.name)
    }
}
