// SPDX-License-Identifier: LGPL-2.1-or-later
// Instantiates the app-wide handlers of the operations area.
import QtQuick 2.6
import Sailfish.Silica 1.0
import "../../qml/components"

ApplicationWindow {
    initialPage: Component {
        Page { }
    }
    OperationsHandlers { shareEnabled: false }
}
