// SPDX-License-Identifier: LGPL-2.1-or-later
// The glyph of a file or folder in the operations pages: the app's own glyph
// by category (FileIcons.js, UI-2).
import QtQuick 2.6
import ".."
import "../FileIcons.js" as FileIcons

AppIcon {
    // FileCategory icon name from App.categoryOf / the "category" of Info.
    property string category: "file"
    property bool isDir

    icon: FileIcons.glyph(category, isDir, iconSize)
}
