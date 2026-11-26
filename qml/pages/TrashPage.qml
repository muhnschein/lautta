// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import QtQml.Models 2.2
import Sailfish.Silica 1.0
import Lautta 1.0
import "../components"
import "../components/directory"
import "../components/directory/GridRows.js" as GridRows
import "../components/operations"
import "../components/ErrorText.js" as ErrorText

Page {
    id: page

    property var listModel: trash
    readonly property int retentionDays: App.setting("recently_deleted_retention_days") || 30
    readonly property int remorseMs: (App.setting("remorse_seconds") || 5) * 1000
    readonly property int soonDays: 7
    readonly property color soonColor: Theme.presenceColor(Theme.PresenceAway)
    readonly property real sectionTextTop: Theme.paddingMedium + (Theme.itemSizeSmall - Theme.paddingMedium - sectionFont.height) / 2
    readonly property string coverKind: "grid"
    //% "Trash"
    readonly property string coverTitle: qsTrId("lautta-trash-title")
    readonly property var coverModel: listModel
    readonly property bool gridMode: JSON.parse(App.settingsJson).view_mode === "grid"
    readonly property real tileWidth: Math.min(Screen.width, Screen.height) / gridMetrics.portraitColumns
    readonly property int columns: GridRows.columns(width, tileWidth, gridMetrics.portraitColumns)
    readonly property real cellWidth: width / columns

    function soon(daysLeft) {
        return daysLeft <= soonDays
    }

    function daysLeftText(daysLeft) {
        //% "%n days left"
        return qsTrId("lautta-trash-days-left", daysLeft)
    }

    function deleteNow(id) {
        //% "Deleting"
        deleteRemorse.execute(qsTrId("lautta-trash-deleting"), function() { trash.remove(id) }, remorseMs)
    }

    function showFolder(uri) {
        pageStack.push(Qt.resolvedUrl("DirectoryPage.qml"), { "uri": uri })
    }

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

    allowedOrientations: Orientation.All

    onGridModeChanged: {
        fitGrid()
        showMode()
    }
    onListModelChanged: {
        fitGrid()
        showMode()
    }
    onColumnsChanged: fitGrid()
    Component.onCompleted: {
        showMode()
        trash.reload()
    }

    TrashModel {
        id: trash
        onFailed: {
            notice.text = ErrorText.message(kind, {})
            notice.show()
        }
        onRestored: {
            //% "Restored"
            notice.text = qsTrId("lautta-trash-restored")
            notice.show()
        }
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

    FontMetrics {
        id: sectionFont

        font {
            family: Theme.fontFamily
            pixelSize: Theme.fontSizeSmall
        }
    }

    Notice {
        id: notice
    }

    RemorsePopup {
        id: emptyRemorse
    }

    RemorsePopup {
        id: deleteRemorse
    }

    SilicaFlickable {
        id: pulleyHost

        objectName: "trashPulleyHost"
        anchors.fill: parent
        contentWidth: width
        contentHeight: height

        PullDownMenu {
            MenuItem {
                enabled: page.listModel.count > 0 && !trash.busy
                //% "Empty trash"
                text: qsTrId("lautta-trash-empty")
                onClicked: {
                    //% "Emptying trash"
                    emptyRemorse.execute(qsTrId("lautta-trash-emptying"), function() { trash.empty() }, page.remorseMs)
                }
            }
        }

        HeaderBand {
            id: top

            objectName: "trashHeader"
            z: 1

            PageHeader {
                //% "Trash"
                title: qsTrId("lautta-trash-title")
            }
        }

        SilicaListView {
            id: list

            anchors {
                top: top.bottom
                left: parent.left
                right: parent.right
                bottom: parent.bottom
            }
            clip: true

            header: Item {
                objectName: "trashNote"
                width: list.width
                height: note.y + note.height

                Label {
                    id: note

                    x: Theme.horizontalPageMargin
                    y: page.sectionTextTop
                    width: parent.width - 2 * Theme.horizontalPageMargin
                    opacity: page.listModel.count > 0 ? 1 : 0
                    wrapMode: Text.Wrap
                    color: Theme.secondaryColor
                    font.pixelSize: Theme.fontSizeSmall
                    //% "Only files deleted from this device's folders are kept here for %n days. Deletes on volumes and servers are permanent."
                    text: qsTrId("lautta-trash-note-kept", page.retentionDays)
                }
            }

            section.property: "dateGroup"
            section.criteria: ViewSection.FullString
            section.delegate: DateSectionHeader {
                group: section
            }

            ViewPlaceholder {
                enabled: trash.loaded && page.listModel.count === 0
                //% "Trash is empty"
                text: qsTrId("lautta-trash-placeholder")
                //% "Files you delete from this device's folders wait here before they are removed for good."
                hintText: qsTrId("lautta-trash-placeholder-hint")
            }

            VerticalScrollDecorator { }
        }
    }

    Component {
        id: listRow

        ListItem {
            id: item

            objectName: "trashRow"
            contentHeight: Theme.itemSizeMedium
            menu: ContextMenu {
                MenuItem {
                    //% "Restore"
                    text: qsTrId("lautta-trash-restore")
                    onClicked: trash.restore(model.trashId)
                }
                MenuItem {
                    //% "Delete now"
                    text: qsTrId("lautta-trash-delete-now")
                    onClicked: {
                        var id = model.trashId
                        item.remorseAction(qsTrId("lautta-trash-deleting"), function() { trash.remove(id) }, page.remorseMs)
                    }
                }
                MenuItem {
                    //% "Show original folder"
                    text: qsTrId("lautta-trash-show-folder")
                    onClicked: page.showFolder(model.folderUri)
                }
            }

            Item {
                id: glyph

                x: Theme.horizontalPageMargin
                anchors.verticalCenter: parent.verticalCenter
                width: Theme.iconSizeMedium
                height: width

                FileIcon {
                    anchors.fill: parent
                    visible: model.thumbnailSource.length > 0
                    category: App.categoryOf(model.name)
                    thumbnailSource: model.thumbnailSource
                    mimeType: model.mimeType
                    highlighted: item.highlighted
                }
                OpIcon {
                    objectName: "trashGlyph"
                    anchors.centerIn: parent
                    visible: model.thumbnailSource.length === 0
                    isDir: model.isDir
                    category: App.categoryOf(model.name)
                    color: page.soon(model.daysLeft) ? page.soonColor : Theme.primaryColor
                    highlighted: item.highlighted
                }
            }
            Column {
                anchors {
                    left: glyph.right
                    leftMargin: Theme.paddingMedium
                    right: remaining.left
                    rightMargin: Theme.paddingMedium
                    verticalCenter: parent.verticalCenter
                }
                Label {
                    width: parent.width
                    text: model.name
                    color: item.highlighted ? Theme.highlightColor : Theme.primaryColor
                    truncationMode: TruncationMode.Fade
                }
                Label {
                    width: parent.width
                    //% "From %1"
                    text: qsTrId("lautta-trash-from").arg(model.folderName)
                    color: item.highlighted ? Theme.secondaryHighlightColor : Theme.secondaryColor
                    font.pixelSize: Theme.fontSizeExtraSmall
                    truncationMode: TruncationMode.Fade
                }
            }
            Label {
                id: remaining

                objectName: "trashDaysLeft"
                anchors {
                    right: parent.right
                    rightMargin: Theme.horizontalPageMargin
                    verticalCenter: parent.verticalCenter
                }
                color: page.soon(model.daysLeft) ? page.soonColor
                                                 : item.highlighted ? Theme.secondaryHighlightColor : Theme.secondaryColor
                font.pixelSize: Theme.fontSizeSmall
                text: page.daysLeftText(model.daysLeft)
            }
        }
    }

    Component {
        id: gridRow

        ListItem {
            id: row

            readonly property int first: model.first
            readonly property int end: model.first + model.count
            property var held: ({ "trashId": -1, "folderUri": "" })

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

            objectName: "trashGridRow"
            contentHeight: cells.height
            menu: Component {
                ContextMenu {
                    MenuItem {
                        text: qsTrId("lautta-trash-restore")
                        onClicked: trash.restore(row.held.trashId)
                    }
                    MenuItem {
                        text: qsTrId("lautta-trash-delete-now")
                        onClicked: page.deleteNow(row.held.trashId)
                    }
                    MenuItem {
                        text: qsTrId("lautta-trash-show-folder")
                        onClicked: page.showFolder(row.held.folderUri)
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

                    readonly property bool lit: highlighted || (row.menuOpen && row.held.trashId === model.trashId)
                    readonly property real gap: gridMetrics.gap

                    objectName: "trashGridTile"
                    x: (index - row.first) * width
                    width: page.cellWidth
                    height: cells.height
                    implicitHeight: gridMetrics.cellHeight(width, caption.implicitHeight + left.height)
                    onPressAndHold: openMenu()

                    function openMenu() {
                        row.held = { "trashId": model.trashId, "folderUri": model.folderUri }
                        row.openMenu()
                    }

                    Item {
                        id: tile

                        x: cell.gap
                        y: cell.gap
                        width: cell.width - 2 * cell.gap
                        height: width

                        Rectangle {
                            objectName: "trashTileBackground"
                            anchors.fill: parent
                            visible: !tileIcon.thumbnailReady
                            color: Theme.rgba(page.soon(model.daysLeft) ? page.soonColor : Theme.highlightBackgroundColor,
                                              Theme.highlightBackgroundOpacity / 3)
                        }
                        FileIcon {
                            id: tileIcon

                            anchors.fill: parent
                            size: tile.width
                            glyphSize: gridMetrics.glyphSize(tile.width)
                            category: App.categoryOf(model.name)
                            isDir: model.isDir
                            thumbnailSource: model.thumbnailSource
                            mimeType: model.mimeType
                            glyphColor: page.soon(model.daysLeft) ? page.soonColor : Theme.primaryColor
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
                        name: model.name
                        emphasised: cell.lit
                        pressed: cell.lit
                    }
                    Label {
                        id: left

                        objectName: "trashDaysLeft"
                        anchors {
                            top: caption.bottom
                            left: tile.left
                            right: tile.right
                        }
                        horizontalAlignment: Text.AlignHCenter
                        truncationMode: TruncationMode.Elide
                        font.pixelSize: Theme.fontSizeTiny
                        color: page.soon(model.daysLeft) ? page.soonColor
                                                         : cell.lit ? Theme.secondaryHighlightColor : Theme.secondaryColor
                        text: page.daysLeftText(model.daysLeft)
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
