// SPDX-License-Identifier: LGPL-2.1-or-later
// Compare folders, step one (SYN-1, SYN-3; design: CompareSetup): choose the
// two folders, how items are matched, which names to leave out and whether to
// keep the pair. Accepting opens the results.
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../components"

Dialog {
    id: page

    // The left folder (where Compare was started).
    property string leftUri
    // The right folder, chosen on this page.
    property string rightUri
    readonly property string pairName: leftUri.length > 0 && rightUri.length > 0
                                       ? App.nameOf(leftUri) + " → " + App.nameOf(rightUri) : ""

    function pickSide(left) {
        var picker = pageStack.push(Qt.resolvedUrl("../dialogs/FolderPickerDialog.qml"), {
            //% "Choose a folder"
            "title": qsTrId("lautta-compare-pick-title"),
            //% "Choose"
            "acceptText": qsTrId("lautta-compare-pick-accept"),
            "startUri": left ? page.leftUri : (page.rightUri || page.leftUri)
        })
        picker.accepted.connect(function () {
            if (left)
                page.leftUri = picker.selectedUri
            else
                page.rightUri = picker.selectedUri
        })
    }

    function swap() {
        var left = leftUri
        leftUri = rightUri
        rightUri = left
    }

    allowedOrientations: Orientation.All
    canAccept: leftUri.length > 0 && rightUri.length > 0 && leftUri !== rightUri
    acceptDestination: Qt.resolvedUrl("CompareResultsPage.qml")
    acceptDestinationAction: PageStackAction.Replace
    acceptDestinationProperties: ({
        "leftUri": leftUri,
        "rightUri": rightUri,
        "dstTolerance": dstSwitch.checked,
        "checksums": checksumSwitch.checked,
        "excludesText": excludeField.text,
        "savePair": pairSwitch.checked,
        "pairLabel": pairName
    })

    SilicaFlickable {
        anchors.fill: parent
        contentHeight: column.height + Theme.paddingLarge

        Column {
            id: column

            width: parent.width

            DialogHeader {
                //% "Compare"
                acceptText: qsTrId("lautta-compare-accept")
                //% "Cancel"
                cancelText: qsTrId("lautta-compare-cancel")
            }

            SectionHeader {
                //% "Folders"
                text: qsTrId("lautta-compare-folders")
            }

            BackgroundItem {
                width: column.width
                height: Theme.itemSizeMedium
                onClicked: page.pickSide(true)

                AppIcon {
                    id: leftIcon

                    x: Theme.horizontalPageMargin
                    anchors.verticalCenter: parent.verticalCenter
                    icon: "image://theme/icon-m-file-folder"
                    highlighted: parent.highlighted
                }
                Column {
                    anchors {
                        left: leftIcon.right
                        leftMargin: Theme.paddingLarge
                        right: parent.right
                        rightMargin: Theme.horizontalPageMargin
                        verticalCenter: parent.verticalCenter
                    }

                    Label {
                        width: parent.width
                        text: page.leftUri.length > 0 ? App.displayAddress(page.leftUri) : ""
                        truncationMode: TruncationMode.Fade
                        color: parent.parent.highlighted ? Theme.highlightColor : Theme.primaryColor
                    }
                    Label {
                        width: parent.width
                        //% "Left"
                        text: qsTrId("lautta-compare-left")
                        color: Theme.secondaryColor
                        font.pixelSize: Theme.fontSizeExtraSmall
                    }
                }
            }

            IconButton {
                anchors.right: parent.right
                anchors.rightMargin: Theme.horizontalPageMargin - Theme.paddingMedium
                icon.source: "image://theme/icon-m-sync"
                onClicked: page.swap()
            }

            BackgroundItem {
                width: column.width
                height: Theme.itemSizeMedium
                onClicked: page.pickSide(false)

                AppIcon {
                    id: rightIcon

                    x: Theme.horizontalPageMargin
                    anchors.verticalCenter: parent.verticalCenter
                    icon: "image://theme/icon-m-file-folder"
                    highlighted: parent.highlighted
                }
                Column {
                    anchors {
                        left: rightIcon.right
                        leftMargin: Theme.paddingLarge
                        right: parent.right
                        rightMargin: Theme.horizontalPageMargin
                        verticalCenter: parent.verticalCenter
                    }

                    Label {
                        width: parent.width
                        text: page.rightUri.length > 0
                              ? App.displayAddress(page.rightUri)
                              //% "Choose the folder to compare with"
                              : qsTrId("lautta-compare-right-empty")
                        truncationMode: TruncationMode.Fade
                        color: parent.parent.highlighted ? Theme.highlightColor : Theme.primaryColor
                    }
                    Label {
                        width: parent.width
                        //% "Right"
                        text: qsTrId("lautta-compare-right")
                        color: Theme.secondaryColor
                        font.pixelSize: Theme.fontSizeExtraSmall
                    }
                }
            }

            SectionHeader {
                //% "Match by"
                text: qsTrId("lautta-compare-match")
            }
            // Size and modification time always apply (SYN-1).
            TextSwitch {
                automaticCheck: false
                checked: true
                //% "Size and modification time"
                text: qsTrId("lautta-compare-match-basic")
                //% "2 s tolerance"
                description: qsTrId("lautta-compare-match-basic-hint")
            }
            TextSwitch {
                id: dstSwitch

                //% "Ignore 1 hour difference"
                text: qsTrId("lautta-compare-dst")
                //% "Daylight saving time"
                description: qsTrId("lautta-compare-dst-hint")
            }
            TextSwitch {
                id: checksumSwitch

                //% "Compare checksums"
                text: qsTrId("lautta-compare-checksums")
                //% "Slower; reads every file"
                description: qsTrId("lautta-compare-checksums-hint")
            }

            SectionHeader {
                //% "Exclude"
                text: qsTrId("lautta-compare-exclude")
            }
            TextField {
                id: excludeField

                width: parent.width
                //% "Patterns"
                label: qsTrId("lautta-compare-patterns")
                placeholderText: "*.tmp, .thumbnails/"
                inputMethodHints: Qt.ImhNoPredictiveText | Qt.ImhNoAutoUppercase
                EnterKey.iconSource: "image://theme/icon-m-enter-close"
                EnterKey.onClicked: focus = false
            }

            TextSwitch {
                id: pairSwitch

                checked: true
                //% "Save as sync pair"
                text: qsTrId("lautta-compare-save-pair")
                //% "Shown in Favourites"
                description: qsTrId("lautta-compare-save-pair-hint")
            }
        }

        VerticalScrollDecorator { }
    }
}
