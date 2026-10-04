// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../../qml/dialogs"

ApplicationWindow {
    initialPage: Component {
        ServerQuestionDialog {
            questionId: "q3"
            kind: "insecure-consent"
            details: ({ "host": "old.example", "url": "ftp://old.example/" })
        }
    }
}
