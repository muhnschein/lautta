// SPDX-License-Identifier: LGPL-2.1-or-later
// Target test: SearchFiltersPage instantiates with real Silica and the real Lautta types.
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../../qml/pages"

ApplicationWindow {

    SearchModel {
        id: filterModel
        rootUri: "lautta://user-documents/"
        query: "thesis*"
        matchMode: "glob"
    }

    initialPage: Component {
        SearchFiltersPage {
            searchModel: filterModel
        }
    }
}
