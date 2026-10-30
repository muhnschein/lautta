// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import "FileIcons.js" as FileIcons

Column {
    id: row

    property var hit
    property string typed
    property string section
    readonly property string source: hit ? hit.source : ""
    readonly property string subtitle: {
        if (!hit)
            return ""
        var label = ""
        if (source === "recent")
            //% "Recent"
            label = qsTrId("lautta-goto-recent")
        else if (source === "favourite")
            //% "Favourite"
            label = qsTrId("lautta-goto-favourite")
        else if (source === "in")
            return ""
        return [label, hit.place].filter(function (p) { return p.length > 0 }).join(" · ")
    }

    signal clicked

    objectName: "goToRow"

    SectionHeader {
        visible: row.section.length > 0
        text: row.section
    }

    BackgroundItem {
        id: item

        width: parent.width
        height: row.subtitle.length > 0 ? Theme.itemSizeMedium : Theme.itemSizeSmall
        onClicked: row.clicked()

        AppIcon {
            id: icon

            anchors {
                left: parent.left
                leftMargin: Theme.horizontalPageMargin
                verticalCenter: parent.verticalCenter
            }
            icon: {
                switch (row.source) {
                case "go": return "image://theme/icon-m-right"
                case "recent": return "image://theme/icon-m-history"
                case "favourite": return "image://theme/icon-m-favorite"
                }
                return FileIcons.sized("file-folder", iconSize)
            }
            highlighted: item.highlighted
        }

        Column {
            id: texts

            anchors {
                left: icon.right
                leftMargin: Theme.paddingLarge
                right: parent.right
                rightMargin: Theme.horizontalPageMargin
                verticalCenter: parent.verticalCenter
            }

            Label {
                width: parent.width
                textFormat: row.source === "go" || row.source === "in" ? Text.PlainText : Text.StyledText
                text: {
                    if (!row.hit)
                        return ""
                    if (row.source === "go")
                        //% "Go to %1"
                        return qsTrId("lautta-goto-go").arg(row.hit.name)
                    if (row.source === "in")
                        return row.hit.name
                    return Theme.highlightText(row.hit.name, row.typed, Theme.highlightColor)
                }
                truncationMode: TruncationMode.Fade
                color: item.highlighted ? Theme.highlightColor : Theme.primaryColor
            }
            Label {
                width: parent.width
                visible: text.length > 0
                text: row.subtitle
                font.pixelSize: Theme.fontSizeExtraSmall
                truncationMode: TruncationMode.Fade
                color: item.highlighted ? Theme.secondaryHighlightColor : Theme.secondaryColor
            }
        }

    }
}
