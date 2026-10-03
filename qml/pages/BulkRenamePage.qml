// SPDX-License-Identifier: LGPL-2.1-or-later
// Bulk rename (OPS-11): one rule, optional numbering, and a live preview that
// flags collisions and names the location does not accept.
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../components/operations"

Dialog {
    id: page

    // The URIs of the selected items (one folder).
    property var uris: []

    property int ruleIndex
    property string findText
    property string replaceText
    property bool useRegex
    property string prefixText
    property string suffixText
    property int caseIndex
    property int extensionIndex
    property string extensionText
    property string dateFormat: "YYYY-MM-DD"
    property bool numbering
    property string startAt: "1"
    property int digits: 3

    allowedOrientations: Orientation.All
    canAccept: bulk.changedCount - bulk.problemCount > 0 && bulk.errorKind.length === 0 && !bulk.busy
    onAccepted: bulk.apply()

    function buildRules() {
        var rules = []
        switch (ruleIndex) {
        case 0:
            rules.push({ "type": "findReplace", "find": findText, "replace": replaceText,
                         "regex": useRegex, "caseSensitive": true })
            break
        case 1:
            rules.push({ "type": "prefix", "text": prefixText })
            break
        case 2:
            rules.push({ "type": "suffix", "text": suffixText })
            break
        case 3:
            rules.push({ "type": "case", "mode": ["lower", "upper", "title", "sentence"][caseIndex] })
            break
        case 4:
            rules.push({ "type": "extension", "mode": ["change", "remove", "lowercase"][extensionIndex],
                         "value": extensionText })
            break
        default:
            rules.push({ "type": "date", "format": dateFormat, "position": "prefix", "separator": " " })
            break
        }
        if (numbering)
            rules.push({ "type": "numbering", "start": parseInt(startAt, 10) || 0, "step": 1,
                         "padding": digits, "position": "suffix", "separator": " " })
        return JSON.stringify({ "includeExtension": false, "rules": rules })
    }

    function statusText(status) {
        switch (status) {
        case "collision":
            //% "Same name as another item"
            return qsTrId("lautta-rename-collision")
        case "invalid":
            //% "Not allowed here"
            return qsTrId("lautta-rename-invalid")
        default:
            return ""
        }
    }

    BulkRenameModel {
        id: bulk
        urisJson: JSON.stringify(page.uris)
        rulesJson: page.buildRules()
    }

    SilicaListView {
        id: list

        anchors.fill: parent
        model: bulk
        currentIndex: -1

        header: Column {
            width: list.width

            DialogHeader {
                //% "Rename"
                acceptText: qsTrId("lautta-rename-accept")
            }
            DialogSubtitle {
                //% "Rename %n items"
                text: qsTrId("lautta-rename-count", page.uris.length)
            }
            ComboBox {
                //% "Rule"
                label: qsTrId("lautta-rename-rule")
                menu: ContextMenu {
                    MenuItem {
                        //% "Find and replace"
                        text: qsTrId("lautta-rename-rule-find")
                    }
                    MenuItem {
                        //% "Add at the start"
                        text: qsTrId("lautta-rename-rule-prefix")
                    }
                    MenuItem {
                        //% "Add at the end"
                        text: qsTrId("lautta-rename-rule-suffix")
                    }
                    MenuItem {
                        //% "Change case"
                        text: qsTrId("lautta-rename-rule-case")
                    }
                    MenuItem {
                        //% "Change extension"
                        text: qsTrId("lautta-rename-rule-extension")
                    }
                    MenuItem {
                        //% "Add date"
                        text: qsTrId("lautta-rename-rule-date")
                    }
                }
                onCurrentIndexChanged: page.ruleIndex = currentIndex
            }

            TextField {
                width: parent.width
                visible: page.ruleIndex === 0
                //% "Find"
                label: qsTrId("lautta-rename-find")
                placeholderText: label
                text: page.findText
                onTextChanged: page.findText = text
                EnterKey.iconSource: "image://theme/icon-m-enter-next"
                EnterKey.onClicked: replaceField.focus = true
            }
            TextSwitch {
                visible: page.ruleIndex === 0
                checked: page.useRegex
                //% "Regular expression"
                text: qsTrId("lautta-rename-regex")
                onCheckedChanged: page.useRegex = checked
            }
            TextField {
                id: replaceField
                width: parent.width
                visible: page.ruleIndex === 0
                //% "Replace with"
                label: qsTrId("lautta-rename-replace")
                placeholderText: label
                text: page.replaceText
                onTextChanged: page.replaceText = text
                EnterKey.iconSource: "image://theme/icon-m-enter-close"
                EnterKey.onClicked: focus = false
            }
            TextField {
                width: parent.width
                visible: page.ruleIndex === 1
                //% "Text to add"
                label: qsTrId("lautta-rename-prefix")
                placeholderText: label
                text: page.prefixText
                onTextChanged: page.prefixText = text
                EnterKey.iconSource: "image://theme/icon-m-enter-close"
                EnterKey.onClicked: focus = false
            }
            TextField {
                width: parent.width
                visible: page.ruleIndex === 2
                label: qsTrId("lautta-rename-prefix")
                placeholderText: label
                text: page.suffixText
                onTextChanged: page.suffixText = text
                EnterKey.iconSource: "image://theme/icon-m-enter-close"
                EnterKey.onClicked: focus = false
            }
            ComboBox {
                visible: page.ruleIndex === 3
                //% "Case"
                label: qsTrId("lautta-rename-case")
                menu: ContextMenu {
                    MenuItem {
                        //% "lowercase"
                        text: qsTrId("lautta-rename-case-lower")
                    }
                    MenuItem {
                        //% "UPPERCASE"
                        text: qsTrId("lautta-rename-case-upper")
                    }
                    MenuItem {
                        //% "Title Case"
                        text: qsTrId("lautta-rename-case-title")
                    }
                    MenuItem {
                        //% "Sentence case"
                        text: qsTrId("lautta-rename-case-sentence")
                    }
                }
                onCurrentIndexChanged: page.caseIndex = currentIndex
            }
            ComboBox {
                visible: page.ruleIndex === 4
                //% "Extension"
                label: qsTrId("lautta-rename-extension")
                menu: ContextMenu {
                    MenuItem {
                        //% "Change to"
                        text: qsTrId("lautta-rename-extension-change")
                    }
                    MenuItem {
                        //% "Remove"
                        text: qsTrId("lautta-rename-extension-remove")
                    }
                    MenuItem {
                        //% "Make lowercase"
                        text: qsTrId("lautta-rename-extension-lower")
                    }
                }
                onCurrentIndexChanged: page.extensionIndex = currentIndex
            }
            TextField {
                width: parent.width
                visible: page.ruleIndex === 4 && page.extensionIndex === 0
                //% "New extension"
                label: qsTrId("lautta-rename-new-extension")
                placeholderText: label
                text: page.extensionText
                onTextChanged: page.extensionText = text
                EnterKey.iconSource: "image://theme/icon-m-enter-close"
                EnterKey.onClicked: focus = false
            }
            TextField {
                width: parent.width
                visible: page.ruleIndex === 5
                //% "Date pattern (YYYY MM DD HH mm ss)"
                label: qsTrId("lautta-rename-date")
                placeholderText: label
                text: page.dateFormat
                onTextChanged: page.dateFormat = text
                EnterKey.iconSource: "image://theme/icon-m-enter-close"
                EnterKey.onClicked: focus = false
            }

            SectionHeader {
                //% "Numbering"
                text: qsTrId("lautta-rename-numbering")
            }
            TextSwitch {
                checked: page.numbering
                //% "Add number"
                text: qsTrId("lautta-rename-add-number")
                onCheckedChanged: page.numbering = checked
            }
            TextField {
                width: parent.width
                visible: page.numbering
                //% "Start at"
                label: qsTrId("lautta-rename-start")
                placeholderText: label
                text: page.startAt
                inputMethodHints: Qt.ImhDigitsOnly
                onTextChanged: page.startAt = text
                EnterKey.iconSource: "image://theme/icon-m-enter-close"
                EnterKey.onClicked: focus = false
            }
            ComboBox {
                visible: page.numbering
                //% "Digits"
                label: qsTrId("lautta-rename-digits")
                currentIndex: page.digits - 1
                menu: ContextMenu {
                    MenuItem { text: "1" }
                    MenuItem { text: "2" }
                    MenuItem { text: "3" }
                    MenuItem { text: "4" }
                    MenuItem { text: "5" }
                    MenuItem { text: "6" }
                }
                onCurrentIndexChanged: page.digits = currentIndex + 1
            }

            Label {
                id: errorLabel
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                visible: bulk.errorKind.length > 0
                wrapMode: Text.Wrap
                color: Theme.errorColor
                font.pixelSize: Theme.fontSizeSmall
                //% "This rule can't be used."
                text: qsTrId("lautta-rename-bad-rule")
            }

            SectionHeader {
                //% "Preview"
                text: qsTrId("lautta-rename-preview")
            }
        }

        delegate: Item {
            width: list.width
            height: previewColumn.height + Theme.paddingSmall * 2

            Column {
                id: previewColumn
                anchors {
                    left: parent.left
                    leftMargin: Theme.horizontalPageMargin
                    right: parent.right
                    rightMargin: Theme.horizontalPageMargin
                    verticalCenter: parent.verticalCenter
                }
                Label {
                    width: parent.width
                    text: model.old
                    color: Theme.secondaryColor
                    font.pixelSize: Theme.fontSizeExtraSmall
                    truncationMode: TruncationMode.Fade
                }
                Label {
                    width: parent.width
                    text: model.new
                    truncationMode: TruncationMode.Fade
                }
                Label {
                    width: parent.width
                    visible: text.length > 0
                    text: page.statusText(model.status)
                    color: Theme.errorColor
                    font.pixelSize: Theme.fontSizeExtraSmall
                    truncationMode: TruncationMode.Fade
                }
            }
        }

        VerticalScrollDecorator { }
    }
}
