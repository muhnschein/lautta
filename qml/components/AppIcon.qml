// SPDX-License-Identifier: LGPL-2.1-or-later
// A medium icon: a theme icon ("image://theme/icon-m-…") when the platform
// has one (UI-2), otherwise one of the app's SVGs in qml/icons/ by name
// (drawn in white and tinted here to follow the ambience).
import QtQuick 2.6
import Sailfish.Silica 1.0

HighlightImage {
    // "image://theme/icon-m-share" or an app icon name such as "cut".
    property string icon
    property bool small
    readonly property bool themed: icon.indexOf(":") >= 0

    source: icon.length === 0 ? "" : (themed ? icon : Qt.resolvedUrl("../icons/" + icon + ".svg"))
    sourceSize.width: small ? Theme.iconSizeSmall : Theme.iconSizeMedium
    sourceSize.height: small ? Theme.iconSizeSmall : Theme.iconSizeMedium
    width: sourceSize.width
    height: sourceSize.height
    color: themed ? "transparent" : Theme.primaryColor
    highlightColor: Theme.highlightColor
}
