// SPDX-License-Identifier: LGPL-2.1-or-later
// App-wide reactions of the browse area: the bridge's questions (NVB-6) open
// their dialog, a connected ad-hoc server opens, failures of bridge calls are
// told, and the app looks for the bridge again when it comes to the
// foreground (NVB-1).
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "ErrorText.js" as ErrorText

Item {
    Connections {
        target: Qt.application
        onStateChanged: {
            var active = Qt.application.state === Qt.ApplicationActive
            Bridge.setForeground(active)
            if (active)
                Bridge.poke()
        }
    }

    Connections {
        target: Bridge

        onQuestion: {
            pageStack.push(Qt.resolvedUrl("../dialogs/ServerQuestionDialog.qml"), {
                               "questionId": questionId,
                               "kind": kind,
                               "details": JSON.parse(detailsJson)
                           })
        }

        onAdhocConnected: {
            pageStack.push(Qt.resolvedUrl("../pages/DirectoryPage.qml"), { "uri": locationUri })
        }

        onFailed: {
            notice.text = ErrorText.message(kind, { "location": subject })
            notice.show()
        }
    }

    Notice {
        id: notice
        duration: Notice.Long
    }
}
