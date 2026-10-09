// SPDX-FileCopyrightText: 2026 Patrick Quinn
// SPDX-License-Identifier: GPL-3.0-or-later

// Change the master password. The result is reported on the settings page
// below, which listens for ShoalKeys.finished("changePassword").

import QtQuick
import Sailfish.Silica 1.0
import Shipwright.Keys 1.0

Dialog {
    id: dialog
    objectName: "changePasswordDialog"

    canAccept: !ShoalKeys.busy && currentPassword.text.length > 0 && newPassword.text.length > 0
               && newPassword.text === newConfirm.text

    onAccepted: ShoalKeys.changePassword(currentPassword.text, newPassword.text)

    SilicaFlickable {
        anchors.fill: parent
        contentHeight: column.height + Theme.paddingLarge

        Column {
            id: column
            width: parent.width

            DialogHeader {
                acceptText: qsTr("Change")
                title: qsTr("Change master password")
            }

            Label {
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                bottomPadding: Theme.paddingLarge
                wrapMode: Text.Wrap
                font.pixelSize: Theme.fontSizeSmall
                color: Theme.secondaryHighlightColor
                text: qsTr("Nobody can reset your master password, so choose one you'll remember. Recovery key files you saved earlier still work.")
            }

            PasswordField {
                id: currentPassword
                width: parent.width
                label: qsTr("Current password")
                placeholderText: qsTr("Current password")
                EnterKey.iconSource: "image://theme/icon-m-enter-next"
                EnterKey.onClicked: newPassword.focus = true
            }
            PasswordField {
                id: newPassword
                width: parent.width
                label: qsTr("New password")
                placeholderText: qsTr("New password")
                description: text.length ? JSON.parse(ShoalKeys.strength(text)).label : ""
                EnterKey.iconSource: "image://theme/icon-m-enter-next"
                EnterKey.onClicked: newConfirm.focus = true
            }
            PasswordField {
                id: newConfirm
                width: parent.width
                label: qsTr("Repeat new password")
                placeholderText: qsTr("Repeat new password")
                errorHighlight: text.length > 0 && text !== newPassword.text
                description: errorHighlight ? qsTr("The passwords do not match") : ""
                EnterKey.iconSource: "image://theme/icon-m-enter-accept"
                EnterKey.enabled: dialog.canAccept
                EnterKey.onClicked: dialog.accept()
            }
        }
        VerticalScrollDecorator { }
    }
}
