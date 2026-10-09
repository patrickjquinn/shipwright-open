// SPDX-FileCopyrightText: 2026 Patrick Quinn
// SPDX-License-Identifier: Apache-2.0
//
// New file, added by Shipwright in 2026 (not part of upstream xmatic).
#include "keelactions.h"

#include <QDBusConnection>
#include <QGuiApplication>
#include <QJsonArray>
#include <QJsonDocument>
#include <QTimer>

namespace {

// The app ID is the D-Bus name, which Sailjail fixes (OrganizationName and
// ApplicationName in the desktop file).
const char *kAppId = "org.shipwright.ShoalMessages";
const char *kPath = "/org/shipwright/Keel/Actions";

// How long a call waits for the core. An action's own limit is in the
// manifest; message.send's two minutes are the longest, and the entity calls
// and Describe have Keel's default.
const int kInvokeTimeoutMs = 120000;
const int kShortTimeoutMs = 25000;

// A context is given only while the app is in the foreground or left it this
// recently, as in Keel's runtime.
const qint64 kContextForegroundMs = 30000;

// The bounds Keel's runtime applies to a context (keel/actions/plugin).
const int kContextPurposeMax = 200;
const int kContextEntityMax = 2048;
const int kContextTextMax = 4000;

QString compact(const QJsonValue &value)
{
    if (value.isObject()) {
        return QString::fromUtf8(QJsonDocument(value.toObject()).toJson(QJsonDocument::Compact));
    }
    if (value.isArray()) {
        return QString::fromUtf8(QJsonDocument(value.toArray()).toJson(QJsonDocument::Compact));
    }
    return QStringLiteral("{}");
}

} // namespace

KeelContext::KeelContext(QObject *parent)
    : QObject(parent)
{
}

KeelContext::~KeelContext()
{
    if (KeelActions *actions = KeelActions::instance()) {
        actions->removeContext(this);
    }
}

void KeelContext::componentComplete()
{
    if (KeelActions *actions = KeelActions::instance()) {
        actions->addContext(this);
    }
}

QJsonObject KeelContext::snapshot(const QString &appId) const
{
    QJsonObject s{{QStringLiteral("app"), appId}};
    if (!m_purpose.isEmpty()) {
        s.insert(QStringLiteral("purpose"), m_purpose.left(kContextPurposeMax));
    }
    if (m_entity.startsWith(QLatin1String("keel://"))) {
        s.insert(QStringLiteral("entity"), m_entity.left(kContextEntityMax));
    }
    if (!m_text.isEmpty()) {
        s.insert(QStringLiteral("text"), m_text.left(kContextTextMax));
    }
    return s;
}

KeelActions *KeelActions::s_instance = nullptr;

KeelActions::KeelActions(QObject *parent)
    : QObject(parent)
{
    s_instance = this;
    auto *app = qobject_cast<QGuiApplication *>(QCoreApplication::instance());
    if (!app) {
        return;
    }
    const auto update = [this](Qt::ApplicationState state) {
        const bool foreground = state == Qt::ApplicationActive;
        if (m_foreground && !foreground) {
            m_sinceForeground.start();
        }
        m_foreground = foreground;
    };
    update(QGuiApplication::applicationState());
    connect(app, &QGuiApplication::applicationStateChanged, this, update);
}

KeelActions::~KeelActions()
{
    if (s_instance == this) {
        s_instance = nullptr;
    }
}

bool KeelActions::publish()
{
    QDBusConnection bus = QDBusConnection::sessionBus();
    if (!bus.isConnected()) {
        qWarning("shoal-messages: no session bus; Keel Actions are not served");
        return false;
    }
    if (!bus.registerObject(QString::fromLatin1(kPath), this, QDBusConnection::ExportAllSlots)) {
        qWarning("shoal-messages: the Keel Actions object could not be registered");
        return false;
    }
    return true;
}

void KeelActions::fail(const QDBusMessage &message, const QString &code, const QString &text)
{
    QDBusConnection::sessionBus().send(
        message.createErrorReply(QStringLiteral("org.shipwright.Keel.Actions.Error.") + code, text));
}

void KeelActions::forward(const QString &command, const QJsonObject &arguments, int timeoutMs,
                          const QDBusMessage &message)
{
    message.setDelayedReply(true);
    const quint64 id = m_send ? m_send(command, arguments, false) : 0;
    if (id == 0) {
        fail(message, QStringLiteral("NotAvailable"), QStringLiteral("the protocol core is not available"));
        return;
    }
    auto *timer = new QTimer(this);
    timer->setSingleShot(true);
    connect(timer, &QTimer::timeout, this, [this, id]() {
        auto it = m_pending.find(id);
        if (it == m_pending.end() || it->message.type() == QDBusMessage::InvalidMessage) {
            return;
        }
        fail(it->message, QStringLiteral("Timeout"), QStringLiteral("Shoal Messages did not answer in time"));
        it->message = QDBusMessage();
        it->timer->deleteLater();
        it->timer = nullptr;
    });
    timer->start(timeoutMs);
    Pending pending;
    pending.message = message;
    pending.timer = timer;
    m_pending.insert(id, pending);
}

bool KeelActions::takeReply(quint64 id, const QJsonObject &message)
{
    auto it = m_pending.find(id);
    if (it == m_pending.end()) {
        return false;
    }
    const Pending pending = it.value();
    m_pending.erase(it);
    if (pending.timer) {
        pending.timer->deleteLater();
    }
    // Timed out already: the caller was told.
    if (pending.message.type() == QDBusMessage::InvalidMessage) {
        return true;
    }
    if (message.value(QStringLiteral("ok")).toBool()) {
        QDBusConnection::sessionBus().send(
            pending.message.createReply(compact(message.value(QStringLiteral("data")))));
    } else {
        // A command the core could not even read has no Keel code: it failed.
        const QString code = message.value(QStringLiteral("code")).toString();
        fail(pending.message, code.isEmpty() ? QStringLiteral("Failed") : code,
             message.value(QStringLiteral("error")).toString());
    }
    return true;
}

QString KeelActions::Describe(const QDBusMessage &message)
{
    forward(QStringLiteral("keel.describe"), QJsonObject(), kShortTimeoutMs, message);
    return QString();
}

QString KeelActions::Invoke(const QString &action, const QString &arguments, const QVariantMap &options,
                            const QDBusMessage &message)
{
    // Whether the person started the request matters to the client, which
    // asks before anything destructive; no action here reads it.
    Q_UNUSED(options)
    const QJsonDocument doc = QJsonDocument::fromJson(arguments.toUtf8());
    if (!doc.isObject()) {
        message.setDelayedReply(true);
        fail(message, QStringLiteral("InvalidArguments"), QStringLiteral("arguments must be a JSON object"));
        return QString();
    }
    forward(QStringLiteral("keel.invoke"),
            QJsonObject{{QStringLiteral("action"), action}, {QStringLiteral("arguments"), doc.object()}},
            kInvokeTimeoutMs, message);
    return QString();
}

QString KeelActions::GetEntity(const QString &type, const QString &id, const QDBusMessage &message)
{
    forward(QStringLiteral("keel.getEntity"),
            QJsonObject{{QStringLiteral("type"), type}, {QStringLiteral("entityId"), id}},
            kShortTimeoutMs, message);
    return QString();
}

QString KeelActions::FindEntities(const QString &type, const QString &query, uint limit,
                                  const QDBusMessage &message)
{
    forward(QStringLiteral("keel.findEntities"),
            QJsonObject{{QStringLiteral("type"), type},
                        {QStringLiteral("query"), query},
                        {QStringLiteral("limit"), static_cast<int>(qMin<uint>(limit, 1000))}},
            kShortTimeoutMs, message);
    return QString();
}

QString KeelActions::GetContext()
{
    const bool recent = m_foreground
        || (m_sinceForeground.isValid() && m_sinceForeground.elapsed() < kContextForegroundMs);
    if (!recent) {
        return compact(QJsonObject());
    }
    // The most recently registered active context: the page on top.
    for (auto it = m_contexts.crbegin(); it != m_contexts.crend(); ++it) {
        if (*it && (*it)->isActive()) {
            return compact((*it)->snapshot(QString::fromLatin1(kAppId)));
        }
    }
    return compact(QJsonObject());
}

void KeelActions::addContext(KeelContext *context)
{
    m_contexts.removeAll(nullptr);
    m_contexts.append(context);
}

void KeelActions::removeContext(KeelContext *context)
{
    m_contexts.removeAll(context);
}
