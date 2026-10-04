// SPDX-License-Identifier: LGPL-2.1-or-later
// App-wide reactions of the operations area: plan summaries, archives opened
// as locations, failures, "Running in the background", the finished extract
// that opens its folder, the share target (INT-1) and keep-alive while an
// archive is being made.
import QtQuick 2.6
import Sailfish.Silica 1.0
import Sailfish.Share 1.0
import Nemo.KeepAlive 1.2
import Lautta 1.0
import "ErrorText.js" as ErrorText

Item {
    id: handlers

    // Off in the QML check (--qml-check): there is no session bus there for
    // the share provider to register on.
    property bool shareEnabled: Qt.application.arguments.indexOf("--qml-check") < 0

    // Resolved when used: the window publishes its instance once created.
    function stack() {
        return __silica_applicationwindow_instance.pageStack
    }

    function show(text) {
        notice.text = text
        notice.show()
    }

    function background() {
        //% "Running in the background"
        show(qsTrId("lautta-ops-background"))
    }

    Notice {
        id: notice
    }

    // Compress runs as a job of its own; keep the device awake meanwhile.
    KeepAlive {
        enabled: Operations.jobsActive > 0
    }

    Connections {
        target: Operations

        onNeedsSummary: handlers.stack().push(Qt.resolvedUrl("../dialogs/PlanSummaryDialog.qml"), {
            "planId": planId,
            "summary": JSON.parse(summaryJson)
        })
        onArchiveOpened: handlers.stack().push(Qt.resolvedUrl("../pages/DirectoryPage.qml"), { "uri": rootUri })
        onFailed: handlers.show(ErrorText.message(kind, {}))
        onStarted: handlers.background()
        onJobStarted: handlers.background()
        onJobFinished: {
            if (ok)
                //% "Archive created"
                handlers.show(qsTrId("lautta-ops-archive-created"))
        }
        onExtractFinished: {
            if (ok)
                handlers.stack().push(Qt.resolvedUrl("../pages/DirectoryPage.qml"), { "uri": destUri })
        }
    }

    // "Save to Lautta" from other apps' share menus (INT-1).
    Loader {
        active: handlers.shareEnabled
        sourceComponent: ShareProvider {
            method: "files"
            registerName: true
            onTriggered: handlers.stack().push(Qt.resolvedUrl("../pages/ShareTargetPage.qml"), { "resources": resources })
        }
    }
}
