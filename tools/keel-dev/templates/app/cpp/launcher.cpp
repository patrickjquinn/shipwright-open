// SPDX-FileCopyrightText: 2026 Patrick Quinn
// SPDX-License-Identifier: MIT-0

// Start-up through Keel's SailfishApp library, as a Sailfish SDK app does:
// SailfishApp::application(), createView() and pathToMainQml()
// (/usr/share/{{name}}/qml/{{name}}.qml; KEEL_SAILFISHAPP_DATADIR moves it).
// Keel's ApplicationWindow is an item, so it needs this view around it.
//
// On a phone (Keel's direct mode on Lipstick, the default on Sailfish OS, or
// under keel-shell) the view is shown full screen. Elsewhere it is a
// phone-sized window, and `keel run` drives it with:
//   KEEL_RUN_SIZE=<w>x<h>          screen size in portrait (default 540x960)
//   KEEL_RUN_ORIENTATION=<name>    ApplicationWindow.deviceOrientation:
//                                  portrait, landscape, portrait-inverted,
//                                  landscape-inverted
//   KEEL_RUN_SCREENSHOT=<file>     wait for the first page, check that the
//                                  cover loads, save the window to a PNG and
//                                  exit (status 0 on success)

#include "{{name}}/cpp/launcher.h"

#include <memory>
#include <string>
#include <vector>

#include <QtCore/QDebug>
#include <QtCore/QElapsedTimer>
#include <QtCore/QPointF>
#include <QtCore/QTimer>
#include <QtCore/QUrl>
#include <QtGui/QGuiApplication>
#include <QtGui/QImage>
#include <QtQml/QQmlComponent>
#include <QtQml/QQmlEngine>
#include <QtQuick/QQuickItem>
#include <QtQuick/QQuickView>

#include <keellauncher.h>
#include <sailfishapp.h>

namespace {

// The app's own QML module, registered by CXX-Qt (build.rs). Spelled out
// here so that keel-compat, which reads string literals in native code,
// counts the import as the app's own.
[[maybe_unused]] constexpr char kAppQmlModule[] = "{{qml_uri}}";

// QGuiApplication keeps argc and argv: they must outlive it.
int g_argc = 0;
std::vector<std::string> g_args;
std::vector<char*> g_argv;

QSize windowSize()
{
    const QStringList parts = qEnvironmentVariable("KEEL_RUN_SIZE").split(QLatin1Char('x'));
    if (parts.size() == 2) {
        const int w = parts[0].toInt();
        const int h = parts[1].toInt();
        if (w > 0 && h > 0)
            return QSize(w, h);
    }
    return QSize(540, 960);
}

// Silica's Orientation values (Qt::ScreenOrientation flags); 0 for unset.
int orientationFromEnv()
{
    const QString name = qEnvironmentVariable("KEEL_RUN_ORIENTATION");
    if (name == QLatin1String("portrait"))
        return 1;
    if (name == QLatin1String("landscape"))
        return 2;
    if (name == QLatin1String("portrait-inverted"))
        return 4;
    if (name == QLatin1String("landscape-inverted"))
        return 8;
    return 0;
}

// The page on top of the ApplicationWindow's stack once no transition runs.
QQuickItem* settledPage(QQuickView* view)
{
    QObject* root = view->rootObject();
    QObject* stack = root ? root->property("pageStack").value<QObject*>() : nullptr;
    if (!stack || stack->property("busy").toBool())
        return nullptr;
    return stack->property("currentPage").value<QQuickItem*>();
}

bool stackIsEmpty(QQuickView* view)
{
    QObject* stack = view->rootObject()->property("pageStack").value<QObject*>();
    return stack && !stack->property("busy").toBool() && stack->property("depth").toInt() == 0;
}

// The page stack does not say why a page failed; compile it again to tell.
void explainInitialPage(QQuickView* view)
{
    const QVariant initial = view->rootObject()->property("initialPage");
    const QUrl url = initial.toUrl();
    if (url.isEmpty()) {
        qWarning("keel-run: the ApplicationWindow has no initialPage URL");
        return;
    }
    QQmlComponent component(view->engine(), url);
    if (component.isError())
        qWarning().noquote() << "keel-run: initialPage did not compile:" << component.errorString();
}

// On a desktop the cover is not shown, so instantiate it once to catch
// errors in it.
bool coverLoads(QQuickView* view)
{
    const QVariant cover = view->rootObject()->property("cover");
    if (!cover.isValid() || cover.isNull()) {
        qInfo("keel-run: no cover");
        return true;
    }
    std::unique_ptr<QQmlComponent> owned;
    QQmlComponent* component = cover.value<QQmlComponent*>();
    if (!component) {
        if (cover.value<QObject*>()) {
            qInfo("keel-run: cover ok (an object)");
            return true;
        }
        const QUrl url = cover.toUrl();
        if (!url.isValid() || url.isEmpty()) {
            qWarning("keel-run: cover is neither a component nor a URL");
            return false;
        }
        owned = std::make_unique<QQmlComponent>(view->engine(), url);
        component = owned.get();
    }
    if (!component->isReady()) {
        qWarning().noquote() << "keel-run: cover did not compile:" << component->errorString();
        return false;
    }
    // In the context the cover component was written in (an inline
    // `cover: Component { ... }` sees the window's properties, as when
    // Keel's ApplicationWindow creates it), not the engine's root context.
    const std::unique_ptr<QObject> item(component->create(component->creationContext()));
    if (!item) {
        qWarning().noquote() << "keel-run: cover did not instantiate:" << component->errorString();
        return false;
    }
    qInfo("keel-run: cover ok");
    return true;
}

void scheduleScreenshot(QQuickView* view, const QString& path)
{
    auto* timer = new QTimer(view);
    auto clock = std::make_shared<QElapsedTimer>();
    clock->start();
    auto readyAt = std::make_shared<qint64>(-1);
    QObject::connect(timer, &QTimer::timeout, view, [view, path, timer, clock, readyAt]() {
        QQuickItem* page = settledPage(view);
        if (!page) {
            *readyAt = -1;
            if (clock->elapsed() > 30000 || (clock->elapsed() > 2000 && stackIsEmpty(view))) {
                timer->stop();
                qWarning("keel-run: no page on the stack");
                explainInitialPage(view);
                QCoreApplication::exit(3);
            }
            return;
        }
        // Let asynchronous images and the first frames settle.
        if (*readyAt < 0) {
            *readyAt = clock->elapsed();
            return;
        }
        if (clock->elapsed() - *readyAt < 500)
            return;
        timer->stop();
        qInfo().noquote() << "keel-run: first page:"
                          << (page->objectName().isEmpty() ? QStringLiteral("(no objectName)")
                                                           : page->objectName())
                          << page->metaObject()->className();
        const bool coverOk = coverLoads(view);
        const QImage image = view->grabWindow();
        if (image.isNull() || !image.save(path, "PNG")) {
            qWarning().noquote() << "keel-run: could not save" << path;
            QCoreApplication::exit(4);
            return;
        }
        qInfo().noquote() << "keel-run: screenshot" << path << image.width() << "x" << image.height();
        QCoreApplication::exit(coverOk ? 0 : 5);
    });
    timer->start(50);
}

// A phone held in `orientation`: the screen stays portrait and Keel rotates
// the page on it, so the root item is a portrait screen turned back by the
// same angle, in a window of the turned shape. The page then reads upright
// on the desktop, laid out as on the phone.
void showOnDesktop(QQuickView* view, const QSize& screen, int orientation)
{
    const int degrees = orientation == 2 ? 90 : orientation == 4 ? 180 : orientation == 8 ? 270 : 0;
    const QSize window = degrees % 180 ? screen.transposed() : screen;
    view->resize(window);
    if (degrees != 0) {
        // QQuickView sizes the root item to the window whenever the window
        // is resized; put the portrait screen back each time.
        QQuickItem* root = view->rootObject();
        const auto turn = [view, root, screen, degrees]() {
            if (root->size() != QSizeF(screen))
                root->setSize(screen);
            root->setPosition(QPointF((view->width() - screen.width()) / 2.0,
                                      (view->height() - screen.height()) / 2.0));
            root->setRotation(-degrees);
        };
        QObject::connect(root, &QQuickItem::widthChanged, view, turn, Qt::QueuedConnection);
        QObject::connect(root, &QQuickItem::heightChanged, view, turn, Qt::QueuedConnection);
        QObject::connect(view, &QWindow::widthChanged, view, turn, Qt::QueuedConnection);
        QObject::connect(view, &QWindow::heightChanged, view, turn, Qt::QueuedConnection);
        turn();
    }
    view->show();
}

} // namespace

std::int32_t keel_app_run(const rust::Vec<rust::String>& args)
{
    for (const rust::String& arg : args)
        g_args.emplace_back(arg.data(), arg.size());
    for (std::string& arg : g_args)
        g_argv.push_back(arg.data());
    g_argv.push_back(nullptr);
    g_argc = static_cast<int>(g_args.size());

    QGuiApplication* app = SailfishApp::application(g_argc, g_argv.data());
    const std::unique_ptr<QQuickView> view(SailfishApp::createView());
    QObject::connect(view->engine(), &QQmlEngine::quit, app, &QCoreApplication::quit);
    QObject::connect(view->engine(), &QQmlEngine::exit, app, &QCoreApplication::exit);

    const QUrl source = SailfishApp::pathToMainQml();
    view->setSource(source);
    if (view->status() == QQuickView::Error || !view->rootObject()) {
        qWarning().noquote() << "keel-run: could not load" << source.toString();
        return 1;
    }
    const int orientation = orientationFromEnv();
    if (orientation)
        view->rootObject()->setProperty("deviceOrientation", orientation);

    if (Keel::windowMode() != Keel::WindowMode::Desktop) {
        view->showFullScreen();
    } else {
        showOnDesktop(view.get(), windowSize(), orientation);
    }
    const QString screenshot = qEnvironmentVariable("KEEL_RUN_SCREENSHOT");
    if (!screenshot.isEmpty())
        scheduleScreenshot(view.get(), screenshot);
    return app->exec();
}

rust::Vec<rust::String> keel_launch_arguments()
{
    rust::Vec<rust::String> out;
    for (const QString& arg : Keel::launchArguments()) {
        const QByteArray utf8 = arg.toUtf8();
        out.push_back(rust::String(utf8.constData(), static_cast<std::size_t>(utf8.size())));
    }
    return out;
}
