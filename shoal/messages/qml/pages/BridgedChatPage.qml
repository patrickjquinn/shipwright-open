// SPDX-FileCopyrightText: 2026 Patrick Quinn
// SPDX-License-Identifier: Apache-2.0
//
// New file, added by Shipwright in 2026 (not part of upstream xmatic).
// Starts a direct chat through a hosted bridge with the person behind a phone
// number (the bridges' `start-chat` command, core/src/bridges.rs). Once the
// bridge names the chat, the core joins it and the app window opens it
// (matrix.bridges chatReady, qml/shipwright-shoal-messages.qml).
import QtQuick 2.0
import Sailfish.Silica 1.0

Page {
    id: page

    allowedOrientations: Orientation.All

    property string contactName
    property string number

    // The bridge asked in this page's lifetime; the chat state shown is its.
    property string requested: ""

    readonly property var signalState: matrix.bridges.signalBridge
    readonly property var telegramState: matrix.bridges.telegramBridge
    readonly property var chat: {
        var state = requested === "signal" ? signalState
                  : (requested === "telegram" ? telegramState : null)
        return state && state.chat ? state.chat : { "state": "idle" }
    }
    readonly property bool working: requested.length > 0
                                    && (chat.state === "resolving" || chat.state === "ready")
    readonly property bool anyLinked: isLinked(signalState) || isLinked(telegramState)
    readonly property bool asking: (signalState.health === "unknown" && signalState.busy === true)
                                   || (telegramState.health === "unknown"
                                       && telegramState.busy === true)

    function isLinked(state) {
        return state.health === "connected" || state.health === "connecting"
    }

    function bridgeName(bridge) {
        return bridge === "signal" ? "Signal" : "Telegram"
    }

    function start(bridge) {
        page.requested = bridge
        errorLabel.text = ""
        matrix.bridges.startChat(bridge, page.number)
    }

    Component.onCompleted: {
        if (signalState.health === "unknown" || telegramState.health === "unknown") {
            matrix.bridges.refreshAll()
        }
    }

    Connections {
        target: matrix.bridges
        onFailed: {
            if (bridge === page.requested) {
                errorLabel.text = message
            }
        }
    }

    SilicaFlickable {
        anchors.fill: parent
        contentHeight: column.height + Theme.paddingLarge

        VerticalScrollDecorator {}

        Column {
            id: column

            width: page.width
            spacing: Theme.paddingMedium

            PageHeader {
                title: page.contactName
                description: page.number
            }

            Label {
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                wrapMode: Text.Wrap
                font.pixelSize: Theme.fontSizeExtraSmall
                color: Theme.secondaryHighlightColor
                text: qsTr("The bridge on your Shipwright cell looks the number up and opens a direct chat. Signal asks its own servers whether the number has an account. Telegram finds a number only if your linked account has met it before.")
            }

            Button {
                anchors.horizontalCenter: parent.horizontalCenter
                visible: page.isLinked(page.signalState)
                enabled: !page.working
                text: qsTr("Chat on Signal")
                onClicked: page.start("signal")
            }

            Button {
                anchors.horizontalCenter: parent.horizontalCenter
                visible: page.isLinked(page.telegramState)
                enabled: !page.working
                text: qsTr("Chat on Telegram")
                onClicked: page.start("telegram")
            }

            BusyIndicator {
                anchors.horizontalCenter: parent.horizontalCenter
                size: BusyIndicatorSize.Medium
                running: page.working || page.asking
                visible: running
            }

            Label {
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                wrapMode: Text.Wrap
                visible: text.length > 0
                color: page.chat.state === "resolving" || page.chat.state === "ready"
                       ? Theme.highlightColor : Theme.errorColor
                text: {
                    if (page.requested.length === 0) {
                        return ""
                    }
                    var name = page.bridgeName(page.requested)
                    switch (page.chat.state) {
                    case "resolving": return qsTr("Asking the %1 bridge…").arg(name)
                    case "ready": return qsTr("Opening the chat…")
                    case "notFound": return qsTr("%1 found no account with this number.").arg(name)
                    case "failed": return qsTr("The chat could not be started: %1").arg(page.chat.reason)
                    default: return ""
                    }
                }
            }

            Label {
                id: errorLabel

                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                wrapMode: Text.Wrap
                visible: text.length > 0
                color: Theme.errorColor
            }

            Label {
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                wrapMode: Text.Wrap
                visible: !page.anyLinked && !page.asking
                color: Theme.highlightColor
                text: qsTr("Link Signal or Telegram first.")
            }

            Button {
                anchors.horizontalCenter: parent.horizontalCenter
                visible: !page.anyLinked && !page.asking
                text: qsTr("Signal and Telegram")
                onClicked: pageStack.push(Qt.resolvedUrl("BridgesPage.qml"))
            }
        }
    }
}
