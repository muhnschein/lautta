// SPDX-License-Identifier: LGPL-2.1-or-later
// Host test of the one place that decides whether a delete goes to the Trash
// or is permanent (OPS-8), Operations.deletesPermanently: the folder view,
// the selection bar and the image viewer all ask it before they delete.
import QtQuick 2.6
import Lautta 1.0

QtObject {
    function check(cond, what) {
        if (!cond)
            console.error("FAILED: " + what)
    }

    function permanent(uris) {
        return Operations.deletesPermanently(JSON.stringify(uris))
    }

    Component.onCompleted: {
        // The registry only knows the user folders when the test home is
        // visible to the process (not under the SDK's sb2 sandbox).
        var known = App.locationName("lautta://user-documents/") === "Documents"
        var doc = "lautta://user-documents/Uni/thesis.odt"
        var server = "lautta://nv-1/srv/photo.jpg"

        check(App.setting("recently_deleted") === true, "the Trash is on by default")
        check(!known || !permanent([doc]), "this device's folders delete to the Trash")
        check(!known || !permanent([doc, "lautta://user-downloads/a"]), "several of them too")
        check(permanent([server]), "servers delete permanently")
        check(permanent([doc, server]), "one permanent item makes the delete permanent")
        check(!permanent([]), "nothing to delete is nothing permanent")
        check(permanent("junk") && Operations.deletesPermanently("{}"), "junk asks first")

        check(App.setSetting("recently_deleted", JSON.stringify(false)), "the Trash switched off")
        check(permanent([doc]), "with the Trash off every delete is permanent")
        App.setSetting("recently_deleted", JSON.stringify(true))
        check(!known || !permanent([doc]), "and back on")
    }
}
