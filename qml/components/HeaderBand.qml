// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0

Rectangle {
    objectName: "headerBand"
    width: parent ? parent.width : 0
    height: childrenRect.height
    color: Theme.rgba(Theme.highlightBackgroundColor, 0.12)
}
