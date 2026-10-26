// SPDX-License-Identifier: LGPL-2.1-or-later
// A medium icon: a theme icon ("image://theme/icon-m-…"), or one of the
// app's SVGs in qml/icons/ by name: its file-type glyphs (FileIcons.js) and
// icons the theme lacks (UI-2). Both are tinted with the ambience's primary colour: uncoloured theme glyphs
// are light and vanish on a light ambience until highlighted.
import QtQuick 2.6
import Sailfish.Silica 1.0

HighlightImage {
    // "image://theme/icon-m-share" or an app icon name such as "cut".
    property string icon
    property bool small
    // The picture's side, `small` or medium unless set (the grid).
    property real iconSize: small ? Theme.iconSizeSmall : Theme.iconSizeMedium
    readonly property bool themed: icon.indexOf(":") >= 0

    source: icon.length === 0 ? "" : (themed ? icon : Qt.resolvedUrl("../icons/" + icon + ".svg"))
    sourceSize.width: iconSize
    sourceSize.height: iconSize
    width: sourceSize.width
    height: sourceSize.height
    color: Theme.primaryColor
    highlightColor: Theme.highlightColor
}
