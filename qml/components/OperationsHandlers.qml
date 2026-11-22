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

    function show(text) {
        notice.text = text
        notice.show()
    }

    function context(subject) {
        if (!subject)
            return {}
        return { "item": Operations.displayPath(subject), "location": App.locationName(subject) }
    }

    // Share sheet leaves its own app in front.
    function receiveShare(resources) {
        __silica_applicationwindow_instance.activate()
        pages.push(Qt.resolvedUrl("../pages/ShareTargetPage.qml"), { "resources": resources })
    }

    function background() {
        //% "Running in the background"
        show(qsTrId("lautta-ops-background"))
    }

    Notice {
        id: notice
    }

    PageQueue {
        id: pages
    }

    KeepAlive {
        enabled: Operations.jobsActive > 0
    }

    Connections {
        target: Operations

        onNeedsSummary: pages.push(Qt.resolvedUrl("../dialogs/PlanSummaryDialog.qml"), {
            "planId": planId,
            "summary": JSON.parse(summaryJson)
        })
        onArchiveOpened: pages.stack().push(Qt.resolvedUrl("../pages/DirectoryPage.qml"), { "uri": rootUri })
        onFailed: handlers.show(ErrorText.message(kind, handlers.context(subject)))
        onStarted: handlers.background()
        onJobStarted: handlers.background()
        onJobFinished: {
            if (ok)
                //% "Compressed file created"
                handlers.show(qsTrId("lautta-ops-compressed-created"))
        }
    }

    Loader {
        active: handlers.shareEnabled
        sourceComponent: ShareProvider {
            method: "files"
            registerName: true
            onTriggered: handlers.receiveShare(resources)
        }
    }
}
