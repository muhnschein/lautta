// SPDX-License-Identifier: LGPL-2.1-or-later
// Share target "Save to Lautta" (INT-1): files received from other apps are
// saved to a folder of the user's choice, on this device or a server. Files
// the sandbox does not let Lautta read are reported and skipped.
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../components/operations"

Dialog {
    id: page

    // The resources of ShareProvider.triggered: URLs, paths or objects with
    // a filePath.
    property var resources: []
    property string destUri
    property string customUri
    property var entries: []
    property var destinations: []

    readonly property int readableCount: countReadable(true)
    readonly property int skippedCount: countReadable(false)

    allowedOrientations: Orientation.All
    canAccept: readableCount > 0 && destUri.length > 0

    function pathOf(resource) {
        if (typeof resource === "string")
            return resource
        return resource.filePath || resource.url || ""
    }

    function countReadable(readable) {
        var n = 0
        for (var i = 0; i < entries.length; ++i)
            if (entries[i].readable === readable)
                ++n
        return n
    }

    function load() {
        var paths = []
        for (var i = 0; i < resources.length; ++i)
            paths.push(pathOf(resources[i]))
        var probed = JSON.parse(Operations.probeShared(JSON.stringify(paths)))
        var list = []
        for (var j = 0; j < probed.length; ++j) {
            var file = probed[j]
            // Data resources without a file have nothing to copy.
            if (paths[j].length === 0)
                file.readable = false
            list.push(file)
        }
        entries = list
        destinations = JSON.parse(Operations.destinationsJson())
        for (var k = 0; k < destinations.length; ++k) {
            if (destinations[k].uri.indexOf("user-downloads") >= 0)
                destUri = destinations[k].uri
        }
        if (destUri.length === 0 && destinations.length > 0)
            destUri = destinations[0].uri
    }

    onAccepted: {
        var uris = []
        for (var i = 0; i < entries.length; ++i)
            if (entries[i].readable)
                uris.push(entries[i].uri)
        Operations.copyTo(JSON.stringify(uris), destUri)
    }

    Component.onCompleted: load()

    SilicaListView {
        id: list

        anchors.fill: parent
        model: page.entries
        currentIndex: -1

        header: Column {
            width: list.width

            DialogHeader {
                //% "Save here"
                acceptText: qsTrId("lautta-share-accept")
            }
            DialogSubtitle {
                //% "Save %n files"
                text: qsTrId("lautta-share-count", page.entries.length)
            }
        }

        delegate: Item {
            width: list.width
            height: Theme.itemSizeMedium

            OpIcon {
                id: glyph
                x: Theme.horizontalPageMargin
                anchors.verticalCenter: parent.verticalCenter
                category: App.categoryOf(modelData.name)
                opacity: modelData.readable ? 1.0 : Theme.opacityLow
            }
            Column {
                anchors {
                    left: glyph.right
                    leftMargin: Theme.paddingMedium
                    right: sizeLabel.left
                    rightMargin: Theme.paddingMedium
                    verticalCenter: parent.verticalCenter
                }
                Label {
                    width: parent.width
                    text: modelData.name
                    color: modelData.readable ? Theme.primaryColor : Theme.secondaryColor
                    truncationMode: TruncationMode.Fade
                }
                Label {
                    width: parent.width
                    visible: !modelData.readable
                    color: Theme.errorColor
                    font.pixelSize: Theme.fontSizeExtraSmall
                    truncationMode: TruncationMode.Fade
                    //% "This file can't be read by Lautta"
                    text: qsTrId("lautta-share-unreadable")
                }
            }
            Label {
                id: sizeLabel
                anchors {
                    right: parent.right
                    rightMargin: Theme.horizontalPageMargin
                    verticalCenter: parent.verticalCenter
                }
                color: Theme.secondaryColor
                font.pixelSize: Theme.fontSizeSmall
                text: Format.formatFileSize(modelData.size)
            }
        }

        footer: Column {
            width: list.width

            SectionHeader {
                //% "Save to"
                text: qsTrId("lautta-share-save-to")
            }
            Repeater {
                model: page.destinations

                BackgroundItem {
                    width: list.width
                    height: Theme.itemSizeSmall
                    highlighted: down || page.destUri === modelData.uri
                    onClicked: page.destUri = modelData.uri

                    Label {
                        anchors {
                            left: parent.left
                            leftMargin: Theme.horizontalPageMargin
                            right: parent.right
                            rightMargin: Theme.horizontalPageMargin
                            verticalCenter: parent.verticalCenter
                        }
                        text: modelData.name
                        color: parent.highlighted ? Theme.highlightColor : Theme.primaryColor
                        truncationMode: TruncationMode.Fade
                    }
                }
            }
            BackgroundItem {
                width: list.width
                height: Theme.itemSizeSmall
                highlighted: down || (page.customUri.length > 0 && page.destUri === page.customUri)
                onClicked: {
                    var picker = pageStack.push(Qt.resolvedUrl("../dialogs/FolderPickerDialog.qml"), {
                        //% "Save to"
                        "title": qsTrId("lautta-share-pick"),
                        //% "Select"
                        "acceptText": qsTrId("lautta-share-pick-accept"),
                        "startUri": page.destUri
                    })
                    picker.accepted.connect(function() {
                        page.customUri = picker.selectedUri
                        page.destUri = picker.selectedUri
                    })
                }

                Label {
                    anchors {
                        left: parent.left
                        leftMargin: Theme.horizontalPageMargin
                        right: parent.right
                        rightMargin: Theme.horizontalPageMargin
                        verticalCenter: parent.verticalCenter
                    }
                    text: page.customUri.length > 0
                          ? Operations.displayPath(page.customUri)
                          //% "Another folder…"
                          : qsTrId("lautta-share-other")
                    color: parent.highlighted ? Theme.highlightColor : Theme.primaryColor
                    truncationMode: TruncationMode.Fade
                }
            }
            Label {
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                horizontalAlignment: Text.AlignHCenter
                wrapMode: Text.Wrap
                color: Theme.secondaryColor
                font.pixelSize: Theme.fontSizeSmall
                //% "%n files will be saved."
                text: qsTrId("lautta-share-will-save", page.readableCount) + (page.skippedCount > 0
                      //% "%n files are skipped."
                      ? " " + qsTrId("lautta-share-skipped", page.skippedCount) : "")
            }
        }

        VerticalScrollDecorator { }
    }
}
