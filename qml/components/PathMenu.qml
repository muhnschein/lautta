// SPDX-License-Identifier: LGPL-2.1-or-later Works as a stock context menu (tap, or hold and slide
// to a row): Silica's own opens only from a list item, this one from the header.
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "FileIcons.js" as FileIcons
import "directory/PathFold.js" as PathFold

Item {
    id: menu

    property var pathModel
    property string currentUri
    property bool open
    // Folder picker: last row Locations (navigate("")), Go to the only action.
    property bool picking
    // Folders in two columns, read down then across.
    property bool wide
    property real maxHeight: -1

    readonly property var ancestors: {
        try {
            return JSON.parse(pathModel ? pathModel.ancestorsJson : "[]")
        } catch (e) {
            return []
        }
    }
    readonly property var entries: ancestors.concat([{
        "uri": "",
        "home": true,
        "icon": "image://theme/icon-m-home",
        //% "Locations"
        "name": picking ? qsTrId("lautta-dir-path-locations")
                          //% "Browse"
                        : qsTrId("lautta-dir-path-browse")
    }])
    readonly property var actions: {
        //% "Go to"
        var goTo = { "id": "goto", "icon": "image://theme/icon-m-edit", "text": qsTrId("lautta-dir-path-goto") }
        if (picking)
            return [goTo]
        return [
            //% "Copy address"
            { "id": "copy", "icon": "image://theme/icon-m-link", "text": qsTrId("lautta-dir-path-copy-address") },
            goTo,
            //% "Favourite"
            { "id": "favourite", "icon": "image://theme/icon-m-favorite", "text": qsTrId("lautta-dir-path-favourite") }
        ]
    }
    // Row under the finger while a press-and-hold on the header goes on; release opens it.
    property Item _dragRow: null
    readonly property var _columns: PathFold.columns(entries, wide ? 2 : 1)
    readonly property real _fullHeight: maxHeight < 0 ? content.height : Math.min(content.height, maxHeight)

    signal navigate(string uri)
    signal browse
    signal goToRequested
    signal favouriteRequested

    function close() {
        open = false
    }

    function _activate(entry) {
        close()
        if (entry.home && !picking)
            browse()
        else
            navigate(entry.uri)
    }

    // Press-and-hold on the header, finger moves on: as Silica's ContextMenu, row under it lights up.
    function track(item, x, y) {
        var p = mapFromItem(item, x, y)
        _dragRow = open && p.y >= 0 && p.y < _fullHeight ? _rowAt(p.x, p.y) : null
    }

    function release() {
        var row = _dragRow
        _dragRow = null
        if (row)
            row.clicked(null)
    }

    function cancelTrack() {
        _dragRow = null
    }

    function _rowAt(x, y) {
        var item = scroller.contentItem
        var p = mapToItem(item, x, y)
        while (item) {
            if (item.objectName === "pathMenuEntry" || item.objectName === "pathMenuAction")
                return item
            var child = item.childAt(p.x, p.y)
            if (child)
                p = item.mapToItem(child, p.x, p.y)
            item = child
        }
        return null
    }

    function _trigger(actionId) {
        close()
        switch (actionId) {
        case "copy":
            Clipboard.text = App.displayAddress(currentUri)
            break
        case "goto":
            goToRequested()
            break
        case "favourite":
            favouriteRequested()
            break
        }
    }

    width: parent ? parent.width : 0
    height: open ? _fullHeight : 0
    visible: open || height > 0
    clip: true

    Behavior on height { NumberAnimation { duration: 150; easing.type: Easing.InOutQuad } }

    Rectangle {
        anchors.fill: parent
        color: Theme.highlightBackgroundColor
        opacity: Theme.highlightBackgroundOpacity
    }

    SilicaFlickable {
        id: scroller

        objectName: "pathMenuScroller"
        anchors.fill: parent
        contentHeight: content.height

        // Placed by hand, not by Column and Row: those place only on polish.
        Item {
            id: content

            width: parent.width
            height: folders.height + actionsBox.height

            Item {
                id: folders

                width: parent.width
                height: menu._columns.reduce(function (most, c) { return Math.max(most, c.length) }, 0)
                        * Theme.itemSizeSmall

                Repeater {
                    id: columns

                    model: menu._columns

                    Item {
                        x: index * width
                        width: content.width / columns.count
                        height: folders.height

                        Repeater {
                            model: modelData

                            BackgroundItem {
                                id: entry

                                objectName: "pathMenuEntry"
                                y: index * height
                                width: parent.width
                                height: Theme.itemSizeSmall
                                highlighted: down || menu._dragRow === entry
                                onClicked: menu._activate(modelData)

                                AppIcon {
                                    id: entryIcon

                                    anchors {
                                        left: parent.left
                                        leftMargin: Theme.horizontalPageMargin
                                        verticalCenter: parent.verticalCenter
                                    }
                                    icon: FileIcons.sized(modelData.icon, iconSize)
                                    highlighted: parent.highlighted
                                }
                                Label {
                                    anchors {
                                        left: entryIcon.right
                                        leftMargin: Theme.paddingLarge
                                        right: parent.right
                                        rightMargin: Theme.horizontalPageMargin
                                        verticalCenter: parent.verticalCenter
                                    }
                                    text: modelData.name
                                    truncationMode: TruncationMode.Fade
                                    color: parent.highlighted ? Theme.highlightColor : Theme.primaryColor
                                }
                            }
                        }
                    }
                }
            }

            Rectangle {
                id: actionsBox

                y: folders.height
                width: parent.width
                height: menu.actions.length * Theme.itemSizeSmall
                color: Theme.rgba(Theme.highlightDimmerColor, Theme.opacityHigh)

                Repeater {
                    model: menu.actions

                    BackgroundItem {
                        id: action

                        objectName: "pathMenuAction"
                        y: index * height
                        width: parent.width
                        height: Theme.itemSizeSmall
                        highlighted: down || menu._dragRow === action
                        onClicked: menu._trigger(modelData.id)

                        AppIcon {
                            id: actionIcon

                            anchors {
                                left: parent.left
                                leftMargin: Theme.horizontalPageMargin
                                verticalCenter: parent.verticalCenter
                            }
                            icon: modelData.icon
                            highlighted: parent.highlighted
                        }
                        Label {
                            anchors {
                                left: actionIcon.right
                                leftMargin: Theme.paddingLarge
                                right: parent.right
                                rightMargin: Theme.horizontalPageMargin
                                verticalCenter: parent.verticalCenter
                            }
                            text: modelData.text
                            truncationMode: TruncationMode.Fade
                            color: parent.highlighted ? Theme.highlightColor : Theme.primaryColor
                        }
                    }
                }
            }
        }
    }
}
