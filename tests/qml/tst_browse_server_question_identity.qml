// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../../qml/dialogs"

ApplicationWindow {
    id: test
    property bool reachedEnd

    initialPage: Component {
        ServerQuestionDialog {
            Component.onCompleted: test.reachedEnd = true
            questionId: "q1"
            kind: "identity-unknown"
            details: ({ "host": "raspberrypi.local", "port": "22", "algorithm": "ssh-ed25519", "fingerprint": "SHA256:k3Vx9T2mQpL0aZr8uYw1cNfB6eHd4Jq7sOg5Xt+Ea0" })
        }
    }
}
