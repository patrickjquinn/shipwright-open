// SPDX-FileCopyrightText: 2026 Patrick Quinn
// SPDX-License-Identifier: GPL-3.0-or-later

// One entry. Secrets are hidden until revealed; tapping a value copies it,
// and the clipboard is cleared after the delay set in Settings.

import QtQuick
import Sailfish.Silica 1.0
import Shipwright.Keys 1.0
import Keel.Actions 1.0
import "text.js" as Untrusted

Page {
    id: page
    objectName: "entryPage"

    property string entryId
    property var entry: {
        var _ = ShoalKeys.revision
        return JSON.parse(ShoalKeys.entry(entryId))
    }
    property bool showPassword: false
    property var code: ({})

    function refreshCode() {
        code = entry.hasTotp ? JSON.parse(ShoalKeys.totp(entryId)) : ({})
    }

    // What Pilot may know when the person asks about this page: which entry
    // it is, never its secrets.
    KeelContext {
        purpose: "viewing a password entry"
        entity: page.entryId ? "keel://org.shipwright.shoal-keys/entry/" + page.entryId : ""
        text: page.entry.title || ""
        active: page.status === PageStatus.Active
    }

    Component.onCompleted: refreshCode()
    onEntryChanged: refreshCode()

    // TOTP countdown: a new code every period, shown with a progress bar.
    Timer {
        interval: 1000
        repeat: true
        running: page.entry.hasTotp === true && page.status === PageStatus.Active
        onTriggered: page.refreshCode()
    }

    SilicaFlickable {
        anchors.fill: parent
        contentHeight: column.height + Theme.paddingLarge

        PullDownMenu {
            MenuItem {
                text: qsTr("Delete")
                onClicked: remorse.execute(qsTr("Deleting"), function() {
                    ShoalKeys.deleteEntry(page.entryId)
                    pageStack.pop()
                })
            }
            MenuItem {
                objectName: "historyItem"
                text: qsTr("Password history")
                visible: (page.entry.historyCount || 0) > 0
                onClicked: pageStack.push(Qt.resolvedUrl("HistoryPage.qml"), { entryId: page.entryId })
            }
            MenuItem {
                text: qsTr("Edit")
                onClicked: pageStack.push(Qt.resolvedUrl("EditDialog.qml"), { entry: page.entry })
            }
        }

        RemorsePopup { id: remorse }

        Column {
            id: column
            width: parent.width

            PageHeader {
                objectName: "entryHeader"
                title: Untrusted.escape(page.entry.title)
                description: Untrusted.escape(page.entry.group)
            }

            CopyItem {
                label: qsTr("User name")
                value: page.entry.username || ""
                monospace: false
            }
            CopyItem {
                objectName: "passwordItem"
                label: qsTr("Password")
                value: page.entry.password || ""
                secret: !page.showPassword
                revealable: true
                onRevealToggled: page.showPassword = !page.showPassword
            }

            // ---- one-time password ----
            SectionHeader {
                text: qsTr("One-time password")
                visible: page.entry.hasTotp === true
            }
            CopyItem {
                id: totpItem
                objectName: "totpItem"
                visible: page.entry.hasTotp === true
                label: page.code.error ? qsTr("Invalid one-time password settings") : qsTr("Code")
                value: page.code.code ? page.code.code.replace(/(\d{3})(\d+)/, "$1 $2") : (page.code.error || "")
                copyValue: page.code.code || ""
                monospace: !page.code.error
                trailingWidth: Theme.iconSizeMedium + Theme.paddingLarge

                // Time left for this code, as Sailfish's authenticators show it.
                ProgressCircle {
                    objectName: "totpCountdown"
                    // (Dotted anchors: the Keel Actions build step parses this file.)
                    anchors.right: parent.right
                    anchors.rightMargin: Theme.horizontalPageMargin
                    anchors.verticalCenter: parent.verticalCenter
                    width: Theme.iconSizeMedium
                    height: width
                    visible: page.code.period !== undefined
                    progressColor: Theme.highlightColor
                    backgroundColor: Theme.highlightDimmerColor
                    value: (page.code.remaining || 0) / (page.code.period || 30)

                    Label {
                        anchors.centerIn: parent
                        text: page.code.remaining || 0
                        font.pixelSize: Theme.fontSizeExtraSmall
                        color: Theme.highlightColor
                    }
                }
            }

            // ---- web ----
            SectionHeader {
                text: qsTr("Websites")
                visible: (page.entry.urls || []).length > 0
            }
            Repeater {
                model: page.entry.urls || []
                CopyItem {
                    required property var modelData
                    label: qsTr("Website")
                    value: modelData
                }
            }

            // ---- custom fields ----
            SectionHeader {
                text: qsTr("Fields")
                visible: (page.entry.fields || []).length > 0
            }
            Repeater {
                model: page.entry.fields || []
                CopyItem {
                    required property var modelData
                    property bool shown: false
                    label: modelData.name
                    value: modelData.value
                    secret: modelData.protected && !shown
                    revealable: modelData.protected
                    monospace: modelData.protected
                    onRevealToggled: shown = !shown
                }
            }

            SectionHeader {
                text: qsTr("Notes")
                visible: (page.entry.notes || "").length > 0
            }
            Label {
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                visible: text.length > 0
                wrapMode: Text.Wrap
                textFormat: Text.PlainText
                text: page.entry.notes || ""
                color: Theme.highlightColor
            }

            SectionHeader {
                text: qsTr("Details")
                visible: (page.entry.tags || []).length > 0 || (page.entry.modified || 0) > 0
            }
            DetailItem {
                label: qsTr("Tags")
                value: (page.entry.tags || []).join(", ")
                visible: (page.entry.tags || []).length > 0
            }
            DetailItem {
                label: qsTr("Modified")
                value: page.entry.modified ? Qt.formatDateTime(new Date(page.entry.modified * 1000),
                                                                      "d MMM yyyy, " + Qt.locale().timeFormat(Locale.ShortFormat)) : ""
                visible: (page.entry.modified || 0) > 0
            }
        }
        VerticalScrollDecorator { }
    }
}
