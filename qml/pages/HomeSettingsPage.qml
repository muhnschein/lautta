// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../components"
import "../components/home"
import "../components/home/HomeText.js" as HomeText

Page {
    id: page

    readonly property alias layoutModel: layout
    readonly property alias listView: list

    allowedOrientations: Orientation.All

    HomeLayoutModel {
        id: layout

        Component.onCompleted: reload()
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
                text: qsTrId("lautta-home-layout-restore")
                onClicked: layout.restoreDefaults()
            }
        }

        header: HeaderBand {
            PageHeader {
                title: qsTrId("lautta-settings-home-page")
            }
        }

        section.property: "section"
        section.criteria: ViewSection.FullString
        section.delegate: SectionHeader {
            text: HomeText.sectionTitle(section)
        }

        moveDisplaced: Transition {
            NumberAnimation {
                properties: "y"
                duration: 150
                easing.type: Easing.OutQuad
            }
        }

        delegate: HomeLayoutDelegate {
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

        VerticalScrollDecorator { }
    }
}
