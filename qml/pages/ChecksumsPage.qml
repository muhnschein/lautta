// SPDX-License-Identifier: LGPL-2.1-or-later
// Checksums (SCR-61) and More algorithms (SCR-62): tap one algorithm to calculate it. Nothing runs
// before the tap.
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../components/operations/Json.js" as Json

Page {
    id: page

    property string uri
    // `common`, or `more` for More algorithms.
    property string group: "common"

    readonly property var rows: {
        var list = Json.value(App.checksumAlgorithms(group), [])
        if (group === "common")
            //% "More algorithms"
            list.push({ "id": "more", "label": qsTrId("lautta-checksum-more") })
        return list
    }

    function choose(id) {
        if (id === "more")
            pageStack.push(Qt.resolvedUrl("ChecksumsPage.qml"), { "uri": page.uri, "group": "more" })
        else
            pageStack.push(Qt.resolvedUrl("ChecksumPage.qml"), { "uri": page.uri, "algorithm": id })
    }

    allowedOrientations: Orientation.All

    SilicaListView {
        anchors.fill: parent
        model: page.rows

        header: PageHeader {
            title: page.group === "more" ? qsTrId("lautta-checksum-more")
                                           //% "Checksums"
                                         : qsTrId("lautta-checksum-title")
        }

        delegate: BackgroundItem {
            objectName: "algorithmRow"
            onClicked: page.choose(modelData.id)

            Label {
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                anchors.verticalCenter: parent.verticalCenter
                text: modelData.label
                truncationMode: TruncationMode.Fade
                color: parent.highlighted ? Theme.highlightColor : Theme.primaryColor
            }
        }

        VerticalScrollDecorator { }
    }
}
