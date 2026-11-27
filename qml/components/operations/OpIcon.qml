// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import ".."
import "../FileIcons.js" as FileIcons

AppIcon {
    property string category: "file"
    property bool isDir

    icon: FileIcons.glyph(category, isDir, iconSize)
}
