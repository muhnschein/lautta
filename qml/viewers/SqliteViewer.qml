// SPDX-License-Identifier: LGPL-2.1-or-later
// SQLite viewer (PRV-4, board SqliteViewer): the tables of a local database
// and the first rows of one, read-only. Remote files are copied first
// (dialogs/OpenRemoteDialog.qml).
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../components/viewers"
import "../components/ErrorText.js" as ErrorText

Page {
    id: page

    property string uri

    readonly property string fileName: App.nameOf(uri)
    readonly property var tables: db.tablesJson.length > 0 ? JSON.parse(db.tablesJson) : []
    readonly property var columns: db.columnsJson.length > 0 ? JSON.parse(db.columnsJson) : []
    readonly property real columnWidth: Theme.itemSizeHuge
    readonly property real rowsWidth: Math.max(width, columns.length * columnWidth + 2 * Theme.horizontalPageMargin)

    allowedOrientations: Orientation.All

    SqliteModel {
        id: db
        uri: page.uri
    }

    ExternalActions {
        id: external
        uri: page.uri
    }

    SilicaListView {
        id: view

        anchors.fill: parent
        model: db
        clip: true
        flickableDirection: Flickable.HorizontalAndVerticalFlick
        contentWidth: page.rowsWidth

        PullDownMenu {
            MenuItem {
                //% "Open with"
                text: qsTrId("lautta-viewers-open-with")
                onClicked: external.openWith()
            }
        }

        header: Column {
            width: page.width

            PageHeader {
                title: page.fileName
                description: db.errorKind !== "" ? "" :
                             //% "%n tables · read-only"
                             qsTrId("lautta-viewers-sqlite-tables", page.tables.length)
            }

            ComboBox {
                visible: page.tables.length > 0
                //% "Table"
                label: qsTrId("lautta-viewers-sqlite-table")
                value: db.table
                menu: ContextMenu {
                    Repeater {
                        model: page.tables
                        MenuItem {
                            text: modelData.name
                            onClicked: db.selectTable(modelData.name)
                        }
                    }
                }
            }

            Label {
                x: Theme.horizontalPageMargin
                visible: page.tables.length > 0
                color: Theme.secondaryHighlightColor
                font.pixelSize: Theme.fontSizeSmall
                text: {
                    var total = db.totalRows >= 0 ? Number(db.totalRows).toLocaleString(Qt.locale(), "f", 0) : "?"
                    if (!db.totalRowsExact)
                        //% "more than %1 rows · showing first %2"
                        return qsTrId("lautta-viewers-sqlite-rows-more").arg(total).arg(db.count)
                    //% "%1 rows · showing first %2"
                    return qsTrId("lautta-viewers-sqlite-rows").arg(total).arg(db.count)
                }
            }

            Row {
                x: Theme.horizontalPageMargin
                height: Theme.itemSizeExtraSmall
                Repeater {
                    model: page.columns
                    Label {
                        width: page.columnWidth
                        height: parent.height
                        verticalAlignment: Text.AlignVCenter
                        color: Theme.highlightColor
                        font.pixelSize: Theme.fontSizeExtraSmall
                        truncationMode: TruncationMode.Fade
                        text: modelData
                    }
                }
            }
        }

        delegate: Row {
            property var cells: JSON.parse(cellsJson)

            x: Theme.horizontalPageMargin
            height: Theme.itemSizeExtraSmall * 0.8

            Repeater {
                model: cells
                Label {
                    width: page.columnWidth
                    height: parent.height
                    verticalAlignment: Text.AlignVCenter
                    font.pixelSize: Theme.fontSizeExtraSmall
                    truncationMode: TruncationMode.Fade
                    text: modelData
                }
            }
        }

        footer: Item {
            width: page.width
            height: db.hasMore ? Theme.itemSizeLarge : Theme.paddingLarge

            Button {
                anchors.centerIn: parent
                visible: db.hasMore
                enabled: !db.loading
                //% "Show more rows"
                text: qsTrId("lautta-viewers-sqlite-more")
                onClicked: db.loadMore()
            }
        }

        ViewPlaceholder {
            enabled: db.errorKind !== "" || (!db.loading && page.tables.length === 0)
            text: db.errorKind !== ""
                  ? ErrorText.message(db.errorKind, { "item": page.fileName, "location": App.locationName(page.uri) })
                  //% "No tables"
                  : qsTrId("lautta-viewers-sqlite-none")
        }

        VerticalScrollDecorator { }
    }

    BusyIndicator {
        anchors.centerIn: parent
        size: BusyIndicatorSize.Large
        running: db.loading && db.count === 0
    }
}
