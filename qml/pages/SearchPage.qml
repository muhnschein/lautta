// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import QtQml.Models 2.2
import Sailfish.Silica 1.0
import Lautta 1.0
import "../components"
import "../components/directory"
import "../components/directory/GridRows.js" as GridRows
import "../components/search"
import "../components/viewers"
import "../components/Open.js" as Open
import "../components/ErrorText.js" as ErrorText
import "../components/search/SearchText.js" as SearchText
import "../components/search/SearchGroups.js" as SearchGroups
import "../components/search/Marks.js" as Marks

Page {
    id: page

    property string rootUri
    property string scope: rootUri.length > 0 ? "folder" : (searchModel.hasServers() ? "everywhere" : "device")
    property bool searched
    property string typeFilter: ""
    property string datePreset: "any"
    property bool includeHidden
    readonly property real menuRoom: scopeMenu.height + typeMenu.height + dateMenu.height
    readonly property Item openMenu: scopeMenu.active ? scopeMenu
                                     : typeMenu.active ? typeMenu
                                     : dateMenu.active ? dateMenu : null
    readonly property var typeChoices: [
        //% "Any type"
        { "value": "", "label": qsTrId("lautta-search-type-any") },
        //% "Folders"
        { "value": "folder", "label": qsTrId("lautta-filters-type-folder") },
        //% "Images"
        { "value": "image", "label": qsTrId("lautta-filters-type-image") },
        //% "Videos"
        { "value": "video", "label": qsTrId("lautta-filters-type-video") },
        //% "Audio"
        { "value": "audio", "label": qsTrId("lautta-filters-type-audio") },
        //% "Documents"
        { "value": "document", "label": qsTrId("lautta-filters-type-document") },
        //% "Archives"
        { "value": "archive", "label": qsTrId("lautta-search-pill-archive") },
        //% "Text"
        { "value": "text", "label": qsTrId("lautta-search-pill-text") }
    ]
    readonly property var dateChoices: [
        //% "Any time"
        { "value": "any", "label": qsTrId("lautta-filters-date-any") },
        //% "Today"
        { "value": "today", "label": qsTrId("lautta-filters-date-today") },
        //% "Past week"
        { "value": "week", "label": qsTrId("lautta-filters-date-week") },
        //% "Past month"
        { "value": "month", "label": qsTrId("lautta-filters-date-month") },
        //% "This year"
        { "value": "year", "label": qsTrId("lautta-filters-date-year") }
    ]
    readonly property bool remote: App.isRemote(rootUri)
    readonly property bool wide: scope !== "folder"
    readonly property var groups: JSON.parse(searchModel.groupsJson || "[]")
    readonly property var recentSearches: JSON.parse(searchModel.recentSearchesJson || "[]")
    readonly property bool gridMode: JSON.parse(App.settingsJson).view_mode === "grid"
    readonly property real tileWidth: Math.min(Screen.width, Screen.height) / gridMetrics.portraitColumns
    readonly property int columns: GridRows.columns(width, tileWidth, gridMetrics.portraitColumns)
    readonly property real cellWidth: width / columns
    property string locationsJson: searchModel.scopeLocationsJson()
    readonly property var scopeLocations: JSON.parse(locationsJson || "[]")
    readonly property var scopeChoices: SearchGroups.scopeChoices(rootUri.length > 0, searchModel.rootName,
                                                                  scopeLocations)

    allowedOrientations: Orientation.All
    onStatusChanged: {
        if (status === PageStatus.Active && !searched)
            searchField.forceActiveFocus()
    }

    function start() {
        searchModel.types = JSON.stringify(page.typeFilter === "" ? [] : [page.typeFilter])
        searchModel.datePreset = page.datePreset
        searchModel.includeHidden = page.includeHidden
        if (searchModel.start()) {
            searched = true
        } else {
            searchModel.clear()
            searched = false
        }
    }

    function edited(text) {
        if (text === searchModel.query)
            return
        searchModel.query = text
        typing.restart()
    }

    function refine() {
        typing.stop()
        page.start()
    }

    onScopeChoicesChanged: {
        if (!SearchGroups.hasChoice(scopeChoices, scope)) {
            scope = "device"
            if (searched)
                refine()
        }
    }

    function refreshLocations() {
        var json = searchModel.scopeLocationsJson()
        if (json !== page.locationsJson)
            page.locationsJson = json
    }

    function openMenuOf(menu) {
        if (menu === scopeMenu)
            page.refreshLocations()
        if (page.openMenu && page.openMenu !== menu)
            page.openMenu.close()
        if (!menu.active)
            menu.open(stripHost)
    }

    function toggleMenu(menu) {
        if (menu.active)
            menu.close()
        else
            page.openMenuOf(menu)
    }

    function choose(kind, value) {
        if (kind === "scope")
            page.scope = value
        else if (kind === "type")
            page.typeFilter = value
        else if (kind === "date")
            page.datePreset = value
        page.refine()
    }

    function useRecent(text) {
        searchField.text = text
        typing.stop()
        page.start()
        searchField.focus = false
    }

    function openHit(row, tileSize) {
        Open.open(pageStack, App, typeof Operations !== "undefined" ? Operations : null,
                  row.uri, row.isDir, "", row.folderUri, tileSize)
    }

    function showMode() {
        list.model = null
        list.delegate = gridMode ? gridRow : listRow
        list.model = gridMode ? gridRows : searchModel
    }

    function fitGrid() {
        var sections = []
        for (var i = 0; gridMode && i < cells.items.count; ++i)
            sections.push(cells.items.get(i).model.section)
        var rows = GridRows.sectionRows(sections, columns).map(function (r) {
            return { "section": r.section, "first": r.first, "count": r.count }
        })
        GridRows.fitRows(gridRows, rows)
    }

    onGridModeChanged: {
        fitGrid()
        showMode()
    }
    onColumnsChanged: fitGrid()
    Component.onCompleted: if (gridMode) showMode()

    SearchModel {
        id: searchModel

        rootUri: page.rootUri
        scope: page.scope
    }

    Timer {
        id: typing

        interval: 400
        onTriggered: page.start()
    }

    ViewerPrefetch { id: prefetch }

    GridMetrics { id: gridMetrics }

    DelegateModel {
        id: cells

        model: searchModel
        delegate: Item { }
    }

    Connections {
        target: cells.items
        onChanged: page.fitGrid()
    }

    ListModel { id: gridRows }

    SilicaFlickable {
        id: pulleyHost

        objectName: "searchPulleyHost"
        anchors.fill: parent
        contentWidth: width
        contentHeight: height + page.menuRoom

        PullDownMenu {
            visible: page.recentSearches.length > 0

            MenuItem {
                objectName: "clearRecentSearches"
                visible: page.recentSearches.length > 0
                //% "Clear recent searches"
                text: qsTrId("lautta-search-clear-recent")
                onClicked: {
                    //% "Clearing recent searches"
                    remorsePopup.execute(qsTrId("lautta-search-clearing-recent"), function () {
                        searchModel.clearRecent()
                    }, App.setting("remorse_seconds") * 1000)
                }
            }
        }

        HeaderBand {
            anchors.fill: heading
        }

        Item {
            id: heading

            objectName: "searchHeading"
            anchors {
                top: parent.top
                left: parent.left
                right: parent.right
            }
            height: stripHost.y + stripHost.height

            PageHeader {
                id: pageHeader

                //% "Search"
                title: qsTrId("lautta-search-title")
            }

            SearchField {
                id: searchField

                objectName: "searchField"
                anchors.top: pageHeader.bottom
                width: parent.width
                placeholderText: page.scope === "folder"
                                 //% "Search in %1"
                                 ? qsTrId("lautta-search-placeholder-in").arg(searchModel.rootName)
                                 //% "Search"
                                 : qsTrId("lautta-search-placeholder")
                inputMethodHints: Qt.ImhNoPredictiveText | Qt.ImhNoAutoUppercase
                EnterKey.iconSource: "image://theme/icon-m-enter-accept"
                EnterKey.onClicked: {
                    typing.stop()
                    page.start()
                    focus = false
                }
                onTextChanged: page.edited(text)
                Component.onCompleted: forceActiveFocus()
            }

            Item {
                id: stripHost

                objectName: "searchPillHost"
                anchors.top: searchField.bottom
                width: parent.width
                height: pillStrip.height + page.menuRoom

                Flickable {
                    id: pillStrip

                    objectName: "searchPills"
                    width: parent.width
                    height: pills.height + Theme.paddingMedium
                    contentWidth: pills.width + 2 * Theme.horizontalPageMargin
                    flickableDirection: Flickable.HorizontalFlick
                    boundsBehavior: Flickable.StopAtBounds
                    clip: true

                    Row {
                        id: pills

                        x: Theme.horizontalPageMargin
                        spacing: Theme.paddingMedium

                        PillButton {
                            objectName: "scopePill"
                            icon: SearchGroups.scopeIcon(page.scope, page.scopeChoices)
                            text: SearchGroups.labelOf(page.scopeChoices, page.scope)
                            checked: true
                            open: scopeMenu.active
                            onClicked: page.toggleMenu(scopeMenu)
                            onPressAndHold: page.openMenuOf(scopeMenu)
                        }

                        PillButton {
                            objectName: "typePill"
                            text: page.typeFilter === ""
                                  //% "Type"
                                  ? qsTrId("lautta-search-pill-type")
                                  : SearchGroups.labelOf(page.typeChoices, page.typeFilter)
                            checked: page.typeFilter !== ""
                            open: typeMenu.active
                            onClicked: page.toggleMenu(typeMenu)
                            onPressAndHold: page.openMenuOf(typeMenu)
                        }

                        PillButton {
                            objectName: "datePill"
                            text: page.datePreset === "any"
                                  //% "Modified"
                                  ? qsTrId("lautta-filters-modified")
                                  : SearchGroups.labelOf(page.dateChoices, page.datePreset)
                            checked: page.datePreset !== "any"
                            open: dateMenu.active
                            onClicked: page.toggleMenu(dateMenu)
                            onPressAndHold: page.openMenuOf(dateMenu)
                        }

                        PillButton {
                            objectName: "hiddenPill"
                            //% "Hidden"
                            text: qsTrId("lautta-search-pill-hidden")
                            checked: page.includeHidden
                            onClicked: {
                                page.includeHidden = !page.includeHidden
                                page.refine()
                            }
                        }
                    }
                }
            }
        }

        PillMenu {
            id: scopeMenu

            objectName: "scopeMenu"
            current: page.scope
            choices: page.scopeChoices
            onChosen: page.choose("scope", value)
        }

        PillMenu {
            id: typeMenu

            objectName: "typeMenu"
            current: page.typeFilter
            choices: page.typeChoices
            onChosen: page.choose("type", value)
        }

        PillMenu {
            id: dateMenu

            objectName: "dateMenu"
            current: page.datePreset
            choices: page.dateChoices
            onChosen: page.choose("date", value)
        }

        SilicaFlickable {
            id: recentView

            objectName: "searchRecents"
            anchors {
                top: heading.bottom
                left: parent.left
                right: parent.right
                bottom: parent.bottom
            }
            visible: !page.searched && page.recentSearches.length > 0
            contentHeight: recentColumn.height
            clip: true

            Column {
                id: recentColumn

                width: parent.width

                SectionHeader {
                    //% "Recent searches"
                    text: qsTrId("lautta-filters-recent")
                }

                Repeater {
                    model: page.recentSearches

                    BackgroundItem {
                        objectName: "recentSearch"
                        width: recentColumn.width
                        height: Theme.itemSizeSmall
                        onClicked: page.useRecent(modelData)

                        Image {
                            id: clock

                            x: Theme.horizontalPageMargin
                            anchors.verticalCenter: parent.verticalCenter
                            source: "image://theme/icon-m-clock?" + (parent.highlighted ? Theme.highlightColor : Theme.primaryColor)
                        }
                        Label {
                            anchors {
                                left: clock.right
                                leftMargin: Theme.paddingLarge
                                right: parent.right
                                rightMargin: Theme.horizontalPageMargin
                                verticalCenter: parent.verticalCenter
                            }
                            text: modelData
                            textFormat: Text.PlainText
                            truncationMode: TruncationMode.Fade
                            color: parent.highlighted ? Theme.highlightColor : Theme.primaryColor
                        }
                    }
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
            visible: !recentView.visible
            clip: true
            model: searchModel
            delegate: listRow

            section {
                property: "section"
                criteria: ViewSection.FullString
                delegate: Item {
                    readonly property var group: page.wide ? SearchGroups.find(page.groups, section) : null

                    width: list.width
                    height: Theme.itemSizeSmall

                    PlaceLabel {
                        objectName: "searchSection"
                        visible: !page.wide
                        x: Theme.horizontalPageMargin
                        width: parent.width - 2 * x
                        anchors {
                            verticalCenter: parent.verticalCenter
                            verticalCenterOffset: Theme.paddingMedium / 2
                        }
                        text: page.wide ? "" : section
                        remote: page.remote
                        horizontalAlignment: Text.AlignRight
                        color: Theme.highlightColor
                        fontSize: Theme.fontSizeSmall
                    }

                    GroupHeader {
                        visible: page.wide
                        group: parent.group
                    }
                }
            }

            footer: Column {
                width: list.width

                Repeater {
                    model: page.wide ? SearchGroups.withoutRows(page.groups) : []

                    GroupHeader {
                        objectName: "searchGroupFooter"
                        width: list.width
                        group: modelData
                    }
                }
            }

            ViewPlaceholder {
                readonly property bool outcome: page.searched && list.count === 0 && !searchModel.running

                objectName: "searchPlaceholder"
                enabled: outcome && !recentView.visible
                text: searchModel.errorKind.length === 0
                      //% "No matches"
                      ? qsTrId("lautta-search-none")
                      //% "Search failed"
                      : qsTrId("lautta-search-failed")
                hintText: searchModel.errorKind.length > 0
                          ? ErrorText.message(searchModel.errorKind, { "location": App.locationName(page.rootUri), "open": true })
                          //% "Try another name or loosen the filters"
                          : qsTrId("lautta-search-none-hint")
            }

            VerticalScrollDecorator { }
        }
    }

    RemorsePopup {
        id: remorsePopup
    }

    Component {
        id: listRow

        ListItem {
            id: item

            contentHeight: Theme.itemSizeSmall
            onPressed: if (!model.isDir) prefetch.start(model.uri, model.mimeType)
            onClicked: page.openHit(model, 0)
            onPressAndHold: prefetch.stop()
            onCanceled: prefetch.stop()

            FileIcon {
                id: icon

                objectName: "searchIcon"
                x: Theme.horizontalPageMargin
                anchors.verticalCenter: parent.verticalCenter
                category: model.category
                thumbnailSource: model.thumbnailSource
                mimeType: model.mimeType
                highlighted: item.highlighted
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
                    readonly property string marked: Marks.marked(model.name, Marks.needle(model.name, model.matchStart, model.matchLength),
                                                                  String(Theme.highlightColor))

                    objectName: "searchResultName"
                    width: parent.width
                    textFormat: marked.length > 0 ? Text.StyledText : Text.PlainText
                    text: marked.length > 0 ? marked : model.name
                    truncationMode: TruncationMode.Fade
                    color: item.highlighted ? Theme.highlightColor : Theme.primaryColor
                }
                Label {
                    width: parent.width
                    text: page.wide
                          ? [model.place, SearchText.sizeAndDate(model.size, model.modified, model.isDir)]
                            .filter(function (s) { return s.length > 0 }).join(" · ")
                          : SearchText.sizeAndDate(model.size, model.modified, model.isDir)
                    truncationMode: TruncationMode.Fade
                    color: item.highlighted ? Theme.secondaryHighlightColor : Theme.secondaryColor
                    font.pixelSize: Theme.fontSizeExtraSmall
                }
            }
        }
    }

    Component {
        id: gridRow

        Item {
            id: row

            readonly property int first: model.first
            readonly property int end: model.first + model.count

            function regroup() {
                var all = rowCells.items
                var to = Math.min(end, all.count)
                var kept = []
                for (var i = inRow.count - 1; i >= 0; --i) {
                    var at = inRow.get(i).itemsIndex
                    if (at < first || at >= to)
                        inRow.remove(i, 1)
                    else
                        kept.push(at)
                }
                for (var j = first; j < to; ++j) {
                    if (kept.indexOf(j) < 0)
                        all.addGroups(j, 1, "inRow")
                }
            }

            width: list.width
            height: tiles.height
            onFirstChanged: regroup()
            onEndChanged: regroup()
            Component.onCompleted: regroup()

            DelegateModel {
                id: rowCells

                model: searchModel
                groups: DelegateModelGroup {
                    id: inRow

                    name: "inRow"
                }
                filterOnGroup: "inRow"
                delegate: BackgroundItem {
                    id: cell

                    readonly property real gap: gridMetrics.gap

                    objectName: "searchGridTile"
                    x: (index - row.first) * width
                    width: page.cellWidth
                    height: tiles.height
                    implicitHeight: gridMetrics.cellHeight(width, caption.implicitHeight + place.height)
                    onPressed: if (!model.isDir) prefetch.start(model.uri, model.mimeType)
                    onClicked: page.openHit(model, tileIcon.thumbnailReady ? Math.round(tileIcon.width) : 0)
                    onPressAndHold: prefetch.stop()
                    onCanceled: prefetch.stop()

                    Item {
                        id: tile

                        x: cell.gap
                        y: cell.gap
                        width: cell.width - 2 * cell.gap
                        height: width

                        Rectangle {
                            anchors.fill: parent
                            visible: !tileIcon.thumbnailReady
                            color: Theme.rgba(Theme.highlightBackgroundColor, Theme.highlightBackgroundOpacity / 3)
                        }
                        TextThumb {
                            id: lines

                            uri: model.textThumb ? model.uri : ""
                        }
                        FileIcon {
                            id: tileIcon

                            anchors.fill: parent
                            size: tile.width
                            glyphSize: gridMetrics.glyphSize(tile.width)
                            category: model.category
                            thumbnailSource: model.thumbnailSource
                            mimeType: model.mimeType
                            previewText: lines.text
                            previewMono: model.textMono
                            typeIcon: model.typeIcon
                            highlighted: cell.highlighted
                        }
                    }
                    GridCaption {
                        id: caption

                        anchors {
                            top: tile.bottom
                            topMargin: Theme.paddingSmall / 2
                            left: tile.left
                            right: tile.right
                        }
                        metrics: gridMetrics
                        name: model.name
                        mark: Marks.needle(model.name, model.matchStart, model.matchLength)
                        emphasised: cell.highlighted
                        pressed: cell.highlighted
                    }
                    PlaceLabel {
                        id: place

                        anchors {
                            top: caption.bottom
                            left: tile.left
                            right: tile.right
                        }
                        text: page.wide ? model.place
                                        : SearchText.sizeAndDate(model.size, model.modified, model.isDir)
                        remote: false
                        fontSize: Theme.fontSizeTiny
                        horizontalAlignment: Text.AlignHCenter
                        color: cell.highlighted ? Theme.secondaryHighlightColor : Theme.secondaryColor
                    }
                }
            }

            Connections {
                target: rowCells.items
                onChanged: row.regroup()
            }

            Item {
                id: tiles

                width: parent.width
                height: GridRows.tallest(children)

                Repeater {
                    model: rowCells
                }
            }
        }
    }
    DockedPanel {
        id: progressPanel

        width: parent.width
        height: Theme.itemSizeMedium
        dock: Dock.Bottom
        open: searchModel.running
        modal: false
        background: BarBackground { }

        BusyIndicator {
            id: busy

            x: Theme.horizontalPageMargin
            anchors.verticalCenter: parent.verticalCenter
            size: BusyIndicatorSize.Small
            running: searchModel.running
        }
        Label {
            objectName: "searchProgress"
            anchors {
                left: busy.right
                leftMargin: Theme.paddingLarge
                right: stopButton.left
                rightMargin: Theme.paddingMedium
                verticalCenter: parent.verticalCenter
            }
            //% "Searching… %n results"
            text: qsTrId("lautta-search-progress", searchModel.hitCount)
            color: Theme.secondaryColor
            font.pixelSize: Theme.fontSizeSmall
            truncationMode: TruncationMode.Fade
        }
        IconButton {
            id: stopButton

            objectName: "searchStop"
            anchors {
                right: parent.right
                rightMargin: Theme.horizontalPageMargin - Theme.paddingMedium
                verticalCenter: parent.verticalCenter
            }
            icon.source: "image://theme/icon-m-stop"
            onClicked: searchModel.cancel()
        }
    }
}
