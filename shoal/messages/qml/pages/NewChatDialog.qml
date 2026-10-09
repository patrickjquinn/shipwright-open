// Modified by Shipwright, 2026: entry to the phone-contact picker for Signal and Telegram; Sailfish UI rules pass; see CHANGES-FROM-UPSTREAM.md.
import QtQuick 2.0
import Sailfish.Silica 1.0

// Start a conversation by Matrix ID. An existing private room with exactly
// that person is reused - two would split the conversation.
Dialog {
    id: dialog

    allowedOrientations: Orientation.All

    /// Filled in when a tapped link brought the user here.
    property string prefill: ""

    canAccept: userField.text.trim().length > 3 && !matrix.startingDirectChat

    onAccepted: matrix.startDirectChat(userField.text)

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
                acceptText: qsTr("Start chat")
            }

            TextField {
                id: userField

                width: parent.width
                text: dialog.prefill
                label: qsTr("User ID")
                placeholderText: "@name:server"
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
                text: qsTr("The other side receives an invitation and the chat appears in your list right away. Messages are encrypted from the start.")
            }

            // Shipwright: somebody on Signal or Telegram, by the phone number in
            // the address book. Needs contact matching, which reads the book.
            SectionHeader {
                text: qsTr("Signal and Telegram")
            }

            BackgroundItem {
                width: parent.width
                height: Theme.itemSizeSmall
                visible: settings.contactsMatching
                onClicked: pageStack.push(Qt.resolvedUrl("ContactPickerPage.qml"))

                Label {
                    x: Theme.horizontalPageMargin
                    width: parent.width - 2 * Theme.horizontalPageMargin
                    anchors.verticalCenter: parent.verticalCenter
                    truncationMode: TruncationMode.Fade
                    color: parent.highlighted ? Theme.highlightColor : Theme.primaryColor
                    text: qsTr("Choose a phone contact")
                }
            }

            Label {
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                wrapMode: Text.Wrap
                font.pixelSize: Theme.fontSizeExtraSmall
                color: Theme.secondaryHighlightColor
                text: settings.contactsMatching
                      ? qsTr("Starts a chat through the Signal or Telegram bridge with the person behind a number from your address book.")
                      : qsTr("To start a Signal or Telegram chat with somebody from your address book, switch on Account › Privacy › \"Match people with my contacts\".")
            }
        }
    }
}
