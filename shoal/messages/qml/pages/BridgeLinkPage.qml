// SPDX-FileCopyrightText: 2026 Patrick Quinn
// SPDX-License-Identifier: Apache-2.0
//
// New file, added by Shipwright in 2026 (not part of upstream xmatic).
// Linking one bridge, step by step, as the bridge bot leads: a QR code to scan
// with the phone that has Signal or Telegram, or a phone number, a login code
// and possibly a two-factor password. The steps come from core/src/bridges.rs
// as matrix.bridges.signalBridge / telegramBridge; the QR code is drawn here
// from the module rows the core computed.
import QtQuick 2.0
import Sailfish.Silica 1.0

Page {
    id: page

    property string bridge: "signal"
    property string flow: "qr"

    readonly property var st: bridge === "signal" ? matrix.bridges.signalBridge
                                                  : matrix.bridges.telegramBridge
    readonly property string step: st.step || "idle"
    readonly property bool inProgress: step === "starting" || step === "qr" || step === "code"
                                       || step === "input" || step === "submitting"
    readonly property string name: bridge === "signal" ? qsTr("Signal") : qsTr("Telegram")

    allowedOrientations: Orientation.All

    function submit() {
        if (valueField.text.trim().length === 0) {
            return
        }
        matrix.bridges.submit(page.bridge, valueField.text)
        // Not kept on screen: it may be a password.
        valueField.text = ""
    }

    Component.onCompleted: matrix.bridges.link(page.bridge, page.flow)
    // Leaving half-way gives the login back, so the bot is not left waiting.
    Component.onDestruction: {
        if (page.inProgress) {
            matrix.bridges.cancel(page.bridge)
        }
    }

    SilicaFlickable {
        anchors.fill: parent
        contentHeight: column.height + Theme.paddingLarge

        VerticalScrollDecorator {}

        Column {
            id: column

            width: page.width
            spacing: Theme.paddingLarge

            PageHeader {
                title: qsTr("Link %1").arg(page.name)
            }

            // ---- Waiting ----------------------------------------------------
            BusyIndicator {
                anchors.horizontalCenter: parent.horizontalCenter
                size: BusyIndicatorSize.Large
                running: page.step === "starting" || page.step === "submitting"
                         || (page.step === "idle" && page.st.busy === true)
                visible: running
            }

            Label {
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                horizontalAlignment: Text.AlignHCenter
                wrapMode: Text.Wrap
                visible: page.step === "starting" || page.step === "submitting"
                color: Theme.secondaryHighlightColor
                text: qsTr("Waiting for the %1 bridge…").arg(page.name)
            }

            // ---- QR code ----------------------------------------------------
            Label {
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                wrapMode: Text.Wrap
                visible: page.step === "qr"
                text: page.bridge === "signal"
                      ? qsTr("On the phone where you use Signal, open Settings › Linked devices › Link new device, and scan this code.")
                      : qsTr("On the phone where you use Telegram, open Settings › Devices › Link Desktop Device, and scan this code.")
            }

            Rectangle {
                id: qrFrame

                anchors.horizontalCenter: parent.horizontalCenter
                width: Math.min(page.width, page.height) - 2 * Theme.horizontalPageMargin
                height: width
                color: "white"
                visible: page.step === "qr"

                Canvas {
                    id: qrCanvas

                    property var rows: page.st.qr || []

                    anchors.fill: parent
                    onRowsChanged: requestPaint()
                    onWidthChanged: requestPaint()
                    onPaint: {
                        var ctx = getContext("2d")
                        ctx.fillStyle = "white"
                        ctx.fillRect(0, 0, width, height)
                        var count = rows.length
                        if (count === 0) {
                            return
                        }
                        // Four modules of quiet zone on each side, whole pixels per module.
                        var cell = Math.floor(width / (count + 8))
                        var offset = Math.floor((width - cell * count) / 2)
                        ctx.fillStyle = "black"
                        for (var y = 0; y < count; y++) {
                            var row = rows[y]
                            for (var x = 0; x < row.length; x++) {
                                if (row.charAt(x) === "1") {
                                    ctx.fillRect(offset + x * cell, offset + y * cell, cell, cell)
                                }
                            }
                        }
                    }
                }
            }

            Label {
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                wrapMode: Text.Wrap
                visible: page.step === "qr"
                font.pixelSize: Theme.fontSizeExtraSmall
                color: Theme.secondaryHighlightColor
                text: qsTr("The code renews itself every minute or so. Keep this page open until the other phone confirms.")
            }

            // With Android App Support, the app may be on this very phone.
            Button {
                anchors.horizontalCenter: parent.horizontalCenter
                visible: page.step === "qr" && (page.st.qrLink || "").length > 0
                text: qsTr("Open %1 on this phone").arg(page.name)
                onClicked: Qt.openUrlExternally(page.st.qrLink)
            }

            // ---- Pairing code -------------------------------------------------
            Label {
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                wrapMode: Text.Wrap
                visible: page.step === "code"
                text: qsTr("Enter this code in %1 on your phone:").arg(page.name)
            }

            Label {
                anchors.horizontalCenter: parent.horizontalCenter
                visible: page.step === "code"
                font.pixelSize: Theme.fontSizeHuge
                font.family: "monospace"
                color: Theme.highlightColor
                text: page.st.code || ""
            }

            // ---- Typed answers (phone number, code, password) ------------------
            Label {
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                wrapMode: Text.Wrap
                visible: page.step === "input" && (page.st.fieldDescription || "").length > 0
                color: Theme.secondaryHighlightColor
                text: page.st.fieldDescription || ""
            }

            TextField {
                id: valueField

                width: parent.width
                visible: page.step === "input"
                label: page.st.fieldName || ""
                placeholderText: page.st.fieldName || ""
                echoMode: page.st.field === "password" ? TextInput.Password : TextInput.Normal
                inputMethodHints: page.st.field === "phone" ? Qt.ImhDialableCharactersOnly
                                  : page.st.field === "code" ? Qt.ImhDigitsOnly
                                  : (Qt.ImhNoAutoUppercase | Qt.ImhNoPredictiveText | Qt.ImhSensitiveData)
                EnterKey.enabled: text.trim().length > 0
                EnterKey.iconSource: "image://theme/icon-m-enter-next"
                EnterKey.onClicked: page.submit()
            }

            Button {
                anchors.horizontalCenter: parent.horizontalCenter
                visible: page.step === "input"
                enabled: valueField.text.trim().length > 0
                text: qsTr("Continue")
                onClicked: page.submit()
            }

            // ---- Messages from the bridge --------------------------------------
            Label {
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                wrapMode: Text.Wrap
                visible: text.length > 0 && page.step !== "done"
                font.pixelSize: Theme.fontSizeSmall
                color: page.st.retry || page.step === "failed" ? Theme.errorColor : Theme.secondaryHighlightColor
                // Never a bare "Try again": without a word from the bridge,
                // say what happened and what helps.
                text: {
                    var said = page.step === "failed" ? (page.st.reason || page.st.message || "")
                                                      : (page.st.error || page.st.message || "")
                    if (said.length === 0 && (page.step === "failed" || (page.step === "idle" && !page.st.busy)))
                        return qsTr("The %1 bridge did not answer. Check that you are online, then try again.").arg(page.name)
                    return said
                }
            }

            // ---- Finished ------------------------------------------------------
            Label {
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                wrapMode: Text.Wrap
                visible: page.step === "done"
                font.pixelSize: Theme.fontSizeLarge
                color: Theme.highlightColor
                text: qsTr("%1 is linked as %2.").arg(page.name).arg(page.st.remoteName || "")
            }

            Label {
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                wrapMode: Text.Wrap
                visible: page.step === "done"
                color: Theme.secondaryHighlightColor
                text: qsTr("Your %1 chats appear in the chat list as messages arrive. Earlier history is not copied.").arg(page.name)
            }

            Button {
                anchors.horizontalCenter: parent.horizontalCenter
                visible: page.step === "done"
                text: qsTr("Done")
                onClicked: pageStack.pop()
            }

            Button {
                anchors.horizontalCenter: parent.horizontalCenter
                visible: page.step === "failed" || (page.step === "idle" && !page.st.busy)
                text: qsTr("Try again")
                onClicked: matrix.bridges.link(page.bridge, page.flow)
            }

            Button {
                anchors.horizontalCenter: parent.horizontalCenter
                visible: page.inProgress
                text: qsTr("Cancel")
                // Leaving cancels (Component.onDestruction).
                onClicked: pageStack.pop()
            }
        }
    }
}
