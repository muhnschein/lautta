// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../../qml/pages"

ApplicationWindow {
    id: test
    property bool reachedEnd

    SearchModel {
        id: filterModel
        rootUri: "lautta://user-documents/"
        query: "thesis*"
        matchMode: "glob"
    }

    initialPage: Component {
        SearchFiltersPage {
            Component.onCompleted: test.reachedEnd = true
            searchModel: filterModel
        }
    }
}
