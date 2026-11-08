import QtQuick 2.6
import Sailfish.Silica 1.0

// Model picker, grouped by provider.
//
// Providers that can be used right away (free OpenCode models, providers with
// a working key) come first. Providers that need an API key are listed after
// them with their models greyed out and an "unlock" row that opens the key
// field in Settings. Big catalogs start compact: only a few providers are
// listed (the current one always), each collapsed to a header with its model
// count unless it holds the current model.
Page {
    id: pickerPage
    property string sessionId
    property var appWindow
    property string currentProvider
    property string currentModel
    property string currentVariant

    // Row kinds of the flattened list.
    readonly property string kRowProvider: "provider"
    readonly property string kRowModel: "model"
    readonly property string kRowUnlock: "unlock"
    readonly property string kRowMoreProviders: "more"
    // `lockReason` values from the worker (main.rs append_locked_models).
    readonly property string kLockRejected: "rejected"
    // Up to this many models in total, every unlocked provider starts expanded.
    readonly property int smallCatalog: 12
    // Providers listed before "Show N more providers".
    readonly property int providersShown: 3

    // Per-provider expanded state chosen by the user (`{pid: bool}`).
    property var expanded: ({})
    property bool showAllProviders: false
    property var rows: []
    property int providerCount: 0
    property int availableCount: 0
    property int lockedCount: 0

    function groups() {
        var list = pickerPage.appWindow.models || []
        var by = {}
        var order = []
        for (var i = 0; i < list.length; i++) {
            var m = list[i]
            if (!(m.providerID in by)) {
                by[m.providerID] = { pid: m.providerID, models: [], locked: true, reason: "" }
                order.push(m.providerID)
            }
            var g = by[m.providerID]
            if (!m.locked)
                g.locked = false
            else if (m.lockReason)
                g.reason = m.lockReason
            // An empty modelID only keeps a locked provider visible.
            if (m.modelID !== "")
                g.models.push(m)
        }
        var aw = pickerPage.appWindow
        var out = []
        for (var j = 0; j < order.length; j++) {
            var grp = by[order[j]]
            grp.models.sort(function (a, b) { return a.modelName.localeCompare(b.modelName) })
            out.push(grp)
        }
        // Usable providers first, then alphabetical.
        out.sort(function (a, b) {
            if (a.locked !== b.locked)
                return a.locked ? 1 : -1
            return aw.providerTitle(a.pid).localeCompare(aw.providerTitle(b.pid))
        })
        return out
    }

    function isOpen(grp, total) {
        if (grp.pid in pickerPage.expanded)
            return pickerPage.expanded[grp.pid]
        if (grp.locked)
            return false
        return grp.pid === pickerPage.currentProvider || total <= pickerPage.smallCatalog
    }

    function variantSummary(vs) {
        if (!vs || vs.length === 0)
            return ""
        var labels = []
        for (var i = 0; i < vs.length; i++)
            labels.push(pickerPage.appWindow.variantLabel(vs[i]))
        return qsTr("Reasoning: %1").arg(labels.join(", "))
    }

    function countText(n) {
        return n === 1 ? qsTr("1 model") : qsTr("%1 models").arg(n)
    }

    function unlockText(grp) {
        var name = pickerPage.appWindow.providerTitle(grp.pid)
        if (grp.reason === pickerPage.kLockRejected)
            return qsTr("The %1 API key was rejected. Enter a valid key in Settings to unlock these models.").arg(name)
        if (grp.models.length === 0)
            return qsTr("Add your %1 API key in Settings to unlock its models.").arg(name)
        return qsTr("Add your %1 API key in Settings to unlock %2.").arg(name).arg(countText(grp.models.length))
    }

    function rebuild() {
        var gs = groups()
        var total = 0
        var available = 0
        var locked = 0
        for (var i = 0; i < gs.length; i++) {
            if (gs[i].locked)
                locked += gs[i].models.length
            else
                available += gs[i].models.length
        }
        total = available
        var shown = gs
        if (!pickerPage.showAllProviders && gs.length > pickerPage.providersShown) {
            // The current provider always stays reachable.
            var keep = {}
            var kept = 0
            for (var c = 0; c < gs.length; c++) {
                if (gs[c].pid === pickerPage.currentProvider) {
                    keep[gs[c].pid] = true
                    kept++
                }
            }
            for (var k = 0; k < gs.length && kept < pickerPage.providersShown; k++) {
                if (!keep[gs[k].pid]) {
                    keep[gs[k].pid] = true
                    kept++
                }
            }
            shown = []
            for (var g = 0; g < gs.length; g++) {
                if (keep[gs[g].pid])
                    shown.push(gs[g])
            }
        }
        var r = []
        for (var p = 0; p < shown.length; p++) {
            var grp = shown[p]
            var open = isOpen(grp, total)
            r.push({ kind: pickerPage.kRowProvider, pid: grp.pid, count: grp.models.length,
                     open: open, locked: grp.locked, text: "" })
            // Locked: the way to unlock is shown even while collapsed.
            if (grp.locked)
                r.push({ kind: pickerPage.kRowUnlock, pid: grp.pid, text: unlockText(grp),
                         locked: true })
            if (!open)
                continue
            for (var j = 0; j < grp.models.length; j++) {
                var m = grp.models[j]
                r.push({ kind: pickerPage.kRowModel, pid: grp.pid, mid: m.modelID,
                         name: m.modelName, variants: m.variants || [], locked: !!m.locked })
            }
        }
        if (shown.length < gs.length)
            r.push({ kind: pickerPage.kRowMoreProviders, count: gs.length - shown.length })
        pickerPage.providerCount = gs.length
        pickerPage.availableCount = available
        pickerPage.lockedCount = locked
        pickerPage.rows = r
    }

    // Rebuilding replaces the list model, which scrolls a ListView back to
    // the top; keep the position so the tapped row stays put.
    function rebuildKeepingPosition() {
        var y = modelList.contentY
        rebuild()
        modelList.forceLayout()
        modelList.contentY = Math.min(y, Math.max(modelList.originY,
            modelList.originY + modelList.contentHeight - modelList.height))
    }

    function toggle(pid, open) {
        var e = pickerPage.expanded
        e[pid] = !open
        pickerPage.expanded = e
        rebuildKeepingPosition()
    }

    function pick(row) {
        if (row.locked) {
            pickerPage.appWindow.openProviderKeySettings(row.pid)
            return
        }
        // Keep the reasoning level when the new model offers it too.
        var keep = row.variants.indexOf(pickerPage.currentVariant) >= 0
                 ? pickerPage.currentVariant : ""
        pickerPage.appWindow.setModel(pickerPage.sessionId, row.pid, row.mid, keep)
        pageStack.pop()
    }

    // Hold a model: pick it together with a reasoning level in one step.
    function pickWithVariant(row, v) {
        pickerPage.appWindow.setModel(pickerPage.sessionId, row.pid, row.mid,
                                      v === pickerPage.appWindow.kVariantDefault ? "" : v)
        pageStack.pop()
    }

    Component.onCompleted: rebuild()
    Connections {
        target: pickerPage.appWindow
        onModelsChanged: pickerPage.rebuildKeepingPosition()
    }

    SilicaListView {
        id: modelList
        anchors.fill: parent
        model: pickerPage.rows

        header: Column {
            width: modelList.width
            PageHeader {
            title: qsTr("Model")
            description: {
                if (pickerPage.providerCount === 0)
                    return ""
                var s = qsTr("%1 providers · %2 available").arg(pickerPage.providerCount)
                                                        .arg(pickerPage.availableCount)
                if (pickerPage.lockedCount > 0)
                    s += " · " + qsTr("%1 locked").arg(pickerPage.lockedCount)
                return s
            }
            }
            Label {
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                visible: pickerPage.providerCount > 0
                wrapMode: Text.Wrap
                horizontalAlignment: Text.AlignRight
                text: qsTr("Hold a model to choose its reasoning level")
                color: Theme.secondaryColor
                font.pixelSize: pickerPage.appWindow.fs(Theme.fontSizeExtraSmall)
            }
            Item { width: 1; height: Theme.paddingMedium }
        }

        delegate: ListItem {
            id: rowItem
            property var row: modelData
            readonly property bool isProvider: row.kind === pickerPage.kRowProvider
            readonly property bool isModel: row.kind === pickerPage.kRowModel
            readonly property bool isUnlock: row.kind === pickerPage.kRowUnlock
            readonly property bool isMore: row.kind === pickerPage.kRowMoreProviders
            readonly property bool isLocked: !!row.locked
            readonly property bool isCurrent: isModel
                                              && row.pid === pickerPage.currentProvider
                                              && row.mid === pickerPage.currentModel
            // Long press on a model with reasoning levels: choose one.
            menu: (isModel && !isLocked && row.variants.length > 0) ? variantMenu : null
            Component {
                id: variantMenu
                ContextMenu {
                    MenuLabel {
                        text: qsTr("Reasoning for %1").arg(rowItem.row.name)
                    }
                    Repeater {
                        model: [pickerPage.appWindow.kVariantDefault].concat(rowItem.row.variants)
                        MenuItem {
                            text: pickerPage.appWindow.variantLabel(modelData)
                                  + ((rowItem.isCurrent && modelData === pickerPage.currentVariant)
                                     ? "  ✓" : "")
                            // After the menu has closed, so the page pop does
                            // not start while the menu animation still runs.
                            onDelayedClick: pickerPage.pickWithVariant(rowItem.row, modelData)
                        }
                    }
                }
            }
            contentHeight: isModel ? Theme.itemSizeSmall
                                   : (isUnlock ? unlockLbl.height + 2 * Theme.paddingMedium
                                               : Theme.itemSizeExtraSmall)

            // ── provider header: name, model count, lock, chevron ──
            Label {
                id: providerLbl
                visible: rowItem.isProvider
                x: Theme.horizontalPageMargin
                anchors.verticalCenter: parent.verticalCenter
                text: rowItem.isProvider ? pickerPage.appWindow.providerTitle(rowItem.row.pid) : ""
                color: rowItem.isLocked ? Theme.secondaryHighlightColor : Theme.highlightColor
                font.pixelSize: pickerPage.appWindow.fs(Theme.fontSizeMedium)
            }
            Label {
                visible: rowItem.isProvider
                anchors.left: providerLbl.right
                anchors.leftMargin: Theme.paddingMedium
                anchors.baseline: providerLbl.baseline
                text: !rowItem.isProvider ? ""
                      : pickerPage.countText(rowItem.row.count)
                        + (rowItem.isLocked ? " · " + qsTr("locked") : "")
                color: Theme.secondaryHighlightColor
                font.pixelSize: pickerPage.appWindow.fs(Theme.fontSizeExtraSmall)
            }
            Image {
                visible: rowItem.isProvider && rowItem.row.count > 0
                anchors.right: parent.right
                anchors.rightMargin: Theme.horizontalPageMargin
                anchors.verticalCenter: parent.verticalCenter
                source: "image://theme/icon-m-down?" + Theme.highlightColor
                rotation: (rowItem.isProvider && rowItem.row.open) ? 180 : 0
            }

            // ── unlock hint of a locked provider ──
            Image {
                id: unlockIcon
                visible: rowItem.isUnlock
                x: Theme.horizontalPageMargin
                anchors.verticalCenter: parent.verticalCenter
                source: "image://theme/icon-s-secure?" + Theme.highlightColor
            }
            Label {
                id: unlockLbl
                visible: rowItem.isUnlock
                anchors.left: unlockIcon.right
                anchors.leftMargin: Theme.paddingMedium
                anchors.right: parent.right
                anchors.rightMargin: Theme.horizontalPageMargin
                anchors.verticalCenter: parent.verticalCenter
                text: rowItem.isUnlock ? rowItem.row.text : ""
                wrapMode: Text.Wrap
                color: rowItem.highlighted ? Theme.highlightColor : Theme.secondaryHighlightColor
                font.pixelSize: pickerPage.appWindow.fs(Theme.fontSizeExtraSmall)
            }

            // ── model row: check mark / lock, name, reasoning levels ──
            Label {
                visible: rowItem.isModel && !rowItem.isLocked
                x: Theme.horizontalPageMargin
                anchors.verticalCenter: parent.verticalCenter
                text: rowItem.isCurrent ? "✓" : ""
                color: Theme.highlightColor
            }
            Image {
                visible: rowItem.isModel && rowItem.isLocked
                x: Theme.horizontalPageMargin
                anchors.verticalCenter: parent.verticalCenter
                source: "image://theme/icon-s-secure?" + Theme.secondaryColor
                opacity: 0.6
            }
            Column {
                visible: rowItem.isModel
                x: Theme.itemSizeSmall
                width: parent.width - x - Theme.horizontalPageMargin
                anchors.verticalCenter: parent.verticalCenter
                // Greyed out until the provider's key is saved.
                opacity: rowItem.isLocked ? 0.4 : 1.0
                Label {
                    width: parent.width
                    truncationMode: TruncationMode.Fade
                    text: rowItem.isModel ? rowItem.row.name : ""
                    font.pixelSize: pickerPage.appWindow.fs(Theme.fontSizeMedium)
                    color: rowItem.isLocked ? Theme.secondaryColor
                           : ((rowItem.highlighted || rowItem.isCurrent)
                              ? Theme.highlightColor : Theme.primaryColor)
                }
                Label {
                    width: parent.width
                    visible: text !== ""
                    truncationMode: TruncationMode.Fade
                    text: !rowItem.isModel ? ""
                          : (rowItem.isLocked ? qsTr("Needs an API key")
                                              : pickerPage.variantSummary(rowItem.row.variants))
                    color: Theme.secondaryColor
                    font.pixelSize: pickerPage.appWindow.fs(Theme.fontSizeExtraSmall)
                }
            }

            // ── "show more providers" button ──
            Label {
                visible: rowItem.isMore
                anchors.centerIn: parent
                text: !rowItem.isMore ? ""
                      : (rowItem.row.count === 1 ? qsTr("Show 1 more provider")
                                                 : qsTr("Show %1 more providers").arg(rowItem.row.count))
                color: rowItem.highlighted ? Theme.highlightColor : Theme.primaryColor
            }

            onClicked: {
                if (rowItem.isProvider) {
                    if (rowItem.row.count > 0)
                        pickerPage.toggle(rowItem.row.pid, rowItem.row.open)
                    else if (rowItem.isLocked)
                        pickerPage.appWindow.openProviderKeySettings(rowItem.row.pid)
                } else if (rowItem.isUnlock) {
                    pickerPage.appWindow.openProviderKeySettings(rowItem.row.pid)
                } else if (rowItem.isModel) {
                    pickerPage.pick(rowItem.row)
                } else {
                    pickerPage.showAllProviders = true
                    pickerPage.rebuildKeepingPosition()
                }
            }
        }

        ViewPlaceholder {
            enabled: pickerPage.providerCount === 0
            text: qsTr("No available models")
        }

        VerticalScrollDecorator {}
    }
}
