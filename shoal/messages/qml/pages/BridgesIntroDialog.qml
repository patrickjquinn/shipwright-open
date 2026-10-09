// SPDX-FileCopyrightText: 2026 Patrick Quinn
// SPDX-License-Identifier: Apache-2.0
//
// New file, added by Shipwright in 2026 (not part of upstream xmatic).
// The first-run entry to the bridges page, offered once after sign-in
// (qml/shipwright-shoal-messages.qml, maybeShowBridgesIntro). Accepting opens
// the bridges page, which records the entry as done; declining records it too.
import QtQuick 2.0
import Sailfish.Silica 1.0

Dialog {
    id: dialog

    allowedOrientations: Orientation.All

    acceptDestination: Qt.resolvedUrl("BridgesPage.qml")
    acceptDestinationAction: PageStackAction.Replace

    onAccepted: settings.bridgesIntroDone = true
    onRejected: settings.bridgesIntroDone = true

    SilicaFlickable {
        anchors.fill: parent
        contentHeight: content.height + Theme.paddingLarge

        VerticalScrollDecorator {}

        Column {
            id: content

            width: parent.width
            spacing: Theme.paddingMedium

            DialogHeader {
                acceptText: qsTr("Link now")
                cancelText: qsTr("Not now")
            }

            Label {
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                wrapMode: Text.Wrap
                font.pixelSize: Theme.fontSizeLarge
                color: Theme.highlightColor
                text: qsTr("Signal and Telegram")
            }

            Label {
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                wrapMode: Text.Wrap
                color: Theme.highlightColor
                text: qsTr("Link your Signal or Telegram account, and those chats appear in your chat list next to your Matrix rooms.")
            }

            Label {
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                wrapMode: Text.Wrap
                font.pixelSize: Theme.fontSizeExtraSmall
                color: Theme.secondaryHighlightColor
                text: qsTr("Linking goes through the bridge on your Shipwright cell. Messages are encrypted between this phone and the bridge; the bridge decrypts them to hand them on, so they pass through it readable on your cell.")
            }

            Label {
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                wrapMode: Text.Wrap
                font.pixelSize: Theme.fontSizeExtraSmall
                color: Theme.secondaryHighlightColor
                text: qsTr("This is asked once. You can link later under Account › Signal and Telegram.")
            }
        }
    }
}
