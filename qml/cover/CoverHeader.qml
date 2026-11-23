// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0

Label {
    id: header

    objectName: "coverHeader"
    x: Theme.paddingMedium
    y: Theme.paddingMedium
    width: parent ? parent.width - 2 * x : 0
    // Too long: its start shows, end fades. Measured apart: own width follows alignment.
    horizontalAlignment: metrics.width > width ? Text.AlignLeft : Text.AlignRight
    truncationMode: TruncationMode.Fade
    textFormat: Text.PlainText
    font.pixelSize: Theme.fontSizeSmall
    color: Theme.highlightColor

    TextMetrics {
        id: metrics

        font: header.font
        text: header.text
    }
}
