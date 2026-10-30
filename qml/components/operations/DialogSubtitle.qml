// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0

Label {
    x: Theme.horizontalPageMargin
    width: (parent ? parent.width : 0) - 2 * Theme.horizontalPageMargin
    horizontalAlignment: Text.AlignRight
    color: Theme.secondaryHighlightColor
    font.pixelSize: Theme.fontSizeSmall
    truncationMode: TruncationMode.Fade
    visible: text.length > 0
}
