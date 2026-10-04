// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import "../../qml/cover"

ApplicationWindow {
    initialPage: Component {
        Page {
            CoverBackground {
                anchors.fill: parent
                TransferCover { anchors.fill: parent }
            }
        }
    }
}
