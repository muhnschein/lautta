// SPDX-License-Identifier: LGPL-2.1-or-later Listing makes rows anew (rename refresh, reset,
// list/grid switch): a row made while remorse runs shows the rest of it.
import QtQuick 2.6

Item {
    id: hook

    property var owner
    property Item row
    property string uri
    readonly property bool renamedHere: !!owner && !!owner.renamed && owner.renamed.uri === uri
    property var shown: null

    // Change handler and Component.onCompleted can both see it as row is made.
    function offer() {
        if (renamedHere && row && (!shown || shown.rename !== owner.renamed))
            shown = owner.showRenameRemorse(row)
    }

    visible: false
    onRenamedHereChanged: offer()
    Component.onCompleted: offer()
    // Runs before row's remorse item goes with row (Qt deletes row later): that ending is not
    // remorse running out.
    Component.onDestruction: {
        if (shown)
            shown.orphaned = true
    }
}
