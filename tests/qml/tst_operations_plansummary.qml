// SPDX-License-Identifier: LGPL-2.1-or-later
// Instantiates PlanSummaryDialog with a large plan on the real Silica.
import QtQuick 2.6
import Sailfish.Silica 1.0
import "../../qml/dialogs"

ApplicationWindow {
    initialPage: Component {
        PlanSummaryDialog {
            planId: 1
            summary: ({
                "kind": "copy", "files": 1284, "dirs": 37, "bytes": 4500000000, "conflicts": 12, "renamed": 3,
                "destination": "lautta://nas/srv/photos", "destinationName": "NAS › srv › photos",
                "freeBytes": 812000000000, "permissionsSupported": true,
                "renames": [ { "from": "aux.txt", "to": "aux_.txt" }, { "from": "notes: draft.md", "to": "notes- draft.md" } ],
                "options": { "verifyChecksums": false, "preserveMtime": true, "preserveMode": false, "suggestedNames": true }
            })
        }
    }
}
