// SPDX-License-Identifier: LGPL-2.1-or-later
// A half-size picture of the Browse page for Settings › Browse page (BRW-12),
// as Salama's start page preview: the real rows, drawn small, with their
// status icons, following every change. Takes no input.
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "BrowseText.js" as BrowseText
import "RowStatus.js" as RowStatus

Item {
    id: preview

    // The settings page's size, which is the Browse page's size.
    property real screenWidth
    property real screenHeight
    readonly property real miniature: 0.5
    readonly property alias count: rows.count

    height: frame.height

    LocationsModel {
        id: rows
        Component.onCompleted: refresh()
    }

    Rectangle {
        id: frame

        anchors.horizontalCenter: parent.horizontalCenter
        width: preview.screenWidth * preview.miniature
        height: preview.screenHeight * preview.miniature
        radius: Theme.paddingLarge
        clip: true
        color: Theme.rgba(Theme.highlightDimmerColor, Theme.opacityLow)
        border {
            width: Math.max(1, Math.round(Theme.paddingSmall / 4))
            color: Theme.rgba(Theme.primaryColor, Theme.opacityLow)
        }

        // Full size scaled down, so the layout matches the real page.
        ListView {
            width: preview.screenWidth
            height: preview.screenHeight
            scale: preview.miniature
            transformOrigin: Item.TopLeft
            interactive: false
            model: rows

            header: PageHeader {
                title: qsTrId("lautta-browse-title")
            }

            section.property: "section"
            section.criteria: ViewSection.FullString
            section.delegate: SectionHeader {
                text: BrowseText.sectionTitle(section)
            }

            delegate: Item {
                readonly property bool banner: model.kind === "consent" || model.kind === "unavailable"

                width: ListView.view.width
                height: banner ? 0 : Theme.itemSizeSmall
                visible: !banner

                HighlightImage {
                    anchors {
                        left: parent.left
                        leftMargin: Theme.horizontalPageMargin
                        verticalCenter: parent.verticalCenter
                    }
                    source: model.icon
                    width: Theme.iconSizeMedium
                    height: width
                    sourceSize.width: width
                    sourceSize.height: height
                    color: model.colour.length > 0 ? model.colour : Theme.primaryColor
                }
                Label {
                    anchors {
                        left: parent.left
                        leftMargin: Theme.horizontalPageMargin + Theme.iconSizeMedium + Theme.paddingLarge
                        right: statusIcon.indicator.length > 0 ? statusIcon.left : parent.right
                        rightMargin: statusIcon.indicator.length > 0 ? Theme.paddingLarge : Theme.horizontalPageMargin
                        verticalCenter: parent.verticalCenter
                    }
                    text: BrowseText.rowName(model.kind, model.name)
                    truncationMode: TruncationMode.Fade
                }
                RowStatusIcon {
                    id: statusIcon

                    indicator: RowStatus.indicator(model.kind, model.status, model.connection,
                                                   model.connectionError, model.drive, Bridge.busyDrives)
                    anchors {
                        right: parent.right
                        rightMargin: Theme.horizontalPageMargin
                        verticalCenter: parent.verticalCenter
                    }
                }
            }
        }
    }
}
