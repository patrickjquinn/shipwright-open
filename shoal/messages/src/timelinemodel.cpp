// Modified by Shipwright, 2026: the latest messages as text, for Pilot's context (Keel Actions); see CHANGES-FROM-UPSTREAM.md.
#include "timelinemodel.h"

#include <QJsonArray>
#include <QSet>
#include <QStringList>

TimelineModel::TimelineModel(QObject *parent)
    : DiffListModel(parent)
{
    // Whenever the rows move, the read mark may move with them: recomputed in one
    // pass, queued. A reset is a different timeline and says nothing about this one.
    connect(this, &QAbstractItemModel::modelReset, this, [this] {
        m_readCounts.clear();
        scheduleReadCounts();
    });
    connect(this, &QAbstractItemModel::rowsInserted, this, [this] { scheduleReadCounts(); });
    connect(this, &QAbstractItemModel::rowsRemoved, this, [this] { scheduleReadCounts(); });
    connect(this, &QAbstractItemModel::dataChanged, this, [this] { scheduleReadCounts(); });
    // Only a signal: the text is built when somebody reads it.
    connect(this, &QAbstractItemModel::modelReset, this, &TimelineModel::transcriptChanged);
    connect(this, &QAbstractItemModel::rowsInserted, this, &TimelineModel::transcriptChanged);
    connect(this, &QAbstractItemModel::rowsRemoved, this, &TimelineModel::transcriptChanged);
    connect(this, &QAbstractItemModel::dataChanged, this, &TimelineModel::transcriptChanged);
}

QVariant TimelineModel::data(const QModelIndex &index, int role) const
{
    if (role == ReadMarkRole || role == ReadMarkByRole) {
        // Looked up by the row's own event id: by row number it was one off after
        // every page of older messages, and the mark stood under the wrong message.
        const QString eventId = DiffListModel::data(index, EventIdRole).toString();
        const int count = eventId.isEmpty() ? 0 : m_readCounts.value(eventId);
        return role == ReadMarkRole ? QVariant(count > 0) : QVariant(count);
    }
    if (role == ThreadCountRole && !m_threadRoots.isEmpty()) {
        // The row's own number first - the SDK's summary, kept live. The server's list
        // is the fallback for roots that summary does not know.
        const int own = DiffListModel::data(index, role).toInt();
        if (own > 0) {
            return own;
        }
        const QString id = DiffListModel::data(index, EventIdRole).toString();
        if (!id.isEmpty()) {
            const int listed = m_threadRoots.value(id, 0);
            if (listed > 0) {
                return listed;
            }
        }
    }
    return DiffListModel::data(index, role);
}

void TimelineModel::setThreadRoots(const QHash<QString, int> &roots)
{
    if (roots == m_threadRoots) {
        return;
    }
    m_threadRoots = roots;
    if (rowCount() > 0) {
        emit dataChanged(index(0), index(rowCount() - 1), QVector<int>() << ThreadCountRole);
    }
}

void TimelineModel::scheduleReadCounts()
{
    // The emit at the end of the recount comes back here through dataChanged.
    if (m_updatingReadCounts || m_readCountsQueued) {
        return;
    }
    m_readCountsQueued = true;
    QMetaObject::invokeMethod(this, "updateReadCounts", Qt::QueuedConnection);
}

void TimelineModel::updateReadCounts()
{
    m_readCountsQueued = false;

    // One pass from the newest row backwards, carrying everyone met: a receipt
    // marks the newest event read, so whoever appears at a row has read it.
    QHash<QString, int> counts;
    QSet<QString> seen;
    for (int i = rows().count() - 1; i >= 0; --i) {
        const QJsonObject &row = rows().at(i);
        const QJsonArray users = row.value(QStringLiteral("readByUsers")).toArray();
        for (const QJsonValue &user : users) {
            seen.insert(user.toString());
        }
        const QString eventId = row.value(QStringLiteral("eventId")).toString();
        if (!eventId.isEmpty()
            && row.value(QStringLiteral("own")).toBool()
            && row.value(QStringLiteral("kind")).toString() == QLatin1String("message")) {
            // Never below what a message already showed: a receipt is taken off the old
            // row before it lands on the new one, and the eye vanished for that moment.
            counts.insert(eventId, qMax(seen.count(), m_readCounts.value(eventId)));
        }
    }

    if (counts == m_readCounts) {
        return;
    }
    m_readCounts = counts;

    if (rows().isEmpty()) {
        return;
    }
    m_updatingReadCounts = true;
    emit dataChanged(index(0, 0), index(rows().count() - 1, 0),
                     QVector<int> { ReadMarkRole, ReadMarkByRole });
    m_updatingReadCounts = false;
}

QHash<int, QByteArray> TimelineModel::roleNames() const
{
    QHash<int, QByteArray> names;
    names.insert(IdRole, "id");
    names.insert(EventIdRole, "eventId");
    names.insert(EditableRole, "editable");
    names.insert(MediaRole, "media");
    names.insert(ReplyToRole, "replyTo");
    names.insert(KindRole, "kind");
    names.insert(BodyRole, "body");
    names.insert(FormattedRole, "formatted");
    names.insert(MsgTypeRole, "msgtype");
    names.insert(PollRole, "poll");
    names.insert(LocationRole, "location");
    names.insert(SenderRole, "sender");
    names.insert(SenderNameRole, "senderName");
    names.insert(SenderAvatarRole, "senderAvatar");
    names.insert(SystemRole, "system");
    names.insert(NameRole, "name");
    names.insert(ReasonRole, "reason");
    names.insert(OwnRole, "own");
    names.insert(TimestampRole, "timestamp");
    names.insert(EditedRole, "edited");
    names.insert(PendingRole, "pending");
    names.insert(SendStateRole, "sendState");
    names.insert(EditStateRole, "editState");
    names.insert(RedactionStateRole, "redactionState");
    names.insert(ThreadRootRole, "threadRoot");
    names.insert(ThreadCountRole, "threadCount");
    names.insert(UtdCauseRole, "utdCause");
    names.insert(ReadByRole, "readBy");
    names.insert(CaptionRole, "caption");
    names.insert(TxnIdRole, "txnId");
    names.insert(ReactionsRole, "reactions");
    names.insert(ReadMarkRole, "readMark");
    names.insert(ReadMarkByRole, "readMarkBy");
    names.insert(ShieldRole, "shield");
    return names;
}

void TimelineModel::setPoll(const QString &eventId, const QJsonValue &poll)
{
    patchField(indexOfEvent(eventId), QStringLiteral("poll"), poll);
}

double TimelineModel::oldestTimestamp() const
{
    double oldest = 0;
    for (const QJsonObject &row : rows()) {
        const double stamp = row.value(QStringLiteral("timestamp")).toDouble();
        if (stamp > 0 && (oldest == 0 || stamp < oldest)) {
            oldest = stamp;
        }
    }
    return oldest;
}

namespace {

// How much of the conversation Pilot is given: enough to answer the latest
// message, well inside the 4,000 characters a context may carry.
const int TranscriptLines = 8;
const int TranscriptLineChars = 300;

// What a row says, in a line: its text, or a mark for what has none. Empty for
// a row that is not a message. In English: it is read by a model, not shown.
QString transcriptText(const QJsonObject &row)
{
    const QString kind = row.value(QStringLiteral("kind")).toString();
    if (kind == QLatin1String("undecryptable")) {
        return QStringLiteral("[a message this phone cannot decrypt]");
    }
    if (kind != QLatin1String("message")) {
        return QString();
    }
    const QString msgtype = row.value(QStringLiteral("msgtype")).toString();
    const QString caption = row.value(QStringLiteral("caption")).toString();
    QString mark;
    if (msgtype == QLatin1String("m.image") || msgtype == QLatin1String("m.sticker")) {
        mark = QStringLiteral("[picture]");
    } else if (msgtype == QLatin1String("m.video")) {
        mark = QStringLiteral("[video]");
    } else if (msgtype == QLatin1String("m.audio")) {
        mark = QStringLiteral("[audio]");
    } else if (msgtype == QLatin1String("m.file")) {
        mark = QStringLiteral("[file]");
    } else if (msgtype == QLatin1String("m.location") || msgtype == QLatin1String("m.beacon_info")) {
        mark = QStringLiteral("[location]");
    }
    if (!mark.isEmpty()) {
        return caption.isEmpty() ? mark : mark + QLatin1Char(' ') + caption;
    }
    if (msgtype == QLatin1String("m.poll")) {
        mark = QStringLiteral("[poll] ");
    }
    // One line each: a message's own line breaks would read as other messages.
    return mark + row.value(QStringLiteral("body")).toString().simplified();
}

} // namespace

QString TimelineModel::transcript() const
{
    QStringList lines;
    for (int i = rows().count() - 1; i >= 0 && lines.count() < TranscriptLines; --i) {
        const QJsonObject &row = rows().at(i);
        QString text = transcriptText(row);
        if (text.isEmpty()) {
            continue;
        }
        if (text.size() > TranscriptLineChars) {
            text = text.left(TranscriptLineChars - 1) + QChar(0x2026);
        }
        const QString name = row.value(QStringLiteral("own")).toBool()
                ? QStringLiteral("Me")
                : row.value(QStringLiteral("senderName")).toString().simplified();
        lines.prepend(name + QStringLiteral(": ") + text);
    }
    return lines.join(QLatin1Char('\n'));
}

int TimelineModel::indexOfEvent(const QString &eventId) const
{
    if (eventId.isEmpty()) {
        return -1;
    }
    for (int i = 0; i < rows().count(); ++i) {
        if (rows().at(i).value(QStringLiteral("eventId")).toString() == eventId) {
            return i;
        }
    }
    return -1;
}
