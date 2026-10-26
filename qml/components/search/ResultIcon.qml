// SPDX-License-Identifier: LGPL-2.1-or-later
// The icon of a search row: the app's own glyph by category
// (the core's FileCategory icon name, `App.categoryOf`; FileIcons.js, UI-2).
import QtQuick 2.6
import "../"
import "../FileIcons.js" as FileIcons

AppIcon {
    property string category
    property bool isDir

    icon: FileIcons.glyph(category, isDir, iconSize)
}
