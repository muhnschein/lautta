// SPDX-License-Identifier: LGPL-2.1-or-later
// The items of one tag (ORG-3). Items that were moved or deleted outside
// Lautta are listed last under "Missing".
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../components/Open.js" as Open
import "../components/browse"

Page {
    id: page

    property int tagId
    // The rows; tests give the page a ListModel.
    property var listModel: items

    allowedOrientations: Orientation.All

    TaggedItemsModel {
        id: items
        tagId: page.tagId
        onRemoved: pageStack.pop()
    }

    SilicaListView {
        id: list

        anchors.fill: parent
        model: page.listModel

        header: PageHeader {
            title: items.tagName
            //% "Tag · %n items"
            description: qsTrId("lautta-tag-description", items.count)
            extraContent.children: [
                Rectangle {
                    anchors.verticalCenter: parent.verticalCenter
                    width: Theme.iconSizeSmall - Theme.paddingSmall
                    height: width
                    radius: width / 2
                    color: items.tagColour
                }
            ]
        }

        section.property: "section"
        section.criteria: ViewSection.FullString
        section.delegate: SectionHeader {
            visible: section === "missing"
            height: visible ? implicitHeight : 0
            //% "Missing"
            text: qsTrId("lautta-tag-missing")
        }

        PullDownMenu {
            MenuItem {
                //% "Delete tag"
                text: qsTrId("lautta-tag-delete")
                onClicked: {
                    //% "Deleting tag"
                    remorsePopup.execute(qsTrId("lautta-tag-deleting"), function() {
                        items.deleteTag()
                    }, App.setting("remorse_seconds") * 1000)
                }
            }
            MenuItem {
                //% "Change colour"
                text: qsTrId("lautta-tag-change-colour")
                onClicked: {
                    var dialog = pageStack.push(Qt.resolvedUrl("../components/browse/TagColourDialog.qml"),
                                                { "colour": items.tagColour })
                    dialog.accepted.connect(function() { items.setColour(dialog.colour) })
                }
            }
            MenuItem {
                //% "Rename tag"
                text: qsTrId("lautta-tag-rename")
                onClicked: {
                    var dialog = pageStack.push(Qt.resolvedUrl("../components/browse/TagRenameDialog.qml"),
                                                { "tagName": items.tagName })
                    dialog.accepted.connect(function() { items.rename(dialog.newName) })
                }
            }
        }

        delegate: ListItem {
            id: row

            readonly property bool gone: model.missing

            contentHeight: Theme.itemSizeMedium
            enabled: !gone
            opacity: gone ? Theme.opacityLow : 1
            onClicked: Open.open(pageStack, App, Operations, model.uri, model.isDir, "", "")

            ItemIcon {
                anchors {
                    left: parent.left
                    leftMargin: Theme.horizontalPageMargin
                    verticalCenter: parent.verticalCenter
                }
                name: model.name
                isDir: model.isDir
                highlightedItem: row.highlighted
            }
            Column {
                anchors {
                    left: parent.left
                    leftMargin: Theme.horizontalPageMargin + Theme.iconSizeMedium + Theme.paddingLarge
                    right: parent.right
                    rightMargin: Theme.horizontalPageMargin
                    verticalCenter: parent.verticalCenter
                }

                Label {
                    width: parent.width
                    text: model.name
                    color: row.highlighted ? Theme.highlightColor : Theme.primaryColor
                    truncationMode: TruncationMode.Fade
                }
                Label {
                    width: parent.width
                    text: row.gone
                          //% "Was in %1"
                          ? qsTrId("lautta-tag-was-in").arg(model.place)
                          : model.place
                    font.pixelSize: Theme.fontSizeExtraSmall
                    color: Theme.secondaryColor
                    truncationMode: TruncationMode.Fade
                }
                Label {
                    visible: row.gone
                    width: parent.width
                    //% "Moved or deleted outside Lautta"
                    text: qsTrId("lautta-tag-missing-hint")
                    font.pixelSize: Theme.fontSizeExtraSmall
                    color: Theme.secondaryColor
                    truncationMode: TruncationMode.Fade
                }
            }
        }

        VerticalScrollDecorator { }
    }

    RemorsePopup {
        id: remorsePopup
    }
}
