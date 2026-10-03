// SPDX-License-Identifier: LGPL-2.1-or-later
// The report of a crash in an earlier run (SPEC RS-4; design: CrashReport):
// shown at start, with Copy report. Dismissing removes the saved reports.
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0

Dialog {
    id: page

    // The report text; read from the app when not given.
    property string report: App.crashReport()

    function copyReport() {
        Clipboard.text = report
    }

    allowedOrientations: Orientation.All
    // Accepting copies the report ("Copy report"); closing only dismisses it.
    onAccepted: copyReport()
    onDone: App.dismissCrashReports()

    SilicaFlickable {
        anchors.fill: parent
        contentHeight: column.height + Theme.paddingLarge

        Column {
            id: column

            width: parent.width

            DialogHeader {
                //% "Copy report"
                acceptText: qsTrId("lautta-crash-copy")
                //% "Close"
                cancelText: qsTrId("lautta-crash-close")
            }

            Label {
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                horizontalAlignment: Text.AlignRight
                color: Theme.highlightColor
                font.pixelSize: Theme.fontSizeSmall
                //% "Lautta stopped unexpectedly"
                text: qsTrId("lautta-crash-title")
            }

            Label {
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                topPadding: Theme.paddingLarge
                bottomPadding: Theme.paddingMedium
                wrapMode: Text.Wrap
                //% "A report was saved. It contains no file names or contents."
                text: qsTrId("lautta-crash-note")
            }

            Rectangle {
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                height: reportText.height + 2 * Theme.paddingMedium
                color: Theme.rgba(Theme.highlightBackgroundColor, Theme.highlightBackgroundOpacity)

                Label {
                    id: reportText

                    x: Theme.paddingMedium
                    y: Theme.paddingMedium
                    width: parent.width - 2 * Theme.paddingMedium
                    wrapMode: Text.WrapAnywhere
                    font.family: "monospace"
                    font.pixelSize: Theme.fontSizeExtraSmall
                    text: page.report
                }
            }

            Label {
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                topPadding: Theme.paddingMedium
                wrapMode: Text.Wrap
                color: Theme.secondaryColor
                font.pixelSize: Theme.fontSizeSmall
                //% "Saved in Lautta's cache folder."
                text: qsTrId("lautta-crash-saved")
            }
        }

        VerticalScrollDecorator { }
    }
}
