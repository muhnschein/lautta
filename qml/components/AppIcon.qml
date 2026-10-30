// SPDX-License-Identifier: LGPL-2.1-or-later Tinted with ambience primary colour: uncoloured theme
// glyphs vanish on a light ambience.
import QtQuick 2.6
import Sailfish.Silica 1.0

HighlightImage {
    property string icon
    property bool small
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
