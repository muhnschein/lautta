// SPDX-License-Identifier: LGPL-2.1-or-later
// The remorse of a rename on the row of the renamed item (SPEC OPS-9, board
// DirectoryRemorse); one sits in every list row and grid cell of the folder
// view. The listing makes rows anew when it changes (the rename's own
// refresh does, so do a reset and a switch between list and grid): a row
// made while the remorse runs shows the rest of it. `owner` is the
// DirectoryView, `row` the row, `uri` its item.
import QtQuick 2.6

Item {
    id: hook

    property var owner
    property Item row
    property string uri
    readonly property bool renamedHere: !!owner && !!owner.renamed && owner.renamed.uri === uri
    // What DirectoryView.showRenameRemorse returned for this row.
    property var shown: null

    // Once per rename: the change handler and Component.onCompleted can
    // both see it as the row is made.
    function offer() {
        if (renamedHere && row && (!shown || shown.rename !== owner.renamed))
            shown = owner.showRenameRemorse(row)
    }

    visible: false
    onRenamedHereChanged: offer()
    Component.onCompleted: offer()
    // Runs before the row's remorse item goes with the row (Qt deletes the
    // row later): its ending then is not the remorse running out.
    Component.onDestruction: {
        if (shown)
            shown.orphaned = true
    }
}
