// SPDX-License-Identifier: LGPL-2.1-or-later
// Settings › Browse page (BRW-12; laid out as Salama's start page settings):
// a switch per row of the Browse page, rows moved within their section by
// dragging the handle, Restore defaults in the pulley and a live preview
// below. Favourites only move; removing one stays on the Browse page.
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../components/browse"
import "../components/browse/BrowseText.js" as BrowseText

Page {
    id: page

    readonly property alias layoutModel: layout
    // Rows the preview shows (tests).
    property int previewCount

    allowedOrientations: Orientation.All

    BrowseLayoutModel {
        id: layout
        Component.onCompleted: reload()
    }

    SilicaListView {
        id: list

        // The row being dragged and where it would land, -1 when none.
        property int dragFrom: -1
        property int dropAt: -1

        function endDrag() {
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

        delegate: ListItem {
            id: row

            readonly property bool toggles: model.kind !== "favourite"
            readonly property bool dragged: list.dragFrom === index
            readonly property bool target: list.dropAt === index && list.dragFrom !== index

            contentHeight: Theme.itemSizeSmall
            highlighted: down || dragged
            onClicked: if (toggles) layout.setHidden(model.section, model.item, !model.hidden)

            Switch {
                id: toggle

                visible: row.toggles
                anchors {
                    left: parent.left
                    leftMargin: Theme.horizontalPageMargin - Theme.paddingLarge
                    verticalCenter: parent.verticalCenter
                }
                automaticCheck: false
                checked: !model.hidden
                onClicked: layout.setHidden(model.section, model.item, !model.hidden)
            }
            HighlightImage {
                visible: !row.toggles
                anchors.centerIn: toggle
                source: "image://theme/icon-m-favorite"
                highlighted: row.highlighted
            }

            Label {
                anchors {
                    left: toggle.right
                    right: handle.left
                    verticalCenter: parent.verticalCenter
                }
                text: {
                    switch (model.kind) {
                    case "deleted":
                        return qsTrId("lautta-browse-recently-deleted")
                    case "nearby":
                        //% "Servers found nearby"
                        return qsTrId("lautta-browse-layout-nearby")
                    default:
                        return model.name
                    }
                }
                color: row.highlighted ? Theme.highlightColor : Theme.primaryColor
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

                visible: model.movable
                anchors {
                    right: parent.right
                    top: parent.top
                    bottom: parent.bottom
                }
                width: Theme.itemSizeSmall + Theme.horizontalPageMargin
                preventStealing: true

                onPressed: {
                    list.dragFrom = index
                    list.dropAt = index
                }
                onPositionChanged: {
                    var p = mapToItem(list.contentItem, mouse.x, mouse.y)
                    var to = list.indexAt(p.x, p.y)
                    if (to >= 0 && layout.sectionAt(to) === model.section)
                        list.dropAt = to
                }
                onReleased: {
                    if (list.dropAt >= 0 && list.dropAt !== list.dragFrom)
                        layout.moveTo(model.section, model.item, layout.placeAt(list.dropAt))
                    list.endDrag()
                }
                onCanceled: list.endDrag()

                HighlightImage {
                    anchors {
                        right: parent.right
                        rightMargin: Theme.horizontalPageMargin - Theme.paddingMedium
                        verticalCenter: parent.verticalCenter
                    }
                    source: "image://theme/icon-m-menu"
                    highlighted: row.highlighted || handle.pressed
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
