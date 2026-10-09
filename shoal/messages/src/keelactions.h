// SPDX-FileCopyrightText: 2026 Patrick Quinn
// SPDX-License-Identifier: Apache-2.0
//
// New file, added by Shipwright in 2026 (not part of upstream xmatic).
#ifndef KEELACTIONS_H
#define KEELACTIONS_H

#include <QDBusMessage>
#include <QElapsedTimer>
#include <QHash>
#include <QJsonObject>
#include <QList>
#include <QObject>
#include <QPointer>
#include <QQmlParserStatus>
#include <QString>
#include <QVariantMap>

#include <functional>

class QTimer;

/// What the page on top shows, for Pilot: `KeelContext` in `Keel.Actions 1.0`,
/// the same type and properties as Keel's (keel/actions/plugin), registered by
/// this app because Keel's plugin is Qt 6. One per page; bind `active` to the
/// page's status.
class KeelContext : public QObject, public QQmlParserStatus
{
    Q_OBJECT
    Q_INTERFACES(QQmlParserStatus)
    /// What the page is for ("reading a conversation").
    Q_PROPERTY(QString purpose MEMBER m_purpose NOTIFY changed)
    /// What the page shows, as a keel:// URI.
    Q_PROPERTY(QString entity MEMBER m_entity NOTIFY changed)
    /// Visible text the page chooses to give (at most 4,000 characters go out).
    Q_PROPERTY(QString text MEMBER m_text NOTIFY changed)
    /// True while this page is the current one.
    Q_PROPERTY(bool active MEMBER m_active NOTIFY activeChanged)

public:
    explicit KeelContext(QObject *parent = nullptr);
    ~KeelContext() override;

    void classBegin() override {}
    void componentComplete() override;

    bool isActive() const { return m_active; }
    QJsonObject snapshot(const QString &appId) const;

signals:
    void changed();
    void activeChanged();

private:
    QString m_purpose;
    QString m_entity;
    QString m_text;
    bool m_active = false;
};

/// Keel Actions (ADR-0018) for Pilot and other MCP clients:
/// `org.shipwright.Keel.Actions` at /org/shipwright/Keel/Actions under the
/// app's D-Bus name, as Keel's runtime serves it in a Qt 6 app. The actions,
/// their schemas and every check live in the core (core/src/actions.rs); a call
/// goes there as a `keel.*` command and its answer comes back by id. Only
/// GetContext is answered here, from the KeelContext of the page on top, and
/// only while the app is in the foreground or left it less than 30 s ago.
class KeelActions : public QObject
{
    Q_OBJECT
    Q_CLASSINFO("D-Bus Interface", "org.shipwright.Keel.Actions")

public:
    using Sender = std::function<quint64(const QString &, const QJsonObject &, bool)>;

    explicit KeelActions(QObject *parent = nullptr);
    ~KeelActions() override;

    /// The one instance, for the KeelContexts to find; null before it exists.
    static KeelActions *instance() { return s_instance; }

    void setSender(Sender sender) { m_send = std::move(sender); }

    /// Registers the object on the session bus. The name is not requested
    /// here: the share provider owns it on the same connection (the root QML),
    /// and a call to the name reaches every object of that connection.
    bool publish();

    /// The core's answer to a `keel.*` command. False when it is not one.
    bool takeReply(quint64 id, const QJsonObject &message);

    void addContext(KeelContext *context);
    void removeContext(KeelContext *context);

public slots:
    QString Describe(const QDBusMessage &message);
    QString Invoke(const QString &action, const QString &arguments, const QVariantMap &options,
                   const QDBusMessage &message);
    QString GetEntity(const QString &type, const QString &id, const QDBusMessage &message);
    QString FindEntities(const QString &type, const QString &query, uint limit,
                         const QDBusMessage &message);
    QString GetContext();

private:
    struct Pending {
        QDBusMessage message;
        QTimer *timer = nullptr;
    };

    void forward(const QString &command, const QJsonObject &arguments, int timeoutMs,
                 const QDBusMessage &message);
    void fail(const QDBusMessage &message, const QString &code, const QString &text);

    static KeelActions *s_instance;

    Sender m_send;
    /// Calls waiting for the core, by command id. A call that timed out stays
    /// with an empty message, so its late answer is still recognised as ours.
    QHash<quint64, Pending> m_pending;
    QList<QPointer<KeelContext>> m_contexts;
    bool m_foreground = false;
    QElapsedTimer m_sinceForeground;
};

#endif // KEELACTIONS_H
