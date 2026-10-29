// SPDX-License-Identifier: LGPL-2.1-or-later
// Settings › Browse page (BRW-12), as Salama's tab group view: a row per row
// of the Browse page, a tap shows or hides it, the grip carries it within
// its section while the rows it passes move aside; Restore defaults in the
// pulley and a live preview below (as Salama's start page settings).
// Favourites only move; removing one stays on the Browse page. Volumes and
// Servers lead with a switch for the whole section: off, the section's rows
// leave this list and the Browse page, and what each row's switch said comes
// back with the section.
//
// A switch or a move changes its row in place, so the list keeps its
// position and its rows (BrowseLayoutDelegate.qml has the carrying).
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
        // Rows that come or go shift the rows under a carried one; its own
        // moves are the only moves while it is carried.
        onRowsInserted: list.dropCarried()
        onRowsRemoved: list.dropCarried()
        onModelReset: list.dropCarried()
    }

    SilicaListView {
        id: list

        // The row being carried, null when none.
        property Item carrier: null

        function dropCarried() {
            if (carrier)
                carrier.drop()
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

        // The rows a carried row passes slide aside rather than jump.
        moveDisplaced: Transition {
            NumberAnimation {
                properties: "y"
                duration: 150
                easing.type: Easing.OutQuad
            }
        }

        delegate: BrowseLayoutDelegate {
            width: list.width
            layoutModel: layout
            layoutSection: model.section
            layoutItem: model.item
            layoutKind: model.kind
            layoutName: model.name
            layoutHidden: model.hidden
            layoutMovable: model.movable
            layoutNested: model.nested
            onToggled: layout.setHidden(layoutSection, layoutItem, shown)
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
