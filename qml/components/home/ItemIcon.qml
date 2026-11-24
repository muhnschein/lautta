// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Lautta 1.0
import "../"
import "../FileIcons.js" as FileIcons

AppIcon {
    property string name
    property bool isDir
    property bool highlightedItem

    readonly property string category: isDir ? "folder" : App.categoryOf(name)

    icon: FileIcons.glyph(category, isDir, iconSize)
    highlighted: highlightedItem
}
