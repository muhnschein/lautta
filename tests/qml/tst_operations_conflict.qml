// SPDX-License-Identifier: LGPL-2.1-or-later
// Instantiates ConflictDialog: a file conflict among twelve, "Keep both" preselected.
import QtQuick 2.6
import Sailfish.Silica 1.0
import "../../qml/dialogs"

ApplicationWindow {
    initialPage: Component {
        ConflictDialog {
            planId: 1
            item: 3
            name: "IMG_2041.jpg"
            folder: "NAS › srv › photos › 2026"
            position: 3
            total: 12
            defaultChoice: "KeepBoth"
            conflict: ({
                "dst_is_dir": false, "src_is_dir": false, "src_size": 3100000, "dst_size": 2800000,
                "src_mtime_ms": 1790000000000, "dst_mtime_ms": 1789000000000, "resumable": false,
                "choices": [ "KeepBoth", "Skip", "Replace", "ReplaceIfNewer" ]
            })
            Component.onCompleted: {
                if (choice !== "KeepBoth")
                    console.error("the preselected choice must be Keep both, not " + choice)
                if (!canAccept)
                    console.error("a preselected choice can be accepted")
            }
        }
    }
}
