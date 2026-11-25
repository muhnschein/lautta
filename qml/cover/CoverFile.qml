// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../components"
import "../components/FileIcons.js" as FileIcons

Item {
    id: root

    property string uri
    readonly property var tile: {
        var json = uri.length > 0 ? App.tileOf(uri) : ""
        return json.length > 0 ? JSON.parse(json) : ({ "name": "", "category": "file", "mimeType": "", "thumbnailSource": "" })
    }
    property bool pictured: picture.thumbnailReady

    FileIcon {
        id: picture

        objectName: "coverPicture"
        size: parent.height
        width: parent.width
        height: parent.height
        glyphSize: FileIcons.snap(parent.width / 2)
        category: root.tile.category
        thumbnailSource: root.tile.thumbnailSource
        mimeType: root.tile.mimeType
        opacity: root.pictured ? 1 : Theme.opacityLow
    }

    Rectangle {
        objectName: "coverFade"
        visible: root.pictured
        width: parent.width
        height: header.y + header.height + Theme.paddingLarge * 2
        gradient: Gradient {
            GradientStop { position: 0; color: Theme.rgba("black", Theme.opacityOverlay) }
            GradientStop { position: 1; color: "transparent" }
        }
    }

    CoverHeader {
        id: header

        text: root.tile.name
        color: root.pictured ? Theme.lightPrimaryColor : Theme.highlightColor
    }
}
