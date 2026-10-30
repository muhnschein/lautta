// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../components/browse"
import "../components/browse/BrowseText.js" as BrowseText

Page {
    id: page

    readonly property alias layoutModel: layout
    readonly property alias listView: list
    property int previewCount

    allowedOrientations: Orientation.All

    BrowseLayoutModel {
        id: layout

        Component.onCompleted: reload()
        // Rows coming or going shift rows under a carried one.
        onRowsInserted: list.dropCarried()
        onRowsRemoved: list.dropCarried()
        onModelReset: list.dropCarried()
    }

    SilicaListView {
        id: list

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
