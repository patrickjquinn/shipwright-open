// SPDX-FileCopyrightText: 2026 Patrick Quinn
// SPDX-License-Identifier: Apache-2.0
//
// New file, added by Shipwright in 2026 (not part of upstream xmatic).
#ifndef BRIDGEACTIONS_H
#define BRIDGEACTIONS_H

#include <QHash>
#include <QJsonObject>
#include <QObject>
#include <QString>
#include <QVariantMap>

#include <functional>

class AppSettings;

/// The hosted Signal and Telegram bridges, as QML sees them: `matrix.bridges`.
/// Holds the latest `bridge.state` the core reported per bridge and turns the
/// pages' taps into `bridges.*` commands. All decisions (what the bot said,
/// what comes next) are made in the core, `core/src/bridges.rs`.
class BridgeActions : public QObject
{
    Q_OBJECT

    /// The state of each bridge: `health` (unknown, unlinked, connected,
    /// connecting, relink, error), `logins`, `step` (idle, starting, qr, code,
    /// input, submitting, done, failed), `qr` (rows of '0'/'1'), `qrLink`,
    /// `field`, `fieldName`, `fieldDescription`, `message`, `retry`,
    /// `remoteName`, `chat` (the latest start-chat: `state` idle, resolving,
    /// ready, notFound or failed, with `roomId` or `reason`), plus `busy` and
    /// `error` kept here.
    Q_PROPERTY(QVariantMap signalBridge READ signalBridge NOTIFY changed)
    Q_PROPERTY(QVariantMap telegramBridge READ telegramBridge NOTIFY changed)

public:
    using Sender = std::function<quint64(const QString &, const QJsonObject &, bool)>;

    BridgeActions(AppSettings *settings, QObject *parent = nullptr);

    void setSender(Sender sender) { m_send = std::move(sender); }

    QVariantMap signalBridge() const { return state(QStringLiteral("signal")); }
    QVariantMap telegramBridge() const { return state(QStringLiteral("telegram")); }
    Q_INVOKABLE QVariantMap state(const QString &bridge) const;

    /// Asks the bot for the account's logins.
    Q_INVOKABLE void refresh(const QString &bridge);
    Q_INVOKABLE void refreshAll();
    /// Starts linking: `flow` "qr" (Signal, Telegram) or "phone" (Telegram).
    Q_INVOKABLE void link(const QString &bridge, const QString &flow);
    /// Answers the current step. The value is wiped from the outgoing buffer.
    Q_INVOKABLE void submit(const QString &bridge, const QString &value);
    Q_INVOKABLE void cancel(const QString &bridge);
    /// Unlinks one login; the bridge revokes the Signal or Telegram session.
    Q_INVOKABLE void unlink(const QString &bridge, const QString &loginId);
    /// Starts a direct chat with the person behind `number` (from the address
    /// book; a number without a country code is completed by the core).
    /// Answers with chatReady once the chat is joined.
    Q_INVOKABLE void startChat(const QString &bridge, const QString &number);

    /// A `bridge.state` event.
    void reportState(const QJsonObject &data);
    /// A `bridge.chatReady` event: the chat start-chat named is joined.
    void reportChatReady(const QJsonObject &data);
    /// The answer to a command this object sent. True if it was one.
    bool takeReply(quint64 id, bool ok, const QJsonObject &data, const QString &error);
    /// Bridge bots named in an onboarding bundle (`{"signal": "@signalbot:..."}`).
    void storeBots(const QJsonObject &bots);
    /// Signed out: nothing is known any more.
    void reset();

signals:
    void changed();
    /// A command failed; `message` is for the page.
    void failed(const QString &bridge, const QString &message);
    /// The chat a startChat asked for can be opened.
    void chatReady(const QString &bridge, const QString &roomId);

private:
    void request(const QString &bridge, const QString &command, QJsonObject arguments,
                 bool wipe = false);
    void setField(const QString &bridge, const QString &key, const QVariant &value);

    AppSettings *m_settings = nullptr;
    Sender m_send;
    QHash<QString, QVariantMap> m_states;
    /// Which bridge an outstanding command was for.
    QHash<quint64, QString> m_requests;
};

#endif // BRIDGEACTIONS_H
