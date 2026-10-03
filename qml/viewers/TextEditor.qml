// SPDX-License-Identifier: LGPL-2.1-or-later
// Built-in text editor (EDT-4, board TextEditor): files up to 1 MiB that are
// valid UTF-8; line endings and the final newline are put back on save, and
// saving a remote file uploads it (EDT-2). Accepting does not write by
// itself: it emits saveRequested(text) and the page that opened the editor
// (which outlives it) saves, so conflicts can be answered there.
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../components/viewers/ViewerText.js" as ViewerText

Dialog {
    id: dialog

    property string uri
    // The document of the viewer that opened the editor; loaded here when
    // the editor is opened on its own.
    property var document: ownDocument

    readonly property bool dirty: editor.loadedText && editor.text !== document.text
    readonly property bool remote: !App.isLocal(uri)

    signal saveRequested(string text)

    allowedOrientations: Orientation.All
    canAccept: document.editable && dirty

    onAccepted: saveRequested(editor.text)

    TextDocument {
        id: ownDocument
        // Only used when no document is handed in.
        uri: dialog.document === ownDocument ? dialog.uri : ""
    }

    SilicaFlickable {
        anchors.fill: parent
        contentHeight: column.height

        Column {
            id: column
            width: parent.width

            DialogHeader {
                //% "Save"
                acceptText: qsTrId("lautta-viewers-save")
            }

            Label {
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                horizontalAlignment: Text.AlignRight
                color: Theme.highlightColor
                font.pixelSize: Theme.fontSizeSmall
                truncationMode: TruncationMode.Fade
                text: App.nameOf(dialog.uri) + " · " + App.locationName(dialog.uri)
            }

            TextArea {
                id: editor

                // Set once, when the text has been read: later changes of
                // the document must not overwrite what is being typed.
                property bool loadedText

                width: parent.width
                visible: dialog.document.editable
                font.family: "monospace"
                font.pixelSize: Theme.fontSizeSmall
                labelVisible: false
                placeholderText: ""
                focus: false

                Connections {
                    target: dialog.document
                    onLoadedChanged: editor.fill()
                    onTextChanged: if (!editor.loadedText) editor.fill()
                }
                Component.onCompleted: fill()

                function fill() {
                    if (dialog.document.loaded && dialog.document.editable && !loadedText) {
                        text = dialog.document.text
                        loadedText = true
                    }
                }
            }

            Label {
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                visible: editor.visible
                wrapMode: Text.Wrap
                color: Theme.secondaryColor
                font.pixelSize: Theme.fontSizeExtraSmall
                text: {
                    var parts = [ViewerText.lineEndingText(dialog.document.lineEnding)]
                    parts.push(dialog.document.finalNewline
                               //% "ends with newline"
                               ? qsTrId("lautta-viewers-ends-newline")
                               //% "no final newline"
                               : qsTrId("lautta-viewers-no-newline"))
                    if (dialog.remote)
                        //% "saving uploads to %1"
                        parts.push(qsTrId("lautta-viewers-saving-uploads").arg(App.locationName(dialog.uri)))
                    return parts.join(" · ")
                }
            }
        }

        ViewPlaceholder {
            enabled: dialog.document.loaded && !dialog.document.editable
            //% "Can't edit this file"
            text: qsTrId("lautta-viewers-cant-edit")
            hintText: !dialog.document.writable
                      //% "You can't change files here."
                      ? qsTrId("lautta-viewers-cant-edit-readonly")
                      : dialog.document.truncated
                        //% "It is larger than 1 MiB."
                        ? qsTrId("lautta-viewers-cant-edit-large")
                        //% "It isn't valid UTF-8 text."
                        : qsTrId("lautta-viewers-cant-edit-binary")
        }

        BusyIndicator {
            anchors.centerIn: parent
            size: BusyIndicatorSize.Large
            running: dialog.document.loading
        }

        VerticalScrollDecorator { }
    }
}
