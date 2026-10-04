// SPDX-License-Identifier: LGPL-2.1-or-later
// Search here (SRC-2..4): recursive name search below a folder with results
// streaming in, grouped by folder (design: Search, SearchPulley).
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../components"
import "../components/search"
import "../components/Open.js" as Open
import "../components/search/SearchText.js" as SearchText

Page {
    id: page

    // The folder (or location root) searched.
    property string rootUri
    // The text in the search field.
    property string queryText
    // Whether a search was run since the page opened (for the placeholder).
    property bool searched

    allowedOrientations: Orientation.All

    function summary() {
        //% "In %1"
        var parts = [qsTrId("lautta-search-in").arg(searchModel.rootName)]
        parts.push(searchModel.matchMode === "glob"
                   //% "glob"
                   ? qsTrId("lautta-search-mode-glob")
                   //% "contains"
                   : qsTrId("lautta-search-mode-substring"))
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

    function openFilters() {
        var dialog = pageStack.push(Qt.resolvedUrl("SearchFiltersPage.qml"), { "searchModel": searchModel })
        dialog.accepted.connect(function () {
            page.queryText = searchModel.query
            page.searched = true
        })
    }

    function pickRoot() {
        var picker = pageStack.push(Qt.resolvedUrl("../dialogs/FolderPickerDialog.qml"), {
            //% "Search in"
            "title": qsTrId("lautta-search-pick-title"),
            //% "Search"
            "acceptText": qsTrId("lautta-search-pick-accept"),
            "startUri": searchModel.rootUri
        })
        picker.accepted.connect(function () {
            searchModel.rootUri = picker.selectedUri
            page.start()
        })
    }

    SearchModel {
        id: searchModel
        rootUri: page.rootUri
        query: page.queryText
    }

    Timer {
        id: typing
        interval: 400
        onTriggered: page.start()
    }

    SilicaListView {
        id: list

        anchors {
            fill: parent
            bottomMargin: progressPanel.visibleSize
        }
        clip: true
        model: searchModel

        PullDownMenu {
            MenuItem {
                //% "Search in another location"
                text: qsTrId("lautta-search-another-location")
                onClicked: page.pickRoot()
            }
            MenuItem {
                //% "Filters"
                text: qsTrId("lautta-search-filters")
                onClicked: page.openFilters()
            }
        }

        header: Column {
            width: list.width

            Binding {
                target: searchField
                property: "text"
                value: page.queryText
            }

            SearchField {
                id: searchField

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
                onTextChanged: {
                    page.queryText = text
                    if (text.length === 0) {
                        typing.stop()
                        searchModel.clear()
                    } else {
                        typing.restart()
                    }
                }
                Component.onCompleted: forceActiveFocus()
            }

            BackgroundItem {
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

        section {
            property: "section"
            delegate: SectionHeader {
                text: section
                horizontalAlignment: Text.AlignRight
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

    // Progress and Stop while the search runs (design: Search).
    DockedPanel {
        id: progressPanel

        width: parent.width
        height: Theme.itemSizeMedium
        dock: Dock.Bottom
        open: searchModel.running
        // The panel hides the progress row once the search is done.
        modal: false

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
