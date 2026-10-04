// SPDX-License-Identifier: LGPL-2.1-or-later
// Target test: CompareResultsPage instantiates with real Silica and the real Lautta types.
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../../qml/pages"

ApplicationWindow {

    initialPage: Component {
        CompareResultsPage {
            leftUri: "lautta://user-pictures/"; rightUri: "lautta://user-downloads/"; savePair: false
        }
    }
}
