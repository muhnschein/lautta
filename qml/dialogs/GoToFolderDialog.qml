// SPDX-License-Identifier: LGPL-2.1-or-later Opens with the address selected: typing replaces it.
// A name finds folders here, recent, favourite and listed before; a path drills down.
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../components"

Dialog {
    id: dialog

    property string folderUri
    property string folderName
    property string startText
    // Row tapped; else the top row, or a server path not listed yet.
    property string pickedUri
    readonly property string chosenUri: pickedUri.length > 0 ? pickedUri : (fresh && hits.count > 0 ? hits.firstUri : typedUri)
    readonly property bool edited: field.text !== startText
    readonly property bool fresh: hits.text === field.text && !typing.running
    readonly property string typedUri: {
        if (!edited || !hits.isPath || !fresh)
            return ""
        var u = resolver.resolve(field.text)
        // Device folders were looked at: not among the hits = not there.
        return u.length > 0 && !App.isLocal(u) ? u : ""
    }
    readonly property bool noFolder: edited && fresh && field.text.trim().length > 0 && hits.count === 0
                                     && hits.isPath && typedUri.length === 0
    readonly property bool keyboardOnly: isLandscape && Qt.inputMethod.visible

    allowedOrientations: Orientation.All
    canAccept: chosenUri.length > 0

    onStatusChanged: {
        if (status === PageStatus.Active && !edited) {
            field.forceActiveFocus()
            field.selectAll()
        }
    }

    function pick(uri) {
        pickedUri = uri
        accept()
    }

    function choose(row) {
        if (row.source === "in") {
            field.text = row.address
            field.cursorPosition = field.text.length
            field.forceActiveFocus()
        } else {
            pick(row.uri)
        }
    }

    function searchHere() {
        pageStack.replace(Qt.resolvedUrl("../pages/SearchPage.qml"), { "rootUri": folderUri, "query": field.text.trim() })
    }

    GotoModel {
        id: hits

        folder: dialog.folderUri
    }

    PathModel {
        id: resolver

        uri: dialog.folderUri
    }

    Timer {
        id: typing

        interval: 150
        onTriggered: hits.query(dialog.edited ? field.text : "")
    }

    SilicaFlickable {
        id: flick

        objectName: "goToScroller"
        anchors.fill: parent
        contentHeight: Math.max(column.height, height + 1)
        onDraggingChanged: if (dragging) field.focus = false

        Column {
            id: column

            width: parent.width

            DialogHeader {
                //% "Go"
                acceptText: qsTrId("lautta-goto-accept")
            }

            TextField {
                id: field

                objectName: "goToField"
                width: parent.width
                text: dialog.startText
                errorHighlight: dialog.noFolder
                label: {
                    if (dialog.noFolder)
                        //% "No such folder"
                        return qsTrId("lautta-goto-none")
                    if (dialog.keyboardOnly && hits.count > 0 && dialog.fresh)
                        //% "%1 · Enter opens it"
                        return qsTrId("lautta-goto-best").arg(hits.firstName)
                    //% "Folder or path"
                    return qsTrId("lautta-goto-label")
                }
                placeholderText: qsTrId("lautta-goto-label")
                inputMethodHints: Qt.ImhNoAutoUppercase | Qt.ImhNoPredictiveText | Qt.ImhUrlCharactersOnly
                EnterKey.enabled: dialog.canAccept
                EnterKey.iconSource: "image://theme/icon-m-enter-accept"
                EnterKey.onClicked: dialog.accept()
                onTextChanged: typing.restart()
            }

            Row {
                width: parent.width
                visible: !dialog.keyboardOnly

                Repeater {
                    id: columns

                    model: dialog.isLandscape ? 2 : 1

                    Column {
                        readonly property var range: {
                            var per = Math.ceil(hits.count / columns.count)
                            return [index * per, (index + 1) * per]
                        }

                        width: column.width / columns.count

                        Repeater {
                            model: hits

                            GoToRow {
                                width: parent.width
                                visible: index >= parent.range[0] && index < parent.range[1]
                                height: visible ? implicitHeight : 0
                                hit: model
                                typed: field.text.trim()
                                //% "In %1"
                                section: index === 1 && hits.firstSource === "go" ? qsTrId("lautta-goto-in").arg(hits.firstName) : ""
                                onClicked: dialog.choose(model)
                            }
                        }

                        BackgroundItem {
                            objectName: "goToSearch"
                            width: parent.width
                            height: visible ? Theme.itemSizeSmall : 0
                            visible: index === columns.count - 1 && !hits.isPath && dialog.fresh
                                     && field.text.trim().length > 0
                            onClicked: dialog.searchHere()

                            AppIcon {
                                id: searchIcon

                                anchors {
                                    left: parent.left
                                    leftMargin: Theme.horizontalPageMargin
                                    verticalCenter: parent.verticalCenter
                                }
                                icon: "image://theme/icon-m-search"
                                highlighted: parent.highlighted
                            }
                            Label {
                                anchors {
                                    left: searchIcon.right
                                    leftMargin: Theme.paddingLarge
                                    right: parent.right
                                    rightMargin: Theme.horizontalPageMargin
                                    verticalCenter: parent.verticalCenter
                                }
                                //% "Search “%1” in %2"
                                text: qsTrId("lautta-goto-search").arg(field.text.trim()).arg(dialog.folderName)
                                truncationMode: TruncationMode.Fade
                                color: parent.highlighted ? Theme.highlightColor : Theme.primaryColor
                            }
                        }
                    }
                }
            }
        }

        VerticalScrollDecorator { }
    }
}
