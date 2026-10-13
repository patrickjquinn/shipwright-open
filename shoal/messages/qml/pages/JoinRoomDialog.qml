// Modified by Shipwright, 2026: Sailfish UI rules pass; see CHANGES-FROM-UPSTREAM.md.
import QtQuick 2.0
import Sailfish.Silica 1.0
import "MatrixLinks.js" as MatrixLinks

// Join a room by address. The server part is what lets the homeserver find a
// room it has never seen.
Dialog {
    id: dialog

    allowedOrientations: Orientation.All

    /// Filled in when a tapped link brought the user here.
    property string prefill: ""

    /// The room the field names: an address as typed, or the room of a pasted
    /// matrix.to or matrix: link. The app does not take web links from other
    /// apps (it would have to claim every https address to get matrix.to ones),
    /// so a matrix.to link from elsewhere arrives here, pasted.
    readonly property string address: {
        var text = aliasField.text.trim()
        var target = MatrixLinks.parse(text)
        if (target && target.kind === "room") {
            return target.id
        }
        return text
    }

    canAccept: address.length > 2 && address.charAt(0) !== "@"
               && address.indexOf("://") < 0

    onAccepted: matrix.joinRoomByAlias(address)

    // In landscape the dialog is barely taller than its header, so the field has
    // to be reachable by scrolling.
    SilicaFlickable {
        anchors.fill: parent
        contentHeight: content.height

        VerticalScrollDecorator {}

        Column {
            id: content

            width: parent.width
            spacing: Theme.paddingMedium

            DialogHeader {
                acceptText: qsTr("Join")
            }

            TextField {
                id: aliasField

                width: parent.width
                text: dialog.prefill
                label: qsTr("Room address")
                placeholderText: "#room:server, https://matrix.to/#/…"
                inputMethodHints: Qt.ImhNoAutoUppercase | Qt.ImhNoPredictiveText
                focus: true
                EnterKey.iconSource: "image://theme/icon-m-enter-accept"
                EnterKey.onClicked: dialog.accept()
            }

            Label {
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                wrapMode: Text.Wrap
                font.pixelSize: Theme.fontSizeExtraSmall
                color: Theme.secondaryHighlightColor
                text: qsTr("The room appears in the list once the server has answered. Joining a large public room can take a moment.")
            }
        }
    }
}
