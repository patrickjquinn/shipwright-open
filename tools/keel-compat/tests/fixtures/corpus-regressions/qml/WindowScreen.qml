// QtQuick.Window imported before Sailfish.Silica: an unqualified Screen is
// Qt Quick's, so its Silica-only members are blockers.
import QtQuick.Window 2.2
import QtQuick 2.0
import Sailfish.Silica 1.0

Item {
    property bool large: Screen.sizeCategory >= Screen.Large
}
