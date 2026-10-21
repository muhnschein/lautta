// SPDX-License-Identifier: LGPL-2.1-or-later
// Settings › Browse page (BRW-12; laid out as Salama's start page settings):
// a switch per row of the Browse page, rows moved within their section by
// dragging the handle, Restore defaults in the pulley and a live preview
// below. Favourites only move; removing one stays on the Browse page.
//
// Rows are plain: a TextSwitch (one press target, as in Salama) and the
// handle. A drag shows only once the finger moved on the handle, and ends
// on release, on cancel, and when its row goes or the rows move.
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../components/browse"
import "../components/browse/BrowseText.js" as BrowseText

Page {
    id: page

    readonly property alias layoutModel: layout
    // The list and the rows the preview shows (tests).
    readonly property alias listView: list
    property int previewCount

    allowedOrientations: Orientation.All

    BrowseLayoutModel {
        id: layout

        Component.onCompleted: reload()
        // A switch changes its row in place; rows that come, go or move
        // make the drag's list rows stale.
        onRowsInserted: list.endDrag()
        onRowsRemoved: list.endDrag()
        onRowsMoved: list.endDrag()
        onModelReset: list.endDrag()
    }

    SilicaListView {
        id: list

        // The row being dragged, the list row it started at and where it
        // would land; none until the finger moved on a handle.
        property Item dragItem: null
        property int dragFrom: -1
        property int dropAt: -1

        function startDrag(item, row) {
            dragItem = item
            dragFrom = row
            dropAt = row
        }

        function endDrag() {
            dragItem = null
            dragFrom = -1
            dropAt = -1
        }

        anchors.fill: parent
        model: layout

        PullDownMenu {
            MenuItem {
                //% "Restore defaults"
                text: qsTrId("lautta-browse-layout-restore")
                onClicked: layout.restoreDefaults()
            }
        }

        header: PageHeader {
            title: qsTrId("lautta-settings-browse-page")
        }

        section.property: "section"
        section.criteria: ViewSection.FullString
        section.delegate: SectionHeader {
            text: BrowseText.sectionTitle(section)
        }

        delegate: Item {
            id: row

            readonly property string layoutSection: model.section
            readonly property string layoutItem: model.item
            readonly property bool toggles: model.kind !== "favourite"
            readonly property bool dragged: list.dragItem === row
            readonly property bool target: list.dragItem !== null && !dragged && list.dropAt === index
            readonly property string title: model.kind === "nearby"
                                            //% "Servers found nearby"
                                            ? qsTrId("lautta-browse-layout-nearby")
                                            : BrowseText.rowName(model.kind, model.name)

            width: list.width
            height: Math.max(Theme.itemSizeSmall, toggle.visible ? toggle.height : 0)

            Component.onDestruction: if (dragged) list.endDrag()

            // The dragged row stands out as a pressed one does.
            Rectangle {
                anchors.fill: parent
                visible: row.dragged
                color: Theme.rgba(Theme.highlightBackgroundColor, Theme.highlightBackgroundOpacity)
            }

            TextSwitch {
                id: toggle

                visible: row.toggles
                anchors.verticalCenter: parent.verticalCenter
                rightMargin: handle.visible ? handle.width : Theme.horizontalPageMargin
                automaticCheck: false
                checked: !model.hidden
                text: row.title
                onClicked: layout.setHidden(row.layoutSection, row.layoutItem, !model.hidden)
            }

            // Favourites have no switch: the star sits where the switch is.
            Item {
                id: star

                visible: !row.toggles
                x: Theme.horizontalPageMargin - Theme.paddingLarge
                width: Theme.itemSizeExtraSmall
                height: parent.height

                HighlightImage {
                    anchors.centerIn: parent
                    source: "image://theme/icon-m-favorite"
                    highlighted: row.dragged
                }
            }
            Label {
                visible: !row.toggles
                anchors {
                    left: star.right
                    right: handle.left
                    verticalCenter: parent.verticalCenter
                }
                text: row.title
                color: row.dragged ? Theme.highlightColor : Theme.primaryColor
                truncationMode: TruncationMode.Fade
            }

            // Where the dragged row lands: a line above or below this one.
            Rectangle {
                visible: row.target
                width: parent.width
                height: Math.max(2, Theme.paddingSmall / 2)
                y: list.dropAt < list.dragFrom ? 0 : parent.height - height
                color: Theme.highlightColor
            }

            // Press and drag the handle to move the row within its section.
            MouseArea {
                id: handle

                property real pressY

                visible: model.movable
                anchors {
                    right: parent.right
                    top: parent.top
                    bottom: parent.bottom
                }
                width: Theme.itemSizeSmall + Theme.horizontalPageMargin
                preventStealing: true

                onPressed: pressY = mouse.y
                onPositionChanged: {
                    if (!row.dragged) {
                        if (Math.abs(mouse.y - pressY) < Theme.startDragDistance)
                            return
                        list.startDrag(row, index)
                    }
                    var p = mapToItem(list.contentItem, mouse.x, mouse.y)
                    var to = list.indexAt(p.x, p.y)
                    if (to >= 0 && layout.sectionAt(to) === row.layoutSection)
                        list.dropAt = to
                }
                onReleased: {
                    if (!row.dragged)
                        return
                    if (list.dropAt !== list.dragFrom)
                        layout.moveTo(row.layoutSection, row.layoutItem, layout.placeAt(list.dropAt))
                    list.endDrag()
                }
                onCanceled: if (row.dragged) list.endDrag()

                HighlightImage {
                    anchors {
                        right: parent.right
                        rightMargin: Theme.horizontalPageMargin - Theme.paddingMedium
                        verticalCenter: parent.verticalCenter
                    }
                    source: "image://theme/icon-m-menu"
                    highlighted: handle.pressed || row.dragged
                }
            }
        }

        footer: Column {
            width: list.width

            SectionHeader {
                //% "Preview"
                text: qsTrId("lautta-browse-layout-preview")
            }
            Item {
                width: 1
                height: Theme.paddingLarge
            }
            BrowsePreview {
                width: parent.width
                onCountChanged: page.previewCount = count
                screenWidth: page.width
                screenHeight: page.height
            }
            Item {
                width: 1
                height: Theme.paddingLarge
            }
        }

        VerticalScrollDecorator { }
    }
}
