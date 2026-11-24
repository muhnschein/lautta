// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import "PathFold.js" as PathFold
import "../FileIcons.js" as FileIcons

Item {
    id: crumbs

    property var pathModel
    property real titleTop
    property int titlePixelSize: Theme.fontSizeLarge
    property string titleFamily: Theme.fontFamilyHeading
    property Item menuHost
    property int menuMost
    property real pageWidth
    //% "Home"
    property string homeText: qsTrId("lautta-path-home")
    property var widths: []
    property Item _menu

    readonly property real separatorWidth: leadSeparator.width
    readonly property real backTapWidth: Theme.itemSizeSmall
    readonly property real reserve: dotsButton.width + separatorWidth + Theme.paddingMedium
    readonly property real leftMargin: backTapWidth + reserve
    readonly property bool homeShown: PathFold.homeFits(widths, separatorWidth, houseButton.width,
                                                        width + reserve - Math.max(Theme.paddingMedium, pageWidth / 4 - backTapWidth))
    readonly property int folded: PathFold.folded(widths, separatorWidth, width)
    readonly property var offsets: PathFold.offsets(widths, separatorWidth, folded)
    readonly property real _lead: (homeShown ? houseButton.width : dotsButton.width) + separatorWidth
    readonly property real textBaseline: titleTop + heading.ascent
    readonly property real menuHeight: _menu ? _menu.height : 0

    signal picked(string uri)
    signal home

    function measure() {
        var measured = []
        for (var i = 0; i < steps.count - 1; ++i) {
            var step = steps.itemAt(i)
            if (!step)
                return
            measured.push(step.crumbWidth)
        }
        widths = measured
    }

    function openFolded() {
        if (!_menu)
            _menu = foldedMenu.createObject(crumbs)
        _menu.open(menuHost)
    }

    FontMetrics {
        id: heading

        font.pixelSize: crumbs.titlePixelSize
        font.family: crumbs.titleFamily
    }

    Item {
        id: row

        objectName: "crumbRow"
        anchors {
            right: parent.right
            rightMargin: Theme.paddingMedium
        }
        width: crumbs._lead + PathFold.tailWidth(crumbs.widths, crumbs.separatorWidth, crumbs.folded)
        height: parent.height

        BackgroundItem {
            id: houseButton

            objectName: "crumbHome"
            visible: crumbs.homeShown
            width: house.width + 2 * Theme.paddingMedium
            height: parent.height
            onClicked: crumbs.home()

            HighlightImage {
                id: house

                objectName: "crumbHouse"
                x: Theme.paddingMedium
                y: Math.round(crumbs.titleTop + (heading.height - height) / 2)
                width: Theme.iconSizeSmallPlus
                height: width
                sourceSize.width: width
                sourceSize.height: height
                source: FileIcons.source("home", Theme.pixelRatio)
                highlighted: houseButton.highlighted
            }
        }

        Label {
            id: leadSeparator

            objectName: "crumbSeparator"
            x: crumbs._lead - width
            y: Math.round(crumbs.textBaseline - baselineOffset)
            text: "›"
            color: Theme.secondaryColor
        }

        BackgroundItem {
            id: dotsButton

            objectName: "crumbDots"
            visible: !crumbs.homeShown
            width: dotsLabel.width + 2 * Theme.paddingMedium
            height: parent.height
            onClicked: crumbs.openFolded()

            Label {
                id: dotsLabel

                x: Theme.paddingMedium
                y: Math.round(crumbs.textBaseline - baselineOffset)
                text: "…"
                color: dotsButton.highlighted ? Theme.highlightColor : Theme.primaryColor
            }
        }

        Repeater {
            id: steps

            model: crumbs.pathModel

            Item {
                id: step

                readonly property string uri: model.uri
                readonly property string name: model.name
                readonly property real crumbWidth: stepButton.width

                objectName: "crumb"
                visible: index >= crumbs.folded && index < steps.count - 1
                x: crumbs._lead + (crumbs.offsets[index] || 0)
                width: crumbWidth + crumbs.separatorWidth
                height: parent.height
                onCrumbWidthChanged: crumbs.measure()

                BackgroundItem {
                    id: stepButton

                    objectName: "crumbButton"
                    width: stepLabel.width + 2 * Theme.paddingMedium
                    height: parent.height
                    onClicked: crumbs.picked(step.uri)

                    Label {
                        id: stepLabel

                        x: Theme.paddingMedium
                        y: Math.round(crumbs.textBaseline - baselineOffset)
                        text: step.name
                        color: stepButton.highlighted ? Theme.highlightColor : Theme.primaryColor
                    }
                }

                Label {
                    x: stepButton.width
                    y: leadSeparator.y
                    text: leadSeparator.text
                    color: Theme.secondaryColor
                }
            }
        }
    }

    Component {
        id: foldedMenu

        ContextMenu {
            objectName: "crumbMenu"

            Repeater {
                model: PathFold.menuRows(crumbs.folded, crumbs.menuMost)

                MenuItem {
                    readonly property Item step: steps.itemAt(modelData)

                    text: modelData < 0 ? crumbs.homeText : step ? step.name : ""
                    onDelayedClick: modelData < 0 ? crumbs.home() : crumbs.picked(step.uri)
                }
            }
        }
    }
}
