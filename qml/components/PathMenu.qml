// SPDX-License-Identifier: LGPL-2.1-or-later Looks like a stock context menu: Silica's own opens
// only from a list item, this one from the header.
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
    // One row while every label fits its share; long translations get a row each.
    readonly property bool actionsInRow: {
        var share = width / actions.length
        for (var i = 0; i < actions.length; ++i) {
            if (Theme.iconSizeSmall + Theme.paddingMedium * 3 + actionMetrics.advanceWidth(actions[i].text) > share)
                return false
        }
        return true
    }

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
    height: open ? (maxHeight < 0 ? content.height : Math.min(content.height, maxHeight)) : 0
    visible: open || height > 0
    clip: true

    Behavior on height { NumberAnimation { duration: 150; easing.type: Easing.InOutQuad } }

    FontMetrics {
        id: actionMetrics

        font.pixelSize: Theme.fontSizeExtraSmall
    }

    Rectangle {
        anchors.fill: parent
        color: Theme.highlightBackgroundColor
        opacity: Theme.highlightBackgroundOpacity
    }

    SilicaFlickable {
        objectName: "pathMenuScroller"
        anchors.fill: parent
        contentHeight: content.height

        Column {
            id: content

            width: parent.width

            Row {
                width: parent.width

                Repeater {
                    id: columns

                    model: PathFold.columns(menu.entries, menu.wide ? 2 : 1)

                    Column {
                        width: content.width / columns.count

                        Repeater {
                            model: modelData

                            BackgroundItem {
                                objectName: "pathMenuEntry"
                                width: parent.width
                                height: Theme.itemSizeSmall
                                onClicked: menu._activate(modelData)

                                AppIcon {
                                    id: entryIcon

                                    anchors {
                                        left: parent.left
                                        leftMargin: Theme.horizontalPageMargin
                                        verticalCenter: parent.verticalCenter
                                    }
                                    small: true
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
                width: parent.width
                height: actionsBox.height
                color: Theme.rgba(Theme.highlightDimmerColor, Theme.opacityHigh)

                Flow {
                    id: actionsBox

                    width: parent.width

                    Repeater {
                        model: menu.actions

                        BackgroundItem {
                            objectName: "pathMenuAction"
                            width: menu.actionsInRow ? actionsBox.width / menu.actions.length : actionsBox.width
                            height: menu.actionsInRow ? Theme.itemSizeMedium : Theme.itemSizeSmall
                            onClicked: menu._trigger(modelData.id)

                            Row {
                                x: menu.actionsInRow ? (parent.width - width) / 2 : Theme.horizontalPageMargin
                                anchors.verticalCenter: parent.verticalCenter
                                spacing: menu.actionsInRow ? Theme.paddingMedium : Theme.paddingLarge

                                AppIcon {
                                    anchors.verticalCenter: parent.verticalCenter
                                    small: true
                                    icon: modelData.icon
                                    highlighted: parent.parent.highlighted
                                }
                                Label {
                                    anchors.verticalCenter: parent.verticalCenter
                                    text: modelData.text
                                    font.pixelSize: menu.actionsInRow ? Theme.fontSizeExtraSmall : Theme.fontSizeMedium
                                    color: parent.parent.highlighted ? Theme.highlightColor : Theme.primaryColor
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
