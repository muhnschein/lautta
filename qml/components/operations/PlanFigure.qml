// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0

Column {
    id: figure

    property string value
    property string label

    Label {
        objectName: "figureValue"
        width: figure.width
        horizontalAlignment: Text.AlignHCenter
        fontSizeMode: Text.HorizontalFit
        minimumPixelSize: Theme.fontSizeSmall
        font.pixelSize: Theme.fontSizeLarge
        font.family: Theme.fontFamilyHeading
        text: figure.value
    }
    Label {
        objectName: "figureLabel"
        width: figure.width
        horizontalAlignment: Text.AlignHCenter
        truncationMode: TruncationMode.Fade
        color: Theme.secondaryColor
        font.pixelSize: Theme.fontSizeSmall
        text: figure.label
    }
}
