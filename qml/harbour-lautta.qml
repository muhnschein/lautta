// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0

ApplicationWindow {
    initialPage: Component {
        Page {
            PageHeader {
                //% "Lautta"
                title: qsTrId("lautta-app-name")
            }
        }
    }
    allowedOrientations: defaultAllowedOrientations
}
