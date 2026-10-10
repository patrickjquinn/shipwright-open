import QtQuick 2.0
import Sailfish.Silica 1.0
import harbour.regress 1.0
import "Constants.js" as Constants
import "helpers.js" as Helpers

ApplicationWindow {
    allowedOrientations: Orientation.All
    // Silica-only Screen member through an unqualified Screen (corpus:
    // babbage, counter, foilnotes, yubikey and others): Silica's under
    // QtQuick 2.0 (keel/qt5compat's shims), so no blocker.
    property bool large: Screen.sizeCategory >= Screen.Large
    initialPage: Component {
        Page {
            onStatusChanged: if (status === PageStatus.Active) Helpers.ready(pageStack)
            SilicaFlickable {
                anchors.fill: parent
                PullDownMenu { MenuItem { text: "Refresh" } }
                Label {
                    width: Screen.width - Constants.margin
                    truncationMode: TruncationMode.Fade
                    font.weight: Font.Bold
                }
                RegressApi { id: api }
            }
        }
    }
    cover: CoverBackground { }
}
