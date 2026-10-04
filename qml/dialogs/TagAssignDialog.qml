// SPDX-License-Identifier: LGPL-2.1-or-later
// Tags for the chosen items (ORG-3): switch tags on or off for all of them,
// or make a new tag. `uris` is a JSON list of URI strings. A tag that only
// some of the items carry shows "n of m items"; tapping it gives it to all.
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../components/browse"

Dialog {
    id: dialog

    property string uris: "[]"
    readonly property int itemCount: JSON.parse(uris).length
    // Tag id -> how many of the items carry it now, and the wish of the user.
    property var carried: ({})
    property var wanted: ({})
    property string newColour: ""

    allowedOrientations: Orientation.All

    function stateOf(tagId) {
        if (wanted[tagId] !== undefined)
            return wanted[tagId] ? "all" : "none"
        var n = carried[tagId] || 0
        return n === 0 ? "none" : (n >= itemCount ? "all" : "some")
    }

    function toggle(tagId) {
        var next = JSON.parse(JSON.stringify(wanted))
        next[tagId] = stateOf(tagId) !== "all"
        wanted = next
    }

    onAccepted: {
        var add = []
        var remove = []
        for (var id in wanted) {
            var before = carried[id] || 0
            if (wanted[id] && before < itemCount)
                add.push(parseInt(id))
            if (!wanted[id] && before > 0)
                remove.push(parseInt(id))
        }
        tags.apply(uris, JSON.stringify(add), JSON.stringify(remove), newField.text.trim(),
                   newColour.length > 0 ? newColour : "#4fa3e5")
    }

    TagsModel {
        id: tags
        onUsageReady: {
            var map = {}
            var list = JSON.parse(json)
            for (var i = 0; i < list.length; ++i)
                map[list[i].id] = list[i].count
            dialog.carried = map
        }
        Component.onCompleted: {
            reload()
            usageFor(dialog.uris)
        }
    }

    SilicaFlickable {
        anchors.fill: parent
        contentHeight: column.height + Theme.paddingLarge

        Column {
            id: column
            width: parent.width

            DialogHeader {
                //% "Done"
                acceptText: qsTrId("lautta-tags-done")
                //% "Tags for %n items"
                title: qsTrId("lautta-tags-for-items", dialog.itemCount)
            }

            Repeater {
                model: tags
                delegate: ListItem {
                    id: tagRow
                    readonly property string tagState: dialog.stateOf(model.itemId)

                    contentHeight: Theme.itemSizeMedium
                    onClicked: dialog.toggle(model.itemId)

                    Rectangle {
                        anchors {
                            left: parent.left
                            leftMargin: Theme.horizontalPageMargin
                            verticalCenter: parent.verticalCenter
                        }
                        width: Theme.iconSizeSmall - Theme.paddingSmall
                        height: width
                        radius: width / 2
                        color: model.colour
                    }
                    Column {
                        anchors {
                            left: parent.left
                            leftMargin: Theme.horizontalPageMargin + Theme.iconSizeMedium
                            right: tagSwitch.left
                            verticalCenter: parent.verticalCenter
                        }
                        Label {
                            width: parent.width
                            text: model.name
                            color: tagRow.tagState === "none" ? Theme.primaryColor : Theme.highlightColor
                            truncationMode: TruncationMode.Fade
                        }
                        Label {
                            visible: tagRow.tagState === "some"
                            width: parent.width
                            //% "%1 of %2 items"
                            text: qsTrId("lautta-tags-partial").arg(dialog.carried[model.itemId] || 0).arg(dialog.itemCount)
                            font.pixelSize: Theme.fontSizeExtraSmall
                            color: Theme.secondaryColor
                        }
                    }
                    Switch {
                        id: tagSwitch
                        anchors {
                            right: parent.right
                            rightMargin: Theme.paddingLarge
                            verticalCenter: parent.verticalCenter
                        }
                        automaticCheck: false
                        checked: tagRow.tagState !== "none"
                        // A tag only some of the items carry is half on.
                        opacity: tagRow.tagState === "some" ? 0.6 : 1
                        onClicked: dialog.toggle(model.itemId)
                    }
                }
            }

            SectionHeader {
                //% "New tag"
                text: qsTrId("lautta-tags-new")
            }
            TextField {
                id: newField
                width: parent.width
                //% "New tag"
                label: qsTrId("lautta-tags-new")
                //% "Name"
                placeholderText: qsTrId("lautta-tags-name")
                EnterKey.iconSource: "image://theme/icon-m-enter-accept"
                EnterKey.onClicked: dialog.accept()
            }
            ColourPicker {
                visible: newField.text.length > 0
                colour: dialog.newColour
                onPicked: dialog.newColour = colour
            }
            Label {
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * x
                wrapMode: Text.Wrap
                horizontalAlignment: Text.AlignRight
                //% "Tags are kept by Lautta and follow moves made in Lautta."
                text: qsTrId("lautta-tags-note")
                font.pixelSize: Theme.fontSizeExtraSmall
                color: Theme.secondaryColor
            }
        }

        VerticalScrollDecorator { }
    }
}
