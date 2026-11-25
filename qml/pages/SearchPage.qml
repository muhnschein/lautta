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

Page {
    id: page

    property string rootUri
    property string scope: rootUri.length > 0 ? "folder" : (searchModel.hasServers() ? "everywhere" : "device")
    property bool searched
    property string choosing: ""
    property var types: []
    property string datePreset: "any"
    property string sizePreset: "any"
    property bool includeHidden
    readonly property bool remote: App.isRemote(rootUri)
    readonly property bool wide: scope !== "folder"
    readonly property var groups: JSON.parse(searchModel.groupsJson || "[]")
    readonly property var recentSearches: JSON.parse(searchModel.recentSearchesJson || "[]")
    readonly property bool gridMode: JSON.parse(App.settingsJson).view_mode === "grid"
    readonly property real tileWidth: Math.min(Screen.width, Screen.height) / gridMetrics.portraitColumns
    readonly property int columns: GridRows.columns(width, tileWidth, gridMetrics.portraitColumns)
    readonly property real cellWidth: width / columns
    readonly property var scopeChoices: {
        var out = []
        if (rootUri.length > 0)
            out.push("folder")
        out.push("device")
        if (scope === "everywhere" || searchModel.hasServers())
            out.push("everywhere")
        return out
    }

    allowedOrientations: Orientation.All

    function start() {
        searchModel.types = JSON.stringify(page.types)
        searchModel.datePreset = page.datePreset
        searchModel.sizePreset = page.sizePreset
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

    function toggleChoices(kind) {
        page.choosing = page.choosing === kind ? "" : kind
    }

    function choose(kind, value) {
        page.choosing = ""
        if (kind === "scope")
            page.scope = value
        else if (kind === "date")
            page.datePreset = value
        else if (kind === "size")
            page.sizePreset = value
        page.refine()
    }

    function toggleType(type) {
        page.types = SearchGroups.toggled(page.types, type)
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

    HeaderBand {
        anchors.fill: heading
    }

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
        }

        SearchField {
            id: searchField

            objectName: "searchField"
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
                    icon: SearchGroups.scopeIcon(page.scope)
                    text: SearchGroups.scopeName(page.scope, searchModel.rootName)
                    checked: true
                    open: page.choosing === "scope"
                    onClicked: page.toggleChoices("scope")
                }

                Repeater {
                    model: [
                        //% "Folders"
                        { "type": "folder", "label": qsTrId("lautta-filters-type-folder") },
                        //% "Images"
                        { "type": "image", "label": qsTrId("lautta-filters-type-image") },
                        //% "Videos"
                        { "type": "video", "label": qsTrId("lautta-filters-type-video") },
                        //% "Audio"
                        { "type": "audio", "label": qsTrId("lautta-filters-type-audio") },
                        //% "Documents"
                        { "type": "document", "label": qsTrId("lautta-filters-type-document") },
                        //% "Archives"
                        { "type": "archive", "label": qsTrId("lautta-search-pill-archive") },
                        //% "Text"
                        { "type": "text", "label": qsTrId("lautta-search-pill-text") }
                    ]

                    PillButton {
                        objectName: "typePill"
                        readonly property string type: modelData.type
                        text: modelData.label
                        checked: page.types.indexOf(modelData.type) >= 0
                        onClicked: page.toggleType(modelData.type)
                    }
                }

                PillButton {
                    objectName: "datePill"
                    text: page.datePreset === "any"
                          //% "Modified"
                          ? qsTrId("lautta-filters-modified")
                          : dateChoices.labelOf(page.datePreset)
                    checked: page.datePreset !== "any"
                    open: page.choosing === "date"
                    onClicked: page.toggleChoices("date")
                }

                PillButton {
                    objectName: "sizePill"
                    text: page.sizePreset === "any"
                          //% "Size"
                          ? qsTrId("lautta-filters-size")
                          : sizeChoices.labelOf(page.sizePreset)
                    checked: page.sizePreset !== "any"
                    open: page.choosing === "size"
                    onClicked: page.toggleChoices("size")
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

        PillChoices {
            id: scopeMenu

            objectName: "scopeChoices"
            shown: page.choosing === "scope"
            current: page.scope
            choices: page.scopeChoices.map(function (s) {
                return { "value": s, "label": SearchGroups.scopeName(s, searchModel.rootName) }
            })
            onChosen: page.choose("scope", value)
        }

        PillChoices {
            id: dateChoices

            objectName: "dateChoices"
            shown: page.choosing === "date"
            current: page.datePreset
            choices: [
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
            onChosen: page.choose("date", value)
        }

        PillChoices {
            id: sizeChoices

            objectName: "sizeChoices"
            shown: page.choosing === "size"
            current: page.sizePreset
            choices: [
                //% "Any size"
                { "value": "any", "label": qsTrId("lautta-filters-size-any") },
                //% "Larger than 1 MB"
                { "value": "gt1m", "label": qsTrId("lautta-filters-size-gt1m") },
                //% "Larger than 10 MB"
                { "value": "gt10m", "label": qsTrId("lautta-filters-size-gt10m") },
                //% "Larger than 100 MB"
                { "value": "gt100m", "label": qsTrId("lautta-filters-size-gt100m") },
                //% "Smaller than 100 kB"
                { "value": "lt100k", "label": qsTrId("lautta-filters-size-lt100k") },
                //% "Smaller than 1 MB"
                { "value": "lt1m", "label": qsTrId("lautta-filters-size-lt1m") }
            ]
            onChosen: page.choose("size", value)
        }
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

                Row {
                    objectName: "searchGroupStatus"
                    visible: page.wide
                    x: Theme.horizontalPageMargin
                    width: groupName.x - x - Theme.paddingMedium
                    anchors {
                        verticalCenter: parent.verticalCenter
                        verticalCenterOffset: Theme.paddingMedium / 2
                    }
                    spacing: Theme.paddingSmall

                    BusyIndicator {
                        size: BusyIndicatorSize.ExtraSmall
                        running: visible
                        visible: page.wide && group !== null && group.state === "searching"
                        anchors.verticalCenter: parent.verticalCenter
                    }
                    Label {
                        width: parent.width
                        text: SearchGroups.status(group)
                        truncationMode: TruncationMode.Fade
                        font.pixelSize: Theme.fontSizeExtraSmall
                        color: SearchGroups.failed(group) ? Theme.errorColor : Theme.secondaryColor
                        anchors.verticalCenter: parent.verticalCenter
                    }
                }

                Label {
                    id: groupName

                    objectName: "searchGroupName"
                    visible: page.wide
                    anchors {
                        right: parent.right
                        rightMargin: Theme.horizontalPageMargin
                        verticalCenter: parent.verticalCenter
                        verticalCenterOffset: Theme.paddingMedium / 2
                    }
                    text: SearchGroups.name(group)
                    color: Theme.highlightColor
                    font.pixelSize: Theme.fontSizeSmall
                }
            }
        }

        ViewPlaceholder {
            enabled: list.count === 0 && !searchModel.running && !recentView.visible
            text: page.searched && searchModel.errorKind.length === 0
                  //% "No matches"
                  ? qsTrId("lautta-search-none")
                  : (searchModel.errorKind.length > 0
                     //% "Search failed"
                     ? qsTrId("lautta-search-failed")
                     //% "Search"
                     : qsTrId("lautta-search-idle"))
            hintText: page.searched
                      ? (searchModel.errorKind.length > 0
                         ? ErrorText.message(searchModel.errorKind, { "location": App.locationName(page.rootUri), "open": true })
                         //% "Try another name or loosen the filters"
                         : qsTrId("lautta-search-none-hint"))
                      //% "Type part of a name, or a pattern such as *.pdf"
                      : qsTrId("lautta-search-idle-hint")
        }

        VerticalScrollDecorator { }
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
                    width: parent.width
                    textFormat: Text.StyledText
                    text: SearchText.highlightedName(model.name, model.matchStart, model.matchLength,
                                                     String(Theme.highlightColor))
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
