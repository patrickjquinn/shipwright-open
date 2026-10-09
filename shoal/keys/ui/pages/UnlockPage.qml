// SPDX-FileCopyrightText: 2026 Patrick Quinn
// SPDX-License-Identifier: GPL-3.0-or-later

// First run: create the vault. Afterwards: unlock it with the master
// password (the device key comes from Sailfish Secrets).

import QtQuick
import Sailfish.Silica 1.0
import Shipwright.Keys 1.0
import "text.js" as Untrusted

Page {
    id: page
    objectName: "unlockPage"

    property string error: ""
    readonly property bool creating: !ShoalKeys.vaultExists

    function submit() {
        error = ""
        if (creating) {
            if (newPassword.text.length === 0) {
                error = qsTr("Choose a master password.")
                return
            }
            if (newPassword.text !== confirmPassword.text) {
                error = qsTr("The passwords do not match.")
                return
            }
            ShoalKeys.create(vaultName.text.length ? vaultName.text : qsTr("Passwords"), newPassword.text)
        } else {
            ShoalKeys.unlock(password.text)
        }
    }

    Connections {
        target: ShoalKeys
        function onFinished(op, ok, message) {
            if (op !== "create" && op !== "unlock" && op !== "recover")
                return
            if (ok) {
                password.text = ""
                newPassword.text = ""
                confirmPassword.text = ""
                pageStack.replace(Qt.resolvedUrl("MainPage.qml"))
            } else {
                page.error = message
            }
        }
    }

    SilicaFlickable {
        anchors.fill: parent
        contentHeight: column.height + Theme.paddingLarge

        PullDownMenu {
            visible: !page.creating
            MenuItem {
                text: qsTr("Restore with recovery key")
                onClicked: pageStack.push(Qt.resolvedUrl("RecoverDialog.qml"))
            }
        }

        Column {
            id: column
            width: parent.width

            PageHeader {
                title: page.creating ? qsTr("New vault") : (ShoalKeys.vaultName.length ? Untrusted.escape(ShoalKeys.vaultName) : qsTr("Keys"))
                description: page.creating ? qsTr("Shoal Keys") : qsTr("Locked")
            }

            Label {
                objectName: "lockReason"
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                wrapMode: Text.Wrap
                color: Theme.highlightColor
                visible: text.length > 0
                text: page.creating ? "" : ShoalKeys.lockReason
            }

            Label {
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                wrapMode: Text.Wrap
                color: Theme.secondaryHighlightColor
                font.pixelSize: Theme.fontSizeSmall
                visible: page.creating
                text: qsTr("Your passwords stay in a KeePass file on this phone, locked by your master password. Nobody can reset it, so choose one you will remember.")
            }

            Item {
                width: 1
                height: Theme.paddingLarge
            }

            // ---- unlock ----
            PasswordField {
                id: password
                objectName: "unlockPassword"
                width: parent.width
                visible: !page.creating
                label: qsTr("Master password")
                placeholderText: qsTr("Master password")
                enabled: !ShoalKeys.busy
                errorHighlight: page.error.length > 0
                description: page.error
                onTextChanged: page.error = ""
                EnterKey.iconSource: "image://theme/icon-m-enter-accept"
                EnterKey.enabled: text.length > 0
                EnterKey.onClicked: page.submit()
            }

            // ---- create ----
            TextField {
                id: vaultName
                width: parent.width
                visible: page.creating
                label: qsTr("Vault name")
                placeholderText: qsTr("Vault name")
                EnterKey.iconSource: "image://theme/icon-m-enter-next"
                EnterKey.onClicked: newPassword.focus = true
            }
            PasswordField {
                id: newPassword
                width: parent.width
                visible: page.creating
                label: qsTr("Master password")
                placeholderText: qsTr("Master password")
                description: newPassword.text.length ? JSON.parse(ShoalKeys.strength(newPassword.text)).label : ""
                EnterKey.iconSource: "image://theme/icon-m-enter-next"
                EnterKey.onClicked: confirmPassword.focus = true
            }
            PasswordField {
                id: confirmPassword
                width: parent.width
                visible: page.creating
                label: qsTr("Repeat master password")
                placeholderText: qsTr("Repeat master password")
                errorHighlight: page.error.length > 0
                                || (text.length > 0 && text !== newPassword.text.substring(0, text.length))
                description: page.error
                onTextChanged: page.error = ""
                EnterKey.iconSource: "image://theme/icon-m-enter-accept"
                EnterKey.onClicked: page.submit()
            }

            Item {
                width: 1
                height: Theme.paddingLarge
            }

            Button {
                objectName: "submitButton"
                anchors.horizontalCenter: parent.horizontalCenter
                preferredWidth: Theme.buttonWidthMedium
                text: page.creating ? qsTr("Create vault") : qsTr("Unlock")
                enabled: !ShoalKeys.busy && (page.creating ? newPassword.text.length > 0 : password.text.length > 0)
                onClicked: page.submit()
            }

            Item {
                width: 1
                height: Theme.paddingLarge
            }

            BusyIndicator {
                objectName: "busyIndicator"
                anchors.horizontalCenter: parent.horizontalCenter
                size: BusyIndicatorSize.Medium
                running: ShoalKeys.busy
                visible: running
            }
        }
        VerticalScrollDecorator { }
    }
}
