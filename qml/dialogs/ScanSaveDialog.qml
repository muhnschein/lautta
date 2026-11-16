// SPDX-License-Identifier: LGPL-2.1-or-later Name and page order of a scan before it is saved as
// PDF into the folder it was started from. Cancel drops the scan.
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../components/scan/ScanMath.js" as ScanMath
import "../components/directory/Names.js" as Names
import "../components/ErrorText.js" as ErrorText

Dialog {
    id: dialog

    property string parentUri
    // Folder view the scan belongs to; told when the scan is left.
    property Item owner
    property QtObject scan
    // Name to start with; empty: "Scan" and today's date.
    property string name
    // Why the last save failed, as an error kind; said once.
    property string failure
    property bool shown
    property bool handedOver
    readonly property int pages: scan ? scan.count : 0
    readonly property bool badName: Names.invalid(nameField.text)

    function addPage() {
        handedOver = true
        pageStack.replace(Qt.resolvedUrl("../pages/ScanPage.qml"),
                          { "parentUri": parentUri, "owner": owner, "scan": scan, "name": nameField.text })
    }

    function showPage(index) {
        pageStack.push(Qt.resolvedUrl("../pages/ScanPageView.qml"), { "scan": scan, "index": index })
    }

    allowedOrientations: Orientation.All
    canAccept: pages > 0 && !scan.busy && nameField.text.length > 0 && !badName
    acceptDestination: Qt.resolvedUrl("../pages/ScanSavingPage.qml")
    acceptDestinationAction: PageStackAction.Replace

    onStatusChanged: {
        if (status !== PageStatus.Active)
            return
        if (!shown && failure.length > 0) {
            notice.text = ErrorText.message(failure, { "location": App.locationName(parentUri) })
            notice.show()
        }
        shown = true
    }
    onAccepted: {
        handedOver = true
        var next = acceptDestinationInstance
        next.parentUri = parentUri
        next.owner = owner
        next.scan = scan
        next.name = nameField.text
    }
    Component.onDestruction: {
        if (scan && owner && !handedOver)
            owner.endScan(scan, false)
    }

    SilicaFlickable {
        id: flickable

        anchors.fill: parent
        contentHeight: column.height + Theme.paddingLarge

        Column {
            id: column

            width: parent.width

            DialogHeader {
                //% "Save"
                acceptText: qsTrId("lautta-scan-save")
                //% "Save %n pages as PDF"
                title: qsTrId("lautta-scan-save-title", dialog.pages)
            }

            TextField {
                id: nameField

                objectName: "nameField"
                width: parent.width
                //% "Scan %1"
                text: dialog.name.length > 0 ? dialog.name
                                             : qsTrId("lautta-scan-default-name").arg(Qt.formatDate(new Date(), "yyyy-MM-dd"))
                errorHighlight: dialog.badName
                //% "Name"
                placeholderText: qsTrId("lautta-scan-name")
                label: dialog.badName ? ErrorText.message("InvalidName", {}) : placeholderText
                EnterKey.iconSource: "image://theme/icon-m-enter-close"
                EnterKey.onClicked: focus = false
            }

            SectionHeader {
                //% "Pages"
                text: qsTrId("lautta-scan-pages")
            }

            // Cells placed by hand: a held page must know where its cell is while cells move.
            Item {
                id: grid

                readonly property int columns: 2
                readonly property real cellWidth: width / columns
                readonly property real cellHeight: cellWidth * 1.3

                objectName: "pageGrid"
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * x
                // Pages and the plus tile.
                height: Math.ceil((dialog.pages + 1) / columns) * cellHeight

                Repeater {
                    model: dialog.scan

                    // Held for a second, the page follows the finger; pages it passes make room.
                    BackgroundItem {
                        id: tile

                        property bool held
                        property bool moved
                        property real pressX
                        property real pressY
                        property real fingerX
                        property real fingerY

                        function follow(mouse) {
                            var at = mapToItem(grid, mouse.x, mouse.y)
                            fingerX = at.x
                            fingerY = at.y
                            var to = ScanMath.cellAt(at.x, at.y, grid.cellWidth, grid.cellHeight, grid.columns, dialog.pages)
                            if (to >= 0 && to !== index)
                                dialog.scan.movePage(index, to)
                        }

                        function drop() {
                            holdTimer.stop()
                            held = false
                        }

                        objectName: "pageTile"
                        x: (index % grid.columns) * grid.cellWidth
                        y: Math.floor(index / grid.columns) * grid.cellHeight
                        width: grid.cellWidth
                        height: grid.cellHeight
                        z: held ? 1 : 0
                        Behavior on x { NumberAnimation { duration: 150; easing.type: Easing.InOutQuad } }
                        Behavior on y { NumberAnimation { duration: 150; easing.type: Easing.InOutQuad } }
                        preventStealing: held
                        highlighted: down || held
                        // Finger stays on the spot of the page it took, wherever the cell goes.
                        contentItem.x: held ? fingerX - x - pressX : 0
                        contentItem.y: held ? fingerY - y - pressY : 0
                        contentItem.scale: held ? 1.05 : 1

                        onPressed: {
                            pressX = mouse.x
                            pressY = mouse.y
                            moved = false
                            holdTimer.restart()
                        }
                        onPositionChanged: {
                            if (held)
                                return follow(mouse)
                            if (Math.abs(mouse.x - pressX) > Theme.iconSizeSmall
                                    || Math.abs(mouse.y - pressY) > Theme.iconSizeSmall)
                                holdTimer.stop()
                        }
                        onReleased: drop()
                        onCanceled: drop()
                        onClicked: {
                            if (!moved)
                                dialog.showPage(index)
                        }

                        Timer {
                            id: holdTimer

                            interval: 1000
                            onTriggered: {
                                tile.fingerX = tile.x + tile.pressX
                                tile.fingerY = tile.y + tile.pressY
                                tile.moved = true
                                tile.held = true
                            }
                        }

                        Image {
                            anchors {
                                fill: parent
                                margins: Theme.paddingMedium
                                bottomMargin: number.height + Theme.paddingMedium
                            }
                            fillMode: Image.PreserveAspectFit
                            sourceSize.width: Screen.width / 2
                            asynchronous: true
                            cache: false
                            source: "file://" + model.view
                        }

                        Label {
                            id: number

                            objectName: "pageNumber"
                            anchors {
                                bottom: parent.bottom
                                horizontalCenter: parent.horizontalCenter
                            }
                            font.pixelSize: Theme.fontSizeSmall
                            color: tile.highlighted ? Theme.secondaryHighlightColor : Theme.secondaryColor
                            text: index + 1
                        }
                    }
                }

                BackgroundItem {
                    id: addTile

                    objectName: "addPageTile"
                    x: (dialog.pages % grid.columns) * grid.cellWidth
                    y: Math.floor(dialog.pages / grid.columns) * grid.cellHeight
                    width: grid.cellWidth
                    height: grid.cellHeight
                    onClicked: dialog.addPage()

                    // Room a page picture gets in its tile.
                    Item {
                        id: plusRoom

                        anchors {
                            fill: parent
                            margins: Theme.paddingMedium
                            bottomMargin: plusSpacer.height + Theme.paddingMedium
                        }
                    }

                    Label {
                        id: plusSpacer

                        visible: false
                        font.pixelSize: Theme.fontSizeSmall
                        text: " "
                    }

                    Rectangle {
                        objectName: "addPageShape"
                        anchors.centerIn: plusRoom
                        width: Math.min(plusRoom.width, plusRoom.height / Math.SQRT2)
                        height: width * Math.SQRT2
                        color: Theme.rgba(Theme.highlightBackgroundColor, Theme.opacityFaint)

                        Icon {
                            anchors.centerIn: parent
                            source: "image://theme/icon-l-add"
                            highlighted: true
                        }
                    }
                }
            }
        }

        VerticalScrollDecorator { }
    }

    Notice { id: notice }
}
