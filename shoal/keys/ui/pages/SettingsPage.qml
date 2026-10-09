// SPDX-FileCopyrightText: 2026 Patrick Quinn
// SPDX-License-Identifier: GPL-3.0-or-later

// Auto-lock, clipboard, master password and the optional licence.

import QtQuick
import Sailfish.Silica 1.0
import Shipwright.Keys 1.0

Page {
    id: page
    objectName: "settingsPage"

    readonly property var clipboardValues: [0, 10, 30, 60, 120]
    readonly property var idleValues: [0, 60, 300, 900, 3600]
    readonly property var backgroundValues: [0, 30, 300, -1]
    property string message: ""
    // The controls get their values from Keys while the page is built;
    // only changes the user makes afterwards are applied.
    property bool ready: false
    Component.onCompleted: ready = true

    function indexOf(values, v) {
        var i = values.indexOf(v)
        return i < 0 ? 0 : i
    }

    function apply() {
        if (!ready)
            return
        var err = ShoalKeys.setSettings(clipboardValues[clipboard.currentIndex], idleValues[idle.currentIndex],
                                   backgroundValues[background.currentIndex], withDevice.checked)
        if (err.length) {
            messageIsError = true
            message = err
        }
    }

    Connections {
        target: ShoalKeys
        function onFinished(op, ok, message) {
            if (op !== "changePassword")
                return
            page.messageIsError = !ok
            page.message = ok ? qsTr("Master password changed.") : message
        }
    }

    property bool messageIsError: false

    SilicaFlickable {
        id: flick
        anchors.fill: parent
        contentHeight: column.height + Theme.paddingLarge

        Column {
            id: column
            width: parent.width

            PageHeader { title: qsTr("Settings") }

            Label {
                objectName: "settingsMessage"
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                bottomPadding: Theme.paddingMedium
                wrapMode: Text.Wrap
                visible: text.length > 0
                color: page.messageIsError ? Theme.errorColor : Theme.highlightColor
                textFormat: Text.PlainText
                text: page.message
            }

            SectionHeader { text: qsTr("Locking") }
            ComboBox {
                id: idle
                objectName: "idleCombo"
                label: qsTr("Lock when idle for")
                currentIndex: page.indexOf(page.idleValues, ShoalKeys.idleLockSecs)
                menu: ContextMenu {
                    MenuItem { text: qsTr("Never") }
                    MenuItem { text: qsTr("1 minute") }
                    MenuItem { text: qsTr("5 minutes") }
                    MenuItem { text: qsTr("15 minutes") }
                    MenuItem { text: qsTr("1 hour") }
                }
                onCurrentIndexChanged: page.apply()
            }
            ComboBox {
                id: background
                label: qsTr("Lock in the background after")
                currentIndex: page.indexOf(page.backgroundValues, ShoalKeys.backgroundLockSecs)
                menu: ContextMenu {
                    MenuItem { text: qsTr("Immediately") }
                    MenuItem { text: qsTr("30 seconds") }
                    MenuItem { text: qsTr("5 minutes") }
                    MenuItem { text: qsTr("Never") }
                }
                onCurrentIndexChanged: page.apply()
            }
            TextSwitch {
                id: withDevice
                text: qsTr("Lock with the device")
                description: qsTr("Locks the vault when the phone's screen locks.")
                checked: ShoalKeys.lockOnScreenLock
                onCheckedChanged: page.apply()
            }

            SectionHeader { text: qsTr("Clipboard") }
            ComboBox {
                id: clipboard
                label: qsTr("Clear copied values after")
                currentIndex: page.indexOf(page.clipboardValues, ShoalKeys.clipboardClearSecs)
                menu: ContextMenu {
                    MenuItem { text: qsTr("Never") }
                    MenuItem { text: qsTr("10 seconds") }
                    MenuItem { text: qsTr("30 seconds") }
                    MenuItem { text: qsTr("1 minute") }
                    MenuItem { text: qsTr("2 minutes") }
                }
                onCurrentIndexChanged: page.apply()
            }

            SectionHeader { text: qsTr("Master password") }
            ValueButton {
                objectName: "changePasswordButton"
                label: qsTr("Change master password")
                description: qsTr("The vault is encrypted again with the new password.")
                enabled: !ShoalKeys.busy
                onClicked: pageStack.push(Qt.resolvedUrl("ChangePasswordDialog.qml"))
            }

            SectionHeader { text: qsTr("Licence") }
            DetailItem {
                objectName: "licenceState"
                label: qsTr("Status")
                value: ShoalKeys.licenceText
            }
            ValueButton {
                objectName: "addLicenceButton"
                visible: !ShoalKeys.licensed
                label: qsTr("Add licence")
                description: qsTr("Shoal Keys is free. A licence adds a password health report, which finds weak, reused and old passwords and logins without a one-time code.")
                onClicked: {
                    var d = pageStack.push(Qt.resolvedUrl("LicenceDialog.qml"))
                    d.accepted.connect(function() { page.installLicence(d.token) })
                }
            }
            ValueButton {
                objectName: "healthButton"
                visible: ShoalKeys.licensed
                label: qsTr("Password health")
                description: qsTr("Weak, reused and old passwords, and logins without a one-time password")
                onClicked: pageStack.push(Qt.resolvedUrl("HealthPage.qml"))
            }
            ValueButton {
                visible: ShoalKeys.licensed
                label: qsTr("Remove licence")
                onClicked: remorse.execute(qsTr("Removing licence"), function() { ShoalKeys.removeLicence() })
            }

            SectionHeader { text: qsTr("About") }
            DetailItem {
                label: qsTr("Device key stored in")
                value: ShoalKeys.keystoreName
            }
            DetailItem {
                label: qsTr("Vault format")
                value: "KeePass KDBX 4.1"
            }
        }
        RemorsePopup { id: remorse }
        VerticalScrollDecorator { }
    }

    // A licence token from LicenceDialog.
    function installLicence(token) {
        var err = ShoalKeys.installLicence(token)
        messageIsError = err.length > 0
        message = err.length ? err : qsTr("Licence added.")
        flick.scrollToTop()
    }
}
