// SPDX-FileCopyrightText: 2026 Patrick Quinn
// SPDX-License-Identifier: GPL-3.0-or-later

// Licensed feature: entries with weak, reused or old passwords, or logins
// without a one-time password.

import QtQuick
import Sailfish.Silica 1.0
import Shipwright.Keys 1.0

Page {
    id: page
    objectName: "healthPage"

    property var report: {
        var _ = ShoalKeys.revision
        return JSON.parse(ShoalKeys.healthReport())
    }
    readonly property var items: Array.isArray(report) ? report : []

    SilicaListView {
        id: list
        anchors.fill: parent
        model: page.items
        PullDownMenu {
            visible: !ShoalKeys.licensed
            MenuItem {
                objectName: "addLicenceItem"
                text: qsTr("Add licence")
                onClicked: pageStack.push(Qt.resolvedUrl("SettingsPage.qml"))
            }
        }

        header: PageHeader {
            title: qsTr("Password health")
            // The list's model is page.items, so its count is their number.
            description: ShoalKeys.licensed && ListView.view && ListView.view.count > 0
                         ? qsTr("%n (entry needs|entries need) attention", "", ListView.view.count) : ""
        }
        delegate: ListItem {
            id: healthItem
            required property var modelData
            contentHeight: Theme.itemSizeMedium
            onClicked: pageStack.push(Qt.resolvedUrl("EntryPage.qml"), { entryId: modelData.id })
            Column {
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                anchors.verticalCenter: parent.verticalCenter
                Label {
                    width: parent.width
                    text: healthItem.modelData.title
                    textFormat: Text.PlainText
                    truncationMode: TruncationMode.Fade
                }
                Label {
                    width: parent.width
                    text: healthItem.modelData.issues.join(" · ")
                    truncationMode: TruncationMode.Fade
                    font.pixelSize: Theme.fontSizeExtraSmall
                    color: healthItem.highlighted ? Theme.secondaryHighlightColor : Theme.secondaryColor
                }
            }
        }
        ViewPlaceholder {
            objectName: "healthPlaceholder"
            enabled: list.count === 0
            text: !ShoalKeys.licensed ? qsTr("Password health needs a licence")
                : page.report.error ? page.report.error
                : qsTr("No problems found")
            hintText: !ShoalKeys.licensed
                      ? qsTr("Pull down to add a licence. It finds weak, reused and old passwords.")
                      : page.report.error ? "" : qsTr("No weak, reused or old passwords")
        }
        VerticalScrollDecorator { }
    }
}
