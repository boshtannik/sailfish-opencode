import QtQuick 2.6
import Sailfish.Silica 1.0

// Voice model management: collapsible sections, a language filter, live
// progress, icon actions and deletion with confirmation (RemorseItem).
// Downloaded models are split by engine (recognition / synthesis) so it is
// obvious which model does what; the remaining catalog entries follow, also
// per engine.
Page {
    id: vmPage

    property var appWindow
    property string filterLang: "auto"
    property bool dlRecExpanded: true
    property bool dlTtsExpanded: true
    property bool recExpanded: true
    property bool ttsExpanded: true

    // Fingerprint of the structural state (WITHOUT the progress counter):
    // the list is rebuilt only when the selection/state/filter/section changes,
    // not every 300 ms of polling — the scroll is not reset during a download.
    property string voiceFp: appWindow !== undefined ? vmPage.fingerprint() : ""

    // Section identifiers (local to the page).
    readonly property string secDlRec: "ondevice_recognition"
    readonly property string secDlTts: "ondevice_synthesis"
    readonly property string secRecognition: "recognition"
    readonly property string secSynthesis: "synthesis"

    onVoiceFpChanged: vmPage.fillModel()

    ListModel { id: voiceModel }

    function fingerprint() {
        var arr = appWindow.voiceModels || []
        var out = ""
        for (var i = 0; i < arr.length; i++)
            out += arr[i].model_id + "|" + arr[i].downloaded + "|"
                   + arr[i].state + ";"
        return out + "@" + appWindow.chosenStt + "#" + appWindow.chosenTts
               + "$" + vmPage.filterLang + "%" + vmPage.recExpanded
               + "!" + vmPage.ttsExpanded + "^" + vmPage.dlRecExpanded
               + "&" + vmPage.dlTtsExpanded
    }

    function langName(id) {
        if (id === "multilang" || id === "multilingual")
            return qsTr("Multilingual")
        var o = appWindow.voiceLangOptions || []
        for (var i = 0; i < o.length; i++)
            if (o[i].id === id)
                return o[i].name || id
        return id
    }

    function filterIndex() {
        var o = appWindow.voiceLangOptions || []
        for (var i = 0; i < o.length; i++)
            if (o[i].id === vmPage.filterLang)
                return i
        return 0
    }

    function visible(m) {
        if (vmPage.filterLang === "" || vmPage.filterLang === "auto")
            return true
        if (m.lang_id === "multilang" || m.lang_id === "multilingual")
            return true
        return m.lang_id === vmPage.filterLang
    }

    // Predicate for rows of the Recognition/Synthesis sections (shared by the list and the counter).
    // In "Auto" mode the section offers only not-yet-downloaded models;
    // when a specific language is selected — ALL models of that language (downloaded ones too),
    // so the language doesn't "disappear" from the section and models can be selected/deleted.
    function storeRows(m, engine) {
        if (m.engine !== engine || !vmPage.visible(m))
            return false
        if (vmPage.filterLang === "" || vmPage.filterLang === "auto")
            return !m.downloaded
        return true
    }

    function isSelected(m) {
        return m.model_id === appWindow.chosenStt
               || m.model_id === appWindow.chosenTts
    }

    function rowSub(m) {
        var s = vmPage.langName(m.lang_id) + " · " + appWindow.humanSize(m.size)
        if (m.state === appWindow.stateError)
            s += " · " + qsTr("download failed")
        else if (m.downloaded)
            s += " · " + qsTr("downloaded")
        return s
    }

    function glyph(m) {
        if (m.state === appWindow.stateDownloading)
            return "✕"
        if (m.state === appWindow.stateError)
            return "!"
        if (vmPage.isSelected(m))
            return "✓"
        if (m.downloaded)
            return "→"
        return "↓"
    }

    function glyphColor(m) {
        if (m.state === appWindow.stateDownloading || m.state === appWindow.stateError)
            return Theme.secondaryColor
        if (vmPage.isSelected(m))
            return Theme.highlightColor
        return Theme.primaryColor
    }

    function findModel(id) {
        var arr = appWindow.voiceModels || []
        for (var i = 0; i < arr.length; i++)
            if (arr[i].model_id === id)
                return arr[i]
        return null
    }

    function liveInfo(id) {
        var m = vmPage.findModel(id)
        if (!m || m.state !== appWindow.stateDownloading)
            return ""
        var size = parseInt(m.size), done = parseInt(m.done || 0)
        if (!size)
            return ""
        var pct = Math.min(99, Math.round(done / size * 100))
        return qsTr("Downloading… %1% (%2 / %3)")
               .arg(pct)
               .arg(appWindow.humanSize(done))
               .arg(appWindow.humanSize(size))
    }

    function liveRatio(id) {
        var m = vmPage.findModel(id)
        if (!m || m.state !== appWindow.stateDownloading)
            return 0
        var size = parseInt(m.size), done = parseInt(m.done || 0)
        if (!size)
            return 0
        return Math.max(0, Math.min(1, done / size))
    }

    function fillModel() {
        voiceModel.clear()
        var arr = appWindow.voiceModels || []

        // 1. Downloaded models, split by engine: the section titles say which
        //    model is for recognition and which is for synthesis.
        //    Always without a language filter.
        vmPage.appendDownloaded(arr, appWindow.engineStt,
                                qsTr("Downloaded · Recognition"),
                                vmPage.secDlRec, vmPage.dlRecExpanded)
        vmPage.appendDownloaded(arr, appWindow.engineTts,
                                qsTr("Downloaded · Synthesis"),
                                vmPage.secDlTts, vmPage.dlTtsExpanded)

        // 2. Recognition, 3. Synthesis — only not-yet-downloaded, with a language filter.
        vmPage.appendStore(arr, appWindow.engineStt, qsTr("Recognition"),
                           vmPage.secRecognition, vmPage.recExpanded)
        vmPage.appendStore(arr, appWindow.engineTts, qsTr("Synthesis"),
                           vmPage.secSynthesis, vmPage.ttsExpanded)
    }

    function downloadedRows(arr, engine) {
        var out = []
        for (var i = 0; i < arr.length; i++)
            if (arr[i].downloaded && arr[i].engine === engine)
                out.push(arr[i])
        return out
    }

    function appendDownloaded(arr, engine, title, kind, expanded) {
        var rows = vmPage.downloadedRows(arr, engine)
        // No empty sections: a downloaded group appears only once something
        // of that kind is actually on the device.
        if (rows.length === 0)
            return
        voiceModel.append({ isHeader: true, kind: kind,
                            title: title, total: rows.length, done: rows.length,
                            name: "", sub: "", dlstate: "", modelId: "",
                            engine: "", downloaded: false })
        if (expanded)
            for (var i = 0; i < rows.length; i++)
                vmPage.appendRow(rows[i])
    }

    function appendStore(arr, engine, title, kind, expanded) {
        var n = 0
        for (var i = 0; i < arr.length; i++)
            if (vmPage.storeRows(arr[i], engine))
                n++
        voiceModel.append({ isHeader: true, kind: kind,
                            title: title, total: n, done: n,
                            name: "", sub: "", dlstate: "", modelId: "",
                            engine: "", downloaded: false })
        if (expanded)
            for (var j = 0; j < arr.length; j++) {
                var mm = arr[j]
                if (vmPage.storeRows(mm, engine))
                    vmPage.appendRow(mm)
            }
    }

    function appendRow(m) {
        voiceModel.append({ isHeader: false, kind: m.engine,
                            title: "", total: 0, done: 0,
                            name: m.name, sub: vmPage.rowSub(m),
                            dlstate: m.state, modelId: m.model_id,
                            engine: m.engine, downloaded: !!m.downloaded })
    }

    function sectionExpanded(kind) {
        if (kind === vmPage.secRecognition)
            return vmPage.recExpanded
        if (kind === vmPage.secSynthesis)
            return vmPage.ttsExpanded
        if (kind === vmPage.secDlRec)
            return vmPage.dlRecExpanded
        return vmPage.dlTtsExpanded
    }

    function toggleSection(kind) {
        if (kind === vmPage.secRecognition)
            vmPage.recExpanded = !vmPage.recExpanded
        else if (kind === vmPage.secSynthesis)
            vmPage.ttsExpanded = !vmPage.ttsExpanded
        else if (kind === vmPage.secDlRec)
            vmPage.dlRecExpanded = !vmPage.dlRecExpanded
        else
            vmPage.dlTtsExpanded = !vmPage.dlTtsExpanded
    }

    function onRowTap(isHeader, kind, modelId, engine, dlstate, downloaded) {
        if (isHeader) {
            vmPage.toggleSection(kind)
            return
        }
        if (dlstate === appWindow.stateDownloading) {
            appWindow.voiceCmd(appWindow.cmdDownloadCancel, modelId)
        } else if (downloaded) {
            appWindow.voiceCmd(engine === appWindow.engineStt
                               ? appWindow.cmdSelectStt : appWindow.cmdSelectTts,
                               modelId)
        } else {
            appWindow.voiceCmd(appWindow.cmdDownload, modelId)
        }
    }

    function onRowMenu(isHeader, kind, modelId, dlstate, downloaded, name, wdg) {
        if (isHeader) {
            vmPage.toggleSection(kind)
            return
        }
        if (dlstate === appWindow.stateDownloading) {
            appWindow.voiceCmd(appWindow.cmdDownloadCancel, modelId)
        } else if (downloaded) {
            // RemorseItem.execute(item, text, callback) — the first argument is the
            // owner, "Removing…" is shown over its row.
            remorse.execute(wdg, qsTr("Removing «%1»…").arg(name),
                            function() {
                appWindow.voiceCmd(appWindow.cmdDelete, modelId)
            })
        } else {
            appWindow.voiceCmd(appWindow.cmdDownload, modelId)
        }
    }

    function downloadedCount() {
        var arr = appWindow.voiceModels || []
        var c = 0
        for (var i = 0; i < arr.length; i++)
            if (arr[i].downloaded)
                c++
        return c
    }

    function downloadedBytes() {
        var arr = appWindow.voiceModels || []
        var b = 0
        for (var i = 0; i < arr.length; i++)
            if (arr[i].downloaded)
                b += parseInt(arr[i].size || 0)
        return b
    }

    // Always open the page at the very top: the icon legend and the language
    // filter are the first thing the user needs to see (the list used to open
    // scrolled just past the filter).
    function scrollToTop() {
        list.positionViewAtBeginning()
    }
    Component.onCompleted: Qt.callLater(vmPage.scrollToTop)
    onStatusChanged: if (status === PageStatus.Active) Qt.callLater(vmPage.scrollToTop)

    RemorseItem { id: remorse }

    SilicaListView {
        id: list
        anchors.fill: parent
        model: voiceModel

        header: Item {
            width: parent.width
            height: hintsLabel.height + Theme.paddingMedium
                    + filterCombo.height + summaryLabel.height + Theme.paddingMedium

            // Icon legend — first thing on the page, before the language filter.
            Label {
                id: hintsLabel
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                text: qsTr("↓ download\n→ start using\n✓ selected\n✕ cancel\nlong press — remove")
                wrapMode: Text.Wrap
                color: Theme.secondaryColor
                font.pixelSize: Theme.fontSizeExtraSmall
            }

            ComboBox {
                id: filterCombo
                width: parent.width
                anchors.top: hintsLabel.bottom
                anchors.topMargin: Theme.paddingMedium
                label: qsTr("Language")
                currentIndex: vmPage.filterIndex()
                menu: ContextMenu {
                    Repeater {
                        model: appWindow.voiceLangOptions
                        MenuItem { text: modelData.name }
                    }
                }
                onCurrentIndexChanged: {
                    // The page filter is pure UI: we do not change the global agent language.
                    var o = appWindow.voiceLangOptions
                    if (currentIndex < 0 || currentIndex >= o.length)
                        return
                    vmPage.filterLang = o[currentIndex].id
                }
            }

            Label {
                id: summaryLabel
                anchors.top: filterCombo.bottom
                anchors.topMargin: Theme.paddingSmall
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                text: qsTr("On device: %1 of %2 models, %3")
                      .arg(vmPage.downloadedCount())
                      .arg((appWindow.voiceModels || []).length)
                      .arg(appWindow.humanSize(vmPage.downloadedBytes()))
                color: Theme.secondaryColor
                font.pixelSize: Theme.fontSizeExtraSmall
                wrapMode: Text.Wrap
            }
        }

        delegate: ListItem {
            id: item
            width: parent.width
            contentHeight: isHeader
                           ? Theme.itemSizeMedium
                           : Math.max(Theme.itemSizeSmall,
textCol.height
                                       + (dlstate === appWindow.stateDownloading
                                          ? Theme.paddingSmall + 6 : 0)
                                       + Theme.paddingMedium)

            onClicked: vmPage.onRowTap(isHeader, kind, modelId,
                                       engine, dlstate, downloaded)

            Label {
                id: headerArrow
                visible: isHeader
                width: Theme.itemSizeSmall
                x: Theme.horizontalPageMargin
                anchors.verticalCenter: parent.verticalCenter
                text: vmPage.sectionExpanded(kind) ? "▾" : "▸"
                color: parent.highlighted ? Theme.highlightColor : Theme.secondaryColor
                font.pixelSize: Theme.fontSizeSmall
            }

            Label {
                id: headerTitle
                visible: isHeader
                x: Theme.horizontalPageMargin + Theme.itemSizeSmall + Theme.paddingSmall
                width: parent.width - x - Theme.horizontalPageMargin
                anchors.verticalCenter: parent.verticalCenter
                text: (total === done)
                      ? title + " (" + done + ")"
                      : title + " (" + done + "/" + total + ")"
                color: parent.highlighted ? Theme.highlightColor : Theme.primaryColor
                font.pixelSize: Theme.fontSizeSmall
                truncationMode: TruncationMode.Fade
            }

            Column {
                id: textCol
                visible: !isHeader
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                         - Theme.itemSizeMedium - Theme.paddingMedium
                anchors.verticalCenter: parent.verticalCenter
                spacing: 2

                Label {
                    width: parent.width
                    text: name
                    wrapMode: Text.Wrap
                    color: vmPage.isSelected({ model_id: modelId, downloaded: downloaded })
                           ? Theme.highlightColor
                           : (item.highlighted ? Theme.highlightColor : Theme.primaryColor)
                    font.pixelSize: Theme.fontSizeSmall
                }
                Label {
                    width: parent.width
                    text: sub
                    wrapMode: Text.Wrap
                    color: Theme.secondaryColor
                    font.pixelSize: Theme.fontSizeExtraSmall
                }
                Row {
                    visible: dlstate === appWindow.stateDownloading
                    spacing: Theme.paddingMedium
                    width: parent.width
                    BusyIndicator {
                        running: parent.visible
                        size: BusyIndicatorSize.Small
                        anchors.verticalCenter: parent.verticalCenter
                    }
                    Label {
                        anchors.verticalCenter: parent.verticalCenter
                        width: parent.width - Theme.paddingMedium - Theme.iconSizeSmall
                        text: vmPage.liveInfo(modelId)
                        color: Theme.primaryColor
                        font.pixelSize: Theme.fontSizeSmall
                        truncationMode: TruncationMode.Fade
                    }
                }
            }

            Label {
                id: iconLbl
                visible: !isHeader
                anchors.right: parent.right
                anchors.rightMargin: Theme.paddingSmall
                anchors.verticalCenter: parent.verticalCenter
                width: Theme.itemSizeMedium
                horizontalAlignment: Text.AlignHCenter
                text: vmPage.glyph({ state: dlstate,
                                    model_id: modelId,
                                    downloaded: downloaded })
                color: vmPage.glyphColor({ state: dlstate,
                                          model_id: modelId,
                                          downloaded: downloaded })
                font.pixelSize: Theme.fontSizeSmall
            }

            Rectangle {
                id: bar
                anchors.left: parent.left
                anchors.right: parent.right
                anchors.bottom: parent.bottom
                height: 3
                visible: !isHeader && dlstate === appWindow.stateDownloading
                color: Theme.highlightBackgroundColor
                Rectangle {
                    x: 0
                    y: 0
                    width: parent.width * vmPage.liveRatio(modelId)
                    height: parent.height
                    color: Theme.highlightColor
                }
            }

            menu: ContextMenu {
                MenuItem {
                    text: isHeader
                          ? qsTr("Expand / collapse")
                          : (dlstate === appWindow.stateDownloading
                             ? qsTr("Cancel download")
                             : (downloaded ? qsTr("Remove model") : qsTr("Download")))
                    onClicked: vmPage.onRowMenu(isHeader, kind, modelId,
                                                dlstate, downloaded, name, item)
                }
            }
        }

        ViewPlaceholder {
            enabled: list.count === 0
            text: qsTr("No models for the selected language")
        }

        PullDownMenu {
            MenuItem {
                text: qsTr("Update model catalogs")
                onClicked: appWindow.voiceCmd(appWindow.cmdCatalogUpdate, "")
            }
        }

        VerticalScrollDecorator {}
    }
}