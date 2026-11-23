// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import QtQml.Models 2.2
import Sailfish.Silica 1.0
import Lautta 1.0
import "../components"
import "../components/Open.js" as Open
import "../components/browse"
import "../components/directory"
import "../components/directory/GridRows.js" as GridRows
import "../components/viewers"

Page {
    id: page

    property var listModel: recents
    readonly property bool recentsOn: JSON.parse(App.settingsJson).recents_enabled !== false
    readonly property string coverKind: "grid"
    //% "Recents"
    readonly property string coverTitle: qsTrId("lautta-recents-title")
    readonly property var coverModel: listModel
    readonly property bool gridMode: JSON.parse(App.settingsJson).view_mode === "grid"
    readonly property real tileWidth: Math.min(Screen.width, Screen.height) / gridMetrics.portraitColumns
    readonly property int columns: GridRows.columns(width, tileWidth, gridMetrics.portraitColumns)
    readonly property real cellWidth: width / columns

    allowedOrientations: Orientation.All

    function showMode() {
        list.model = null
        list.delegate = gridMode ? gridRow : listRow
        list.model = gridMode ? gridRows : listModel
    }

    function fitGrid() {
        var sections = []
        for (var i = 0; gridMode && i < dates.items.count; ++i)
            sections.push(dates.items.get(i).model.dateGroup)
        var rows = GridRows.sectionRows(sections, columns).map(function (r) {
            return { "dateGroup": r.section, "first": r.first, "count": r.count }
        })
        GridRows.fitRows(gridRows, rows)
    }

    onGridModeChanged: {
        fitGrid()
        showMode()
    }
    onListModelChanged: {
        fitGrid()
        showMode()
    }
    onColumnsChanged: fitGrid()
    Component.onCompleted: showMode()

    Connections {
        target: App
        onSettingsJsonChanged: recents.reload()
    }

    function openRecent(row, tileSize) {
        if (row.kind === "transferred")
            pageStack.push(Qt.resolvedUrl("DirectoryPage.qml"), { "uri": App.parentUri(row.uri) })
        else
            Open.open(pageStack, App, Operations, row.uri, false, "", "", tileSize)
    }

    ViewerPrefetch { id: prefetch }

    RecentsModel {
        id: recents
        Component.onCompleted: reload()
    }

    DelegateModel {
        id: dates

        model: page.listModel
        delegate: Item { }
    }

    Connections {
        target: dates.items
        onChanged: page.fitGrid()
    }

    ListModel {
        id: gridRows
    }

    GridMetrics {
        id: gridMetrics
    }

    SilicaListView {
        id: list

        anchors.fill: parent

        header: PageHeader {
            //% "Recents"
            title: qsTrId("lautta-recents-title")
        }

        section.property: "dateGroup"
        section.criteria: ViewSection.FullString
        section.delegate: DateSectionHeader {
            group: section
        }

        PullDownMenu {
            visible: recents.count > 0

            MenuItem {
                //% "Clear recents"
                text: qsTrId("lautta-recents-clear")
                onClicked: {
                    //% "Clearing recents"
                    remorsePopup.execute(qsTrId("lautta-recents-clearing"), function() {
                        recents.clear()
                    }, App.setting("remorse_seconds") * 1000)
                }
            }
        }

        ViewPlaceholder {
            enabled: recents.loaded && recents.count === 0
            text: page.recentsOn
                  //% "No recents yet"
                  ? qsTrId("lautta-recents-empty")
                  //% "Recents are off"
                  : qsTrId("lautta-recents-off")
            hintText: page.recentsOn
                      //% "Files you open, edit or transfer show up here"
                      ? qsTrId("lautta-recents-empty-hint")
                      //% "Turn them on in Settings"
                      : qsTrId("lautta-recents-off-settings-hint")
        }

        VerticalScrollDecorator { }
    }

    RemorsePopup {
        id: remorsePopup
    }

    Component {
        id: listRow

        ListItem {
            id: row

            contentHeight: Theme.itemSizeMedium
            menu: Component {
                ContextMenu {
                    MenuItem {
                        //% "Remove from recents"
                        text: qsTrId("lautta-recents-remove")
                        onClicked: recents.remove(model.itemId)
                    }
                }
            }
            onPressed: if (model.kind !== "transferred") prefetch.start(model.uri, model.mimeType)
            onClicked: page.openRecent(model, icon.thumbnailReady ? Math.round(icon.size) : 0)
            onPressAndHold: prefetch.stop()
            onCanceled: prefetch.stop()

            FileIcon {
                id: icon

                objectName: "recentIcon"
                x: Theme.horizontalPageMargin
                anchors.verticalCenter: parent.verticalCenter
                category: model.category
                thumbnailSource: model.thumbnailSource
                mimeType: model.mimeType
                highlighted: row.highlighted
            }
            Column {
                anchors {
                    left: parent.left
                    leftMargin: Theme.horizontalPageMargin + Theme.iconSizeMedium + Theme.paddingLarge
                    right: parent.right
                    rightMargin: Theme.horizontalPageMargin
                    verticalCenter: parent.verticalCenter
                }

                Label {
                    width: parent.width
                    text: model.name
                    color: row.highlighted ? Theme.highlightColor : Theme.primaryColor
                    truncationMode: TruncationMode.Fade
                }
                PlaceLabel {
                    width: parent.width
                    text: model.place
                    remote: App.isRemote(model.uri)
                    color: row.highlighted ? Theme.secondaryHighlightColor : Theme.secondaryColor
                }
            }
        }
    }

    Component {
        id: gridRow

        ListItem {
            id: row

            readonly property int first: model.first
            readonly property int end: model.first + model.count
            property int heldId: -1

            function regroup() {
                var all = rowCells.items
                var to = Math.min(end, all.count)
                var kept = []
                for (var i = inRow.count - 1; i >= 0; --i) {
                    var at = inRow.get(i).itemsIndex
                    if (at < first || at >= to)
                        inRow.remove(i, 1)
                    else
                        kept.push(at)
                }
                for (var j = first; j < to; ++j) {
                    if (kept.indexOf(j) < 0)
                        all.addGroups(j, 1, "inRow")
                }
            }

            contentHeight: cells.height
            menu: Component {
                ContextMenu {
                    MenuItem {
                        //% "Remove from recents"
                        text: qsTrId("lautta-recents-remove")
                        onClicked: recents.remove(row.heldId)
                    }
                }
            }
            onFirstChanged: regroup()
            onEndChanged: regroup()
            Component.onCompleted: regroup()

            DelegateModel {
                id: rowCells

                model: page.listModel
                groups: DelegateModelGroup {
                    id: inRow

                    name: "inRow"
                }
                filterOnGroup: "inRow"
                delegate: BackgroundItem {
                    id: cell

                    readonly property string itemName: model.name
                    readonly property bool lit: highlighted || (row.menuOpen && row.heldId === model.itemId)
                    readonly property real gap: gridMetrics.gap

                    objectName: "recentGridTile"
                    x: (index - row.first) * width
                    width: page.cellWidth
                    height: cells.height
                    implicitHeight: gridMetrics.cellHeight(width, caption.implicitHeight + place.height)
                    onPressed: if (model.kind !== "transferred") prefetch.start(model.uri, model.mimeType)
                    onClicked: page.openRecent(model, tileIcon.thumbnailReady ? Math.round(tileIcon.width) : 0)
                    onPressAndHold: {
                        prefetch.stop()
                        row.heldId = model.itemId
                        row.openMenu()
                    }
                    onCanceled: prefetch.stop()

                    Item {
                        id: tile

                        x: cell.gap
                        y: cell.gap
                        width: cell.width - 2 * cell.gap
                        height: width

                        Rectangle {
                            anchors.fill: parent
                            visible: !tileIcon.thumbnailReady
                            color: Theme.rgba(Theme.highlightBackgroundColor, Theme.highlightBackgroundOpacity / 3)
                        }
                        TextThumb {
                            id: lines

                            uri: model.textThumb ? model.uri : ""
                        }
                        FileIcon {
                            id: tileIcon

                            anchors.fill: parent
                            size: tile.width
                            glyphSize: gridMetrics.glyphSize(tile.width)
                            category: model.category
                            thumbnailSource: model.thumbnailSource
                            mimeType: model.mimeType
                            previewText: lines.text
                            previewMono: model.textMono
                            typeIcon: model.typeIcon
                            highlighted: cell.lit
                        }
                    }
                    GridCaption {
                        id: caption

                        anchors {
                            top: tile.bottom
                            topMargin: Theme.paddingSmall / 2
                            left: tile.left
                            right: tile.right
                        }
                        metrics: gridMetrics
                        name: cell.itemName
                        emphasised: cell.lit
                        pressed: cell.lit
                    }
                    PlaceLabel {
                        id: place

                        anchors {
                            top: caption.bottom
                            left: tile.left
                            right: tile.right
                        }
                        text: model.place
                        remote: App.isRemote(model.uri)
                        fontSize: Theme.fontSizeTiny
                        horizontalAlignment: Text.AlignHCenter
                        color: cell.lit ? Theme.secondaryHighlightColor : Theme.secondaryColor
                    }
                }
            }

            Connections {
                target: rowCells.items
                onChanged: row.regroup()
            }

            MouseArea {
                anchors.fill: parent
            }

            Item {
                id: cells

                width: parent.width
                height: GridRows.tallest(children)

                Repeater {
                    model: rowCells
                }
            }
        }
    }
}
