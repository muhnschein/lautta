// SPDX-License-Identifier: LGPL-2.1-or-later
// Instantiates RecentlyDeletedPage (empty on a fresh home).
import QtQuick 2.6
import Sailfish.Silica 1.0
import "../../qml/pages"

ApplicationWindow {
    initialPage: Component {
        RecentlyDeletedPage { }
    }
}
