// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import "PathFold.js" as PathFold

Item {
    id: crumbs

    property var pathModel
    property real titleTop
    property Item menuHost
    property int menuMost
    property var widths: []
    property Item _menu

    readonly property bool hasAncestors: steps.count > 1
    readonly property real separatorWidth: houseSeparator.width
    readonly property real _lead: houseButton.width + separatorWidth
    readonly property real _foldable: hasAncestors ? dotsButton.width + separatorWidth : 0
    readonly property real reserve: _lead + _foldable + Theme.paddingMedium
    readonly property real room: width + _foldable
    readonly property int folded: PathFold.folded(widths, separatorWidth, dotsButton.width, room)
    readonly property var offsets: PathFold.offsets(widths, separatorWidth, folded)
    readonly property real shownWidth: PathFold.shownWidth(widths, separatorWidth, dotsButton.width, folded)
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

        font.pixelSize: Theme.fontSizeLarge
        font.family: Theme.fontFamilyHeading
    }

    Item {
        id: row

        objectName: "crumbRow"
        anchors {
            right: parent.right
            rightMargin: Theme.paddingMedium
        }
        width: crumbs._lead + crumbs.shownWidth
        height: parent.height

        BackgroundItem {
            id: houseButton

            objectName: "crumbHome"
            width: house.width + 2 * Theme.paddingMedium
            height: parent.height
            onClicked: crumbs.home()

            HighlightImage {
                id: house

                x: Theme.paddingMedium
                y: Math.round(crumbs.titleTop + (heading.height - height) / 2)
                width: Theme.iconSizeSmallPlus
                height: width
                sourceSize.width: width
                sourceSize.height: height
                source: "image://theme/icon-m-home"
                highlighted: houseButton.highlighted
            }
        }

        Label {
            id: houseSeparator

            objectName: "crumbSeparator"
            x: houseButton.width
            y: Math.round(crumbs.textBaseline - baselineOffset)
            text: "›"
            color: Theme.secondaryColor
        }

        BackgroundItem {
            id: dotsButton

            objectName: "crumbDots"
            visible: crumbs.folded > 0
            x: crumbs._lead
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

        Label {
            visible: dotsButton.visible
            x: dotsButton.x + dotsButton.width
            y: houseSeparator.y
            text: houseSeparator.text
            color: Theme.secondaryColor
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
                x: crumbs._lead + (crumbs.folded > 0 ? crumbs._foldable : 0) + (crumbs.offsets[index] || 0)
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
                    y: houseSeparator.y
                    text: houseSeparator.text
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

                    text: step ? step.name : ""
                    onDelayedClick: crumbs.picked(step.uri)
                }
            }
        }
    }
}
