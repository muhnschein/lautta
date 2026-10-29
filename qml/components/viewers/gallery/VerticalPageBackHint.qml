// SPDX-FileCopyrightText: 2017-2018 Jolla Ltd.
// SPDX-FileCopyrightText: 2025 Jolla Mobile Ltd
//
// SPDX-License-Identifier: BSD-3-Clause
//
// jolla-gallery's src/pages/VerticalPageBackHint.qml, adapted for lautta
// (PRV-4, see LAUTTA.md): lautta's text and its own dconf key, so the first
// two times the image viewer opens it shows that a swipe down goes back.
import QtQuick 2.6
import Sailfish.Silica 1.0

Loader {
    id: root

    property bool pageActive

    anchors.fill: parent
    active: counter.active
    sourceComponent: Component {
        Item {
            property bool pageActive: root.pageActive
            onPageActiveChanged: {
                if (pageActive) {
                    timer.restart()
                    counter.increase()
                    pageActive = false
                }
            }

            anchors.fill: parent

            Timer {
                id: timer

                interval: 500
                onTriggered: touchInteractionHint.restart()
            }

            InteractionHintLabel {
                //% "Swipe down to go to previous view"
                text: qsTrId("lautta-viewers-image-back-hint")
                anchors.bottom: parent.bottom
                opacity: touchInteractionHint.running ? 1.0 : 0.0
                Behavior on opacity { FadeAnimation { duration: 1000 } }
                textColor: Theme.highlightFromColor(Theme.highlightColor, Theme.LightOnDark)
                backgroundColor: Theme.rgba(Theme.highlightDimmerFromColor(Theme.highlightDimmerColor,
                                                                           Theme.LightOnDark), 0.9)
            }
            TouchInteractionHint {
                id: touchInteractionHint

                color: Theme.lightPrimaryColor
                direction: TouchInteraction.Down
                anchors.horizontalCenter: parent.horizontalCenter
            }
        }
    }
    FirstTimeUseCounter {
        id: counter

        limit: 2
        key: "/apps/harbour-lautta/hints/image-swipe-back"
    }
}
