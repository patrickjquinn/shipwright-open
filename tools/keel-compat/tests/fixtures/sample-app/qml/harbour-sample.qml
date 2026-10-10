import QtQuick 2.0
import Sailfish.Silica 1.0
import Nemo.Notifications 1.0

ApplicationWindow {
    initialPage: Component {
        Page {
            SilicaListView {
                anchors.fill: parent
                header: PageHeader { title: qsTr("Sample") }
                PullDownMenu {
                    MenuItem { text: qsTr("Refresh") }
                }
                delegate: ListItem {
                    Label { x: Theme.horizontalPageMargin; text: model.name }
                }
            }
        }
    }
    cover: CoverBackground {
        CoverActionList { CoverAction { iconSource: "image://theme/icon-cover-refresh" } }
    }
    Notification { id: notification }
}
