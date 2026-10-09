// SPDX-FileCopyrightText: 2026 Patrick Quinn
// SPDX-License-Identifier: Apache-2.0
//
// New file, added by Shipwright in 2026 (not part of upstream xmatic).
// The hosted Signal and Telegram bridges: status, link, unlink. The work is in
// core/src/bridges.rs (commands to the bridge bots, their replies parsed) and
// src/bridgeactions.cpp; this page shows matrix.bridges.
import QtQuick 2.0
import Sailfish.Silica 1.0

Page {
    id: page

    allowedOrientations: Orientation.All

    function bridgeName(bridge) {
        return bridge === "signal" ? qsTr("Signal") : qsTr("Telegram")
    }

    function healthText(state) {
        switch (state.health) {
        case "unlinked": return qsTr("Not linked")
        case "connected": return qsTr("Linked and connected")
        case "connecting": return qsTr("Linked, connecting…")
        case "relink": return qsTr("Signed out by %1. Unlink and link again.").arg(bridgeName(state.bridge))
        case "error": return qsTr("Linked, but the bridge reports a problem")
        default: return state.busy ? qsTr("Asking the bridge…") : qsTr("Status unknown")
        }
    }

    Component.onCompleted: {
        // Opening the page is what the first-run entry leads to; it is not
        // offered again afterwards.
        settings.bridgesIntroDone = true
        matrix.bridges.refreshAll()
    }

    SilicaFlickable {
        anchors.fill: parent
        contentHeight: column.height + Theme.paddingLarge

        PullDownMenu {
            MenuItem {
                text: qsTr("Refresh")
                onClicked: matrix.bridges.refreshAll()
            }
        }

        VerticalScrollDecorator {}

        Column {
            id: column

            width: page.width
            spacing: Theme.paddingMedium

            PageHeader {
                title: qsTr("Signal and Telegram")
            }

            Label {
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                wrapMode: Text.Wrap
                font.pixelSize: Theme.fontSizeExtraSmall
                color: Theme.secondaryHighlightColor
                text: qsTr("Your subscription runs bridges to Signal and Telegram on the Shoal server. Linked chats appear in your chat list. They are encrypted between this phone and the bridge; the bridge itself has to read them to pass them on, so they are not end-to-end encrypted the way chats between Matrix users are.")
            }

            Repeater {
                model: ["signal", "telegram"]

                delegate: Column {
                    id: section

                    property string bridge: modelData
                    property var st: bridge === "signal" ? matrix.bridges.signalBridge
                                                         : matrix.bridges.telegramBridge
                    property bool linked: st.logins !== undefined && st.logins.length > 0

                    width: column.width
                    spacing: Theme.paddingSmall

                    SectionHeader {
                        text: page.bridgeName(section.bridge)
                    }

                    Label {
                        x: Theme.horizontalPageMargin
                        width: parent.width - 2 * Theme.horizontalPageMargin
                        wrapMode: Text.Wrap
                        color: section.st.health === "relink" || section.st.health === "error"
                               ? Theme.errorColor : Theme.highlightColor
                        text: page.healthText(section.st)
                    }

                    Repeater {
                        model: section.st.logins || []

                        delegate: ListItem {
                            id: loginItem

                            width: section.width
                            contentHeight: Theme.itemSizeMedium
                            menu: Component {
                                ContextMenu {
                                    MenuItem {
                                        text: qsTr("Unlink")
                                        onClicked: loginItem.remorseAction(
                                            qsTr("Unlinking"),
                                            function() { matrix.bridges.unlink(section.bridge, modelData.id) })
                                    }
                                }
                            }

                            Label {
                                id: loginName
                                x: Theme.horizontalPageMargin
                                width: parent.width - 2 * Theme.horizontalPageMargin
                                anchors.top: parent.top
                                anchors.topMargin: Theme.paddingSmall
                                truncationMode: TruncationMode.Fade
                                text: modelData.name || modelData.id
                            }

                            Label {
                                x: Theme.horizontalPageMargin
                                width: parent.width - 2 * Theme.horizontalPageMargin
                                anchors.top: loginName.bottom
                                font.pixelSize: Theme.fontSizeExtraSmall
                                color: loginItem.highlighted ? Theme.secondaryHighlightColor : Theme.secondaryColor
                                text: qsTr("Press and hold to unlink")
                            }
                        }
                    }

                    BusyIndicator {
                        anchors.horizontalCenter: parent.horizontalCenter
                        size: BusyIndicatorSize.Small
                        running: section.st.busy === true
                        visible: running
                    }

                    Label {
                        x: Theme.horizontalPageMargin
                        width: parent.width - 2 * Theme.horizontalPageMargin
                        wrapMode: Text.Wrap
                        visible: (section.st.error || "").length > 0
                        font.pixelSize: Theme.fontSizeExtraSmall
                        color: Theme.errorColor
                        text: section.st.error || ""
                    }

                    Button {
                        anchors.horizontalCenter: parent.horizontalCenter
                        visible: section.bridge === "signal" && !section.linked
                        enabled: !section.st.busy && matrix.sessionState === "signed-in"
                        text: qsTr("Link Signal")
                        onClicked: pageStack.push(Qt.resolvedUrl("BridgeLinkPage.qml"),
                                                  { bridge: "signal", flow: "qr" })
                    }

                    Button {
                        anchors.horizontalCenter: parent.horizontalCenter
                        visible: section.bridge === "telegram" && !section.linked
                        enabled: !section.st.busy && matrix.sessionState === "signed-in"
                        text: qsTr("Link with phone number")
                        onClicked: pageStack.push(Qt.resolvedUrl("BridgeLinkPage.qml"),
                                                  { bridge: "telegram", flow: "phone" })
                    }

                    Button {
                        anchors.horizontalCenter: parent.horizontalCenter
                        visible: section.bridge === "telegram" && !section.linked
                        enabled: !section.st.busy && matrix.sessionState === "signed-in"
                        text: qsTr("Link with QR code")
                        onClicked: pageStack.push(Qt.resolvedUrl("BridgeLinkPage.qml"),
                                                  { bridge: "telegram", flow: "qr" })
                    }
                }
            }
        }
    }
}
