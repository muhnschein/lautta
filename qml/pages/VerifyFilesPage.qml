// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../components"
import "../components/ErrorText.js" as ErrorText

Page {
    id: page

    property string uri
    readonly property alias model: verify

    function stateText(state, errorKind) {
        switch (state) {
        case "matches":
            //% "Matches"
            return qsTrId("lautta-verify-matches")
        case "differs":
            //% "Doesn't match"
            return qsTrId("lautta-verify-differs")
        case "missing":
            //% "Missing"
            return qsTrId("lautta-verify-missing")
        case "notChecked":
            //% "Not checked"
            return qsTrId("lautta-verify-not-checked")
        case "failed":
            return ErrorText.message(errorKind, { "location": App.locationName(page.uri), "open": true })
        }
        return ""
    }

    function stateIcon(state) {
        switch (state) {
        case "matches": return "image://theme/icon-s-accept"
        case "differs":
        case "missing":
        case "failed": return "image://theme/icon-s-warning"
        }
        return ""
    }

    allowedOrientations: Orientation.All

    VerifyModel {
        id: verify

        uri: page.uri
        Component.onCompleted: start()
    }

    SilicaListView {
        id: list

        anchors.fill: parent
        model: verify

        header: HeaderBand {
            PageHeader {
                title: App.nameOf(page.uri)
            }
        }

        delegate: ListItem {
            objectName: "verifyRow"
            contentHeight: Math.max(Theme.itemSizeMedium, texts.height + 2 * Theme.paddingSmall)
            enabled: false

            HighlightImage {
                id: icon

                x: Theme.horizontalPageMargin
                anchors.verticalCenter: parent.verticalCenter
                width: Theme.iconSizeSmall
                height: Theme.iconSizeSmall
                source: page.stateIcon(model.state)
                color: model.state === "matches" ? Theme.highlightColor : Theme.errorColor
            }

            Column {
                id: texts

                anchors {
                    left: icon.right
                    leftMargin: Theme.paddingMedium
                    right: parent.right
                    rightMargin: Theme.horizontalPageMargin
                    verticalCenter: parent.verticalCenter
                }

                Label {
                    width: parent.width
                    text: model.name
                    truncationMode: TruncationMode.Fade
                }

                ProgressBar {
                    visible: model.state === "checking"
                    width: parent.width
                    leftMargin: 0
                    rightMargin: 0
                    indeterminate: model.progress < 0
                    value: Math.max(0, model.progress)
                }

                Label {
                    width: parent.width
                    visible: text.length > 0
                    text: page.stateText(model.state, model.errorKind)
                    font.pixelSize: Theme.fontSizeExtraSmall
                    wrapMode: Text.Wrap
                    color: model.state === "matches" ? Theme.highlightColor
                         : model.state === "notChecked" ? Theme.secondaryColor : Theme.errorColor
                }
            }
        }

        ViewPlaceholder {
            enabled: !verify.running && verify.count === 0
            text: verify.errorKind.length > 0 && verify.errorKind !== "Canceled"
                  ? ErrorText.message(verify.errorKind, { "item": App.nameOf(page.uri), "open": true })
                  //% "No files listed"
                  : qsTrId("lautta-verify-empty")
        }

        BusyIndicator {
            anchors.centerIn: parent
            size: BusyIndicatorSize.Large
            running: verify.running && verify.count === 0
        }

        VerticalScrollDecorator { }
    }
}
