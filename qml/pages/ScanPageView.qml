// SPDX-License-Identifier: LGPL-2.1-or-later Pages of a scan one at a time, as the image viewer
// shows pictures; all page actions live here.
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../components/viewers"
import "../components/viewers/gallery"
import "../components/ErrorText.js" as ErrorText

FullscreenContentPage {
    id: page

    property QtObject scan
    // Page shown first.
    property int index
    readonly property int pages: scan ? scan.count : 0
    readonly property int current: paged.currentIndex
    readonly property string colour: paged.currentItem ? paged.currentItem.colour : ""
    readonly property bool zoomed: paged.currentItem !== null && paged.currentItem.zoomed
    property bool controlsShown: true

    function colourText(name) {
        if (name === "grayscale")
            //% "Grayscale"
            return qsTrId("lautta-scan-colour-grayscale")
        if (name === "blackwhite")
            //% "Black and white"
            return qsTrId("lautta-scan-colour-black-white")
        //% "Original"
        return qsTrId("lautta-scan-colour-original")
    }

    function adjust() {
        pageStack.push(Qt.resolvedUrl("../dialogs/ScanAdjustDialog.qml"), { "scan": scan, "index": current })
    }

    function retake() {
        pageStack.push(Qt.resolvedUrl("ScanPage.qml"), { "scan": scan, "retakeOnly": true, "retakeIndex": current })
    }

    function remove() {
        var index = current
        //% "Removing page"
        remorse.execute(qsTrId("lautta-scan-removing-page"), function () {
            page.scan.remove(index)
        }, App.setting("remorse_seconds") * 1000)
    }

    allowedOrientations: Orientation.All
    backNavigation: !zoomed

    // Last page removed: nothing left to show.
    onPagesChanged: {
        if (pages === 0 && status === PageStatus.Active)
            pageStack.pop()
    }

    Connections {
        target: page.scan
        onFailed: {
            if (page.status !== PageStatus.Active)
                return
            notice.text = ErrorText.message(kind, {})
            notice.show()
        }
    }

    PagedView {
        id: paged

        objectName: "pagedView"
        anchors.fill: parent
        model: page.scan
        // Binding at creation: set later, the view slides there.
        currentIndex: page.index
        interactive: count > 1 && !page.zoomed && !remorse.active

        delegate: GalleryImage {
            readonly property string colour: model.colour
            readonly property int pageId: model.pageId

            width: paged.width
            height: paged.height
            source: "file://" + model.view
            active: PagedView.isCurrentItem
            viewMoving: paged.moving
            onZoomedChanged: page.controlsShown = !zoomed
            onClicked: {
                if (zoomed)
                    zoomOut()
                else
                    page.controlsShown = !page.controlsShown
            }
        }
    }

    Item {
        id: overlay

        objectName: "overlay"
        anchors.fill: parent
        enabled: page.controlsShown && !remorse.active
        opacity: enabled ? 1.0 : 0.0
        Behavior on opacity { FadeAnimator { } }

        FadeGradient {
            width: parent.width
            height: dismissButton.height + 2 * dismissButton.y
            topDown: true
        }

        FadeGradient {
            width: parent.width
            height: tools.height + 2 * tools.anchors.bottomMargin
            anchors.bottom: parent.bottom
        }

        Label {
            objectName: "placeLabel"
            x: Theme.horizontalPageMargin
            anchors.verticalCenter: dismissButton.verticalCenter
            //% "%1 of %2"
            text: qsTrId("lautta-scan-page-of").arg(page.current + 1).arg(page.pages)
        }

        IconButton {
            id: dismissButton

            objectName: "dismissButton"
            y: Theme.paddingLarge
            anchors {
                right: parent.right
                rightMargin: Theme.horizontalPageMargin
            }
            icon.source: "image://theme/icon-m-dismiss"
            onClicked: pageStack.pop()
        }

        Column {
            id: tools

            width: parent.width
            anchors {
                bottom: parent.bottom
                bottomMargin: parent.height / 20
            }
            spacing: Theme.paddingMedium

            Row {
                anchors.horizontalCenter: parent.horizontalCenter

                Repeater {
                    model: ["original", "grayscale", "blackwhite"]

                    BackgroundItem {
                        id: choice

                        objectName: "colour-" + modelData
                        width: label.implicitWidth + 2 * Theme.paddingLarge
                        height: Theme.itemSizeExtraSmall
                        highlighted: down || page.colour === modelData
                        onClicked: page.scan.setColour(page.current, modelData)

                        Label {
                            id: label

                            anchors.centerIn: parent
                            font.pixelSize: Theme.fontSizeSmall
                            color: choice.highlighted ? Theme.highlightColor : Theme.primaryColor
                            text: page.colourText(modelData)
                        }
                    }
                }
            }

            Row {
                anchors.horizontalCenter: parent.horizontalCenter
                spacing: Theme.paddingLarge

                IconButton {
                    objectName: "adjustButton"
                    icon.source: "image://theme/icon-m-edit"
                    onClicked: page.adjust()
                }

                IconButton {
                    objectName: "rotateButton"
                    icon.source: "image://theme/icon-m-rotate-right"
                    onClicked: page.scan.turn(page.current, 1)
                }

                IconButton {
                    objectName: "retakeButton"
                    icon.source: "image://theme/icon-m-camera"
                    onClicked: page.retake()
                }

                IconButton {
                    objectName: "removeButton"
                    icon.source: "image://theme/icon-m-delete"
                    onClicked: page.remove()
                }
            }
        }
    }

    BusyIndicator {
        anchors.centerIn: parent
        size: BusyIndicatorSize.Large
        running: page.scan !== null && page.scan.busy
    }

    RemorsePopup { id: remorse }

    Notice { id: notice }
}
