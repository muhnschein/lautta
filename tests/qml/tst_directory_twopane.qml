// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import "../../qml/pages"

ApplicationWindow {
    initialPage: Component {
        TwoPanePage {
            leftUri: "lautta://user-documents/"
            rightUri: "lautta://user-downloads/"
        }
    }
}
