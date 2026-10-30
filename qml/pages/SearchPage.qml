// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../components"
import "../components/search"
import "../components/Open.js" as Open
import "../components/search/SearchText.js" as SearchText

Page {
    id: page

    property string rootUri
    property string query
    property bool searched
    readonly property bool remote: App.isRemote(rootUri)

    allowedOrientations: Orientation.All

    function summary() {
        var parts = [searchModel.matchMode === "glob"
                     //% "glob"
                     ? qsTrId("lautta-search-mode-glob")
                     //% "contains"
                     : qsTrId("lautta-search-mode-substring")]
        var types = []
        try {
            types = JSON.parse(searchModel.types || "[]")
        } catch (e) {
            types = []
        }
        parts.push(types.length === 0
                   //% "any type"
                   ? qsTrId("lautta-search-any-type")
                   : types.join(", "))
        return parts.join(" · ")
    }

    function start() {
        if (searchModel.start())
            searched = true
    }

    // Model query set here, not bound: filters dialog sets it too, which would drop a binding. Text
    // equal to the query is the dialog's own name coming back, already searched.
    function edited(text) {
        if (text === searchModel.query)
            return
        searchModel.query = text
        if (text.length === 0) {
            typing.stop()
            searchModel.clear()
        } else {
            typing.restart()
        }
    }

    function openFilters() {
        var dialog = pageStack.push(Qt.resolvedUrl("SearchFiltersPage.qml"), { "searchModel": searchModel })
        dialog.accepted.connect(function () {
            searchField.text = searchModel.query
            page.searched = true
        })
    }

    SearchModel {
        id: searchModel
        rootUri: page.rootUri
    }

    Timer {
        id: typing
        interval: 400
        onTriggered: page.start()
    }

    // Field and filter summary sit above the list, not in its header: each search resets model,
    // list makes first new row current and takes focus, so a header field lost the keyboard.
    Column {
        id: heading

        objectName: "searchHeading"
        anchors {
            top: parent.top
            left: parent.left
            right: parent.right
        }

        PageHeader {
            //% "Search"
            title: qsTrId("lautta-search-title")
            description: searchModel.rootName
        }

        SearchField {
            id: searchField

            objectName: "searchField"
            width: parent.width
            //% "Search"
            placeholderText: qsTrId("lautta-search-placeholder")
            inputMethodHints: Qt.ImhNoPredictiveText | Qt.ImhNoAutoUppercase
            EnterKey.iconSource: "image://theme/icon-m-enter-accept"
            EnterKey.onClicked: {
                typing.stop()
                page.start()
                focus = false
            }
            onTextChanged: page.edited(text)
            Component.onCompleted: {
                if (page.query.length === 0) {
                    forceActiveFocus()
                    return
                }
                text = page.query
                typing.stop()
                page.start()
            }
        }

        BackgroundItem {
            objectName: "searchFilters"
            width: parent.width
            height: Math.max(Theme.itemSizeExtraSmall, summaryLabel.height + Theme.paddingMedium)
            onClicked: page.openFilters()

            Label {
                id: summaryLabel

                anchors {
                    left: parent.left
                    right: parent.right
                    leftMargin: Theme.horizontalPageMargin + Theme.itemSizeSmall
                    rightMargin: Theme.horizontalPageMargin
                    verticalCenter: parent.verticalCenter
                }
                text: page.summary()
                truncationMode: TruncationMode.Fade
                color: parent.highlighted ? Theme.highlightColor : Theme.secondaryColor
                font.pixelSize: Theme.fontSizeSmall
            }
        }
    }

    SilicaListView {
        id: list

        objectName: "searchList"
        anchors {
            top: heading.bottom
            left: parent.left
            right: parent.right
            bottom: parent.bottom
            bottomMargin: progressPanel.visibleSize
        }
        // Rows otherwise draw over heading when scrolled.
        clip: true
        model: searchModel

        section {
            property: "section"
            delegate: Item {
                width: list.width
                height: Theme.itemSizeSmall

                PlaceLabel {
                    objectName: "searchSection"
                    x: Theme.horizontalPageMargin
                    width: parent.width - 2 * x
                    anchors {
                        verticalCenter: parent.verticalCenter
                        verticalCenterOffset: Theme.paddingMedium / 2
                    }
                    text: section
                    remote: page.remote
                    horizontalAlignment: Text.AlignRight
                    color: Theme.highlightColor
                    fontSize: Theme.fontSizeSmall
                }
            }
        }

        delegate: ListItem {
            id: item

            contentHeight: Theme.itemSizeSmall
            onClicked: Open.open(pageStack, App, typeof Operations !== "undefined" ? Operations : null,
                                 model.uri, model.isDir, "", model.folderUri)

            ResultIcon {
                id: icon

                x: Theme.horizontalPageMargin
                anchors.verticalCenter: parent.verticalCenter
                category: model.category
                isDir: model.isDir
                highlighted: item.highlighted
            }

            Column {
                anchors {
                    left: icon.right
                    leftMargin: Theme.paddingLarge
                    right: parent.right
                    rightMargin: Theme.horizontalPageMargin
                    verticalCenter: parent.verticalCenter
                }

                Label {
                    width: parent.width
                    textFormat: Text.StyledText
                    text: SearchText.highlightedName(model.name, model.matchStart, model.matchLength,
                                                     String(Theme.highlightColor))
                    truncationMode: TruncationMode.Fade
                    color: item.highlighted ? Theme.highlightColor : Theme.primaryColor
                }
                Label {
                    width: parent.width
                    text: SearchText.sizeAndDate(model.size, model.modified, model.isDir)
                    truncationMode: TruncationMode.Fade
                    color: item.highlighted ? Theme.secondaryHighlightColor : Theme.secondaryColor
                    font.pixelSize: Theme.fontSizeExtraSmall
                }
            }
        }

        ViewPlaceholder {
            enabled: list.count === 0 && !searchModel.running
            text: page.searched && searchModel.errorKind.length === 0
                  //% "No matches"
                  ? qsTrId("lautta-search-none")
                  : (searchModel.errorKind.length > 0
                     //% "Search stopped"
                     ? qsTrId("lautta-search-failed")
                     //% "Search"
                     : qsTrId("lautta-search-idle"))
            hintText: page.searched
                      ? (searchModel.errorKind.length > 0
                         ? searchModel.errorMessage
                         //% "Try another name or loosen the filters"
                         : qsTrId("lautta-search-none-hint"))
                      //% "Type part of a name, or a pattern such as *.pdf"
                      : qsTrId("lautta-search-idle-hint")
        }

        VerticalScrollDecorator { }
    }

    DockedPanel {
        id: progressPanel

        width: parent.width
        height: Theme.itemSizeMedium
        dock: Dock.Bottom
        open: searchModel.running
        modal: false
        background: BarBackground { }

        Row {
            anchors {
                left: parent.left
                right: parent.right
                leftMargin: Theme.horizontalPageMargin
                rightMargin: Theme.horizontalPageMargin
                verticalCenter: parent.verticalCenter
            }
            spacing: Theme.paddingLarge

            BusyIndicator {
                size: BusyIndicatorSize.Small
                running: searchModel.running
                anchors.verticalCenter: parent.verticalCenter
            }
            Label {
                width: parent.width - parent.spacing * 2 - Theme.itemSizeSmall * 2
                anchors.verticalCenter: parent.verticalCenter
                //% "Searching… %n results"
                text: qsTrId("lautta-search-progress", searchModel.hitCount)
                color: Theme.secondaryColor
                font.pixelSize: Theme.fontSizeSmall
                truncationMode: TruncationMode.Fade
            }
            Button {
                anchors.verticalCenter: parent.verticalCenter
                preferredWidth: Theme.buttonWidthSmall
                //% "Stop"
                text: qsTrId("lautta-search-stop")
                onClicked: searchModel.cancel()
            }
        }
    }
}
