// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import Sailfish.Share 1.0
import Nemo.KeepAlive 1.2
import Lautta 1.0
import "ErrorText.js" as ErrorText

Item {
    id: handlers

    // Off in --qml-check: no session bus for the share provider to register on.
    property bool shareEnabled: Qt.application.arguments.indexOf("--qml-check") < 0
    property string pendingFolder

    // Resolved when used: window publishes its instance once created.
    function stack() {
        return __silica_applicationwindow_instance.pageStack
    }

    function show(text) {
        notice.text = text
        notice.show()
    }

    function context(subject) {
        if (!subject)
            return {}
        return { "item": Operations.displayPath(subject), "location": App.locationName(subject) }
    }

    // PageStack ignores a push during a transition, e.g. the extract dialog closing.
    function openFolder(uri) {
        pendingFolder = uri
        if (!stack().busy)
            openPending()
    }

    function openPending() {
        var uri = pendingFolder
        pendingFolder = ""
        stack().push(Qt.resolvedUrl("../pages/DirectoryPage.qml"), { "uri": uri })
    }

    function background() {
        //% "Running in the background"
        show(qsTrId("lautta-ops-background"))
    }

    Notice {
        id: notice
    }

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
        onFailed: handlers.show(ErrorText.message(kind, handlers.context(subject)))
        onStarted: handlers.background()
        onJobStarted: handlers.background()
        onJobFinished: {
            if (ok)
                //% "Archive created"
                handlers.show(qsTrId("lautta-ops-archive-created"))
        }
        onExtractFinished: {
            if (ok)
                handlers.openFolder(destUri)
        }
    }

    Connections {
        target: handlers.pendingFolder.length > 0 ? handlers.stack() : null
        onBusyChanged: handlers.openPending()
    }

    Loader {
        active: handlers.shareEnabled
        sourceComponent: ShareProvider {
            method: "files"
            registerName: true
            onTriggered: handlers.stack().push(Qt.resolvedUrl("../pages/ShareTargetPage.qml"), { "resources": resources })
        }
    }
}
