// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6

Item {
    id: hook

    property var owner
    property Item row
    property string uri
    readonly property bool renamedHere: !!owner && !!owner.renamed && owner.renamed.uri === uri
    property var shown: null

    function offer() {
        if (renamedHere && row && (!shown || shown.rename !== owner.renamed))
            shown = owner.showRenameRemorse(row)
    }

    visible: false
    onRenamedHereChanged: offer()
    Component.onCompleted: offer()
    Component.onDestruction: {
        if (shown)
            shown.orphaned = true
    }
}
