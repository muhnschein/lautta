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
            questionId: "q2"
            kind: "keyboard-interactive"
            details: ({ "name": "Two-factor", "instruction": "Enter the code", "prompts": "Code: \nPIN: " })
        }
    }
}
