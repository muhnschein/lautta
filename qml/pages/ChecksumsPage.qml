// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../components"
import "../components/operations/Json.js" as Json

Page {
    id: page

    property string uri

    readonly property var rows: {
        var out = []
        var groups = ["common", "more"]
        for (var g = 0; g < groups.length; ++g) {
            var list = Json.value(App.checksumAlgorithms(groups[g]), [])
            for (var i = 0; i < list.length; ++i)
                out.push({ "id": list[i].id, "label": list[i].label, "section": groups[g] })
        }
        return out
    }

    allowedOrientations: Orientation.All

    ListModel { id: rowModel }
    Component.onCompleted: rows.forEach(function (r) { rowModel.append(r) })

    SilicaListView {
        anchors.fill: parent
        model: rowModel

        header: HeaderBand {
            PageHeader {
                //% "Checksums"
                title: qsTrId("lautta-checksum-title")
            }
        }

        section.property: "section"
        section.delegate: SectionHeader {
            objectName: "checksumSection"
            //% "Common"
            text: section === "common" ? qsTrId("lautta-checksum-common")
                                         //% "More"
                                       : qsTrId("lautta-checksum-section-more")
        }

        delegate: BackgroundItem {
            objectName: "algorithmRow"
            onClicked: pageStack.push(Qt.resolvedUrl("ChecksumPage.qml"), { "uri": page.uri, "algorithm": model.id })

            Label {
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                anchors.verticalCenter: parent.verticalCenter
                text: model.label
                truncationMode: TruncationMode.Fade
                color: parent.highlighted ? Theme.highlightColor : Theme.primaryColor
            }
        }

        VerticalScrollDecorator { }
    }
}
