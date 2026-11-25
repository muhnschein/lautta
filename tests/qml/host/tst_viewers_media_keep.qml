// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import QtMultimedia 5.6 as MM
import Lautta 1.0

Item {
    id: root
    property bool reachedEnd

    readonly property int loading: 2
    readonly property int buffered: 6
    property Item page
    property QtObject keptPlayer
    property Item keptOutput

    function check(cond, what) {
        if (!cond)
            console.error("FAILED: " + what)
    }

    function settleWithNothingToWaitFor() {
        gc()
        tools.pump(50)
    }

    function until(done) {
        for (var i = 0; i < 200 && !done(); ++i)
            tools.pump(25)
    }

    ViewerTools { id: tools }
    MM.MediaPlayer { id: idlePlayer }

    Component {
        id: pageComponent
        Item {
            property alias player: statusOnlyStandIn
            property alias output: output

            QtObject {
                id: statusOnlyStandIn
                property int status
            }
            Item { id: output }
        }
    }

    Component.onCompleted: {
        page = pageComponent.createObject(root)
        page.player.status = loading
        keptPlayer = page.player
        keptOutput = page.output
        check(tools.keepWhileLoading(keptPlayer, keptOutput), "output of a loading player kept")
        check(tools.keepWhileLoading(keptPlayer, keptPlayer), "loading player kept")
        check(keptOutput.parent === null, "kept output leaves the page")
        page.destroy()
        until(function() { return page === null })
        settleWithNothingToWaitFor()
        check(page === null, "page gone")
        check(keptPlayer !== null && keptOutput !== null, "nothing deletes player and output while loading")
        keptPlayer.status = buffered
        until(function() { return keptPlayer === null && keptOutput === null })
        check(keptPlayer === null && keptOutput === null, "both go once the player leaves loading")

        page = pageComponent.createObject(root)
        page.player.status = buffered
        check(!tools.keepWhileLoading(page.player, page.output), "loaded player: nothing kept")
        check(page.output.parent === page, "output stays on its page")
        keptOutput = page.output
        page.destroy()
        until(function() { return keptOutput === null })
        check(keptOutput === null, "output goes with its page")

        check(!tools.keepWhileLoading(idlePlayer, root), "idle QtMultimedia player: nothing kept")
        check(!tools.keepWhileLoading(tools, root), "object without status: nothing kept")
        check(!tools.keepWhileLoading(null, root), "no player: nothing kept")
        reachedEnd = true
    }
}
