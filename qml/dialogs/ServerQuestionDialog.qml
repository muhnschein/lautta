// SPDX-License-Identifier: LGPL-2.1-or-later
// A question of the bridge about an ad-hoc server (NVB-6): an unknown
// identity, an unencrypted connection, or sign-in prompts. Answered in the
// app; for accounts these questions never come here (NVB-5). `kind` is the
// bridge's name of the question, `details` its display strings.
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0

Dialog {
    id: dialog

    property string questionId
    property string kind
    property var details: ({})

    readonly property bool identity: kind === "identity-unknown"
    readonly property bool insecure: kind === "insecure-consent"
    readonly property bool signIn: kind === "keyboard-interactive"
    readonly property string host: details.host || ""
    readonly property var prompts: signIn && details.prompts ? String(details.prompts).split("\n") : []
    // The question was settled elsewhere (answered, or the connection ended).
    property bool settled
    property bool answered

    allowedOrientations: Orientation.All

    function answer(json) {
        if (answered)
            return
        answered = true
        Bridge.answer(questionId, json)
    }

    onAccepted: {
        if (signIn) {
            var responses = []
            for (var i = 0; i < fields.count; ++i) {
                responses.push(fields.itemAt(i).text)
                fields.itemAt(i).text = ""
            }
            answer(JSON.stringify({ "responses": responses }))
        } else {
            answer(JSON.stringify({ "accept": true }))
        }
    }
    onRejected: answer(JSON.stringify({ "decline": true }))
    onStatusChanged: if (status === PageStatus.Active && settled) reject()
    Component.onDestruction: {
        for (var i = 0; i < fields.count; ++i)
            fields.itemAt(i).text = ""
        answer(JSON.stringify({ "decline": true }))
    }

    Connections {
        target: Bridge
        onQuestionResolved: {
            if (questionId !== dialog.questionId)
                return
            dialog.settled = true
            if (dialog.status === PageStatus.Active && !dialog.answered)
                dialog.reject()
        }
    }

    SilicaFlickable {
        anchors.fill: parent
        contentHeight: column.height + Theme.paddingLarge

        Column {
            id: column
            width: parent.width

            DialogHeader {
                acceptText: dialog.identity
                            //% "Trust"
                            ? qsTrId("lautta-question-trust")
                            : (dialog.insecure
                               //% "Allow"
                               ? qsTrId("lautta-question-allow")
                               : (dialog.signIn
                                  //% "Sign in"
                                  ? qsTrId("lautta-question-sign-in")
                                  //% "Accept"
                                  : qsTrId("lautta-question-accept")))
                cancelText: dialog.identity
                            //% "Reject"
                            ? qsTrId("lautta-question-reject")
                            //% "Cancel"
                            : qsTrId("lautta-question-cancel")
                title: dialog.identity
                       //% "Unknown server"
                       ? qsTrId("lautta-question-identity-title")
                       : (dialog.insecure
                          //% "Unencrypted connection"
                          ? qsTrId("lautta-question-insecure-title")
                          : (dialog.signIn
                             ? (dialog.details.name ? dialog.details.name
                                                    //% "Sign-in"
                                                    : qsTrId("lautta-question-sign-in-title"))
                             //% "Question from the server"
                             : qsTrId("lautta-question-other-title")))
            }

            Label {
                visible: text.length > 0
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * x
                wrapMode: Text.Wrap
                color: Theme.highlightColor
                text: {
                    if (dialog.identity)
                        //% "%1 hasn’t been seen before. Check that this fingerprint matches the server."
                        return qsTrId("lautta-question-identity-text").arg(dialog.host)
                    if (dialog.insecure)
                        //% "%1 doesn’t encrypt the connection. What you send can be read on the network."
                        return qsTrId("lautta-question-insecure-text").arg(dialog.host)
                    if (dialog.signIn)
                        return dialog.details.instruction || ""
                    return ""
                }
            }

            Item {
                visible: dialog.identity || dialog.insecure
                width: 1
                height: Theme.paddingLarge
            }

            DetailItem {
                visible: (dialog.identity || dialog.insecure) && dialog.host.length > 0
                //% "Host"
                label: qsTrId("lautta-question-host")
                value: dialog.host
            }
            DetailItem {
                visible: dialog.identity && !!dialog.details.port
                //% "Port"
                label: qsTrId("lautta-question-port")
                value: dialog.details.port || ""
            }
            DetailItem {
                visible: dialog.identity && !!dialog.details.algorithm
                //% "Key type"
                label: qsTrId("lautta-question-key-type")
                value: dialog.details.algorithm || ""
            }
            DetailItem {
                visible: dialog.identity && !!dialog.details.fingerprint
                //% "Fingerprint"
                label: qsTrId("lautta-question-fingerprint")
                value: dialog.details.fingerprint || ""
            }
            DetailItem {
                visible: dialog.identity && !!dialog.details.problems
                //% "Problems"
                label: qsTrId("lautta-question-problems")
                value: dialog.details.problems || ""
            }

            Repeater {
                id: fields
                model: dialog.prompts

                delegate: PasswordField {
                    width: column.width
                    label: modelData.replace(/[:\s]+$/, "")
                    placeholderText: label
                    // Whether a prompt echoes is the server's business; showing the
                    // text only while it is typed keeps codes readable.
                    showEchoModeToggle: true
                    EnterKey.iconSource: index === fields.count - 1 ? "image://theme/icon-m-enter-accept"
                                                                    : "image://theme/icon-m-enter-next"
                    EnterKey.onClicked: {
                        if (index === fields.count - 1)
                            dialog.accept()
                        else
                            fields.itemAt(index + 1).focus = true
                    }
                }
            }
        }

        VerticalScrollDecorator { }
    }
}
