#include <sailfishapp.h>
#include <MGConfItem>
#include <QtQuick>

#define APP_QML_IMPORT "harbour.regress"

class RegressApi : public QObject
{
};

int main(int argc, char *argv[])
{
    qmlRegisterType<RegressApi>(APP_QML_IMPORT, 1, 0, "RegressApi");
    return SailfishApp::main(argc, argv);
}
