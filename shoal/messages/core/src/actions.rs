// SPDX-FileCopyrightText: 2026 Patrick Quinn
// SPDX-License-Identifier: Apache-2.0
//
// New file, added by Shipwright in 2026 (not part of upstream xmatic).

//! Shoal Messages' Keel Actions (ADR-0018), for Pilot and other MCP clients:
//! find a conversation, read its latest messages, and send a text message
//! (destructive and outbound: Pilot always asks first). What the room on
//! screen shows is the `KeelContext` in qml/pages/RoomPage.qml.
//!
//! The declarations are the `#[keel::...]` items below; build.rs generates
//! actions.json from them. In a Keel app the Qt 6 Keel.Actions runtime checks
//! every call against that manifest; this app is Qt 5, so the same checks are
//! made here, with the validator keel-mcp and Reef use: arguments against the
//! tool's `inputSchema` before it runs, the result against its `outputSchema`,
//! and an entity cut down to what a model may see (uri, id, title and the
//! summarisable properties). src/keelactions.cpp serves them on D-Bus as the
//! `keel.*` commands.
//!
//! Message text does go to the model, unlike Shoal Mail's bodies: answering
//! "reply to this" needs it. Only `messages.read` hands it out, and only for
//! the conversation the request names.

use std::collections::HashMap;

use keel::{EntityRef, EntitySource, Error, ErrorCode, Invocation};
use keel_actions_schema::bounds::{FIND_LIMIT_MAX, FIND_QUERY_MAX_LENGTH, ID_MAX_LENGTH};
use keel_actions_schema::meta_key;
use keel_actions_schema::validate::validate;
use matrix_sdk::ruma::{RoomId, UserId};
use matrix_sdk::{Client, Room, RoomState};
use serde::Serialize;
use serde_json::{json, Value};

use crate::protocol::{reply_error, reply_ok};
use crate::text::strip_bidi;

keel::manifest!();

tokio::task_local! {
    /// The signed-in client, for the duration of one call: the declarations
    /// take only their arguments.
    static CLIENT: Client;
}

/// The longest a message's text gets in a transcript; a pasted log is cut.
const TEXT_MAX_CHARS: usize = 2000;
/// The most messages `messages.read` returns.
const READ_MAX: u32 = 30;

fn client() -> keel::Result<Client> {
    CLIENT
        .try_with(Client::clone)
        .map_err(|_| Error::not_available("Shoal Messages is not signed in"))
}

/// A conversation: a direct chat or a group room the person has joined.
#[keel::entity(
    type = "conversation",
    title = "Conversation",
    description = "A conversation in Shoal Messages (Matrix): a direct chat with one person or a group room. Its name, whether it is direct, and how many messages are unread."
)]
#[derive(Serialize)]
pub struct Conversation {
    id: String,
    title: String,
    #[keel(summarisable, indexable, values = ["direct", "group"], max_length = 8)]
    kind: String,
    #[keel(summarisable, minimum = 0, maximum = 1000000)]
    unread: u32,
}

impl Conversation {
    fn of(room: &Room) -> Self {
        let title = room
            .cached_display_name()
            .map(|name| strip_bidi(&name.to_string()))
            .filter(|name| !name.trim().is_empty())
            .unwrap_or_else(|| "Unnamed conversation".to_owned());
        Self {
            id: room.room_id().to_string(),
            title: cut(&title, 512),
            kind: if room.direct_targets_length() > 0 {
                "direct"
            } else {
                "group"
            }
            .to_owned(),
            unread: u32::try_from(room.num_unread_messages()).unwrap_or(u32::MAX),
        }
    }
}

impl EntitySource for Conversation {
    fn get(id: &str) -> keel::Result<Option<Self>> {
        Ok(joined(&client()?, id).map(|room| Self::of(&room)))
    }

    fn find(query: &str, limit: u32) -> keel::Result<Vec<Self>> {
        let mut rooms: Vec<(u8, u64, Room)> = client()?
            .joined_rooms()
            .into_iter()
            // Only what a reference can name; a space is not a conversation.
            .filter(|room| addressable(room.room_id().as_str()) && !room.is_space())
            .filter_map(|room| {
                let name = room
                    .cached_display_name()
                    .map(|name| name.to_string())
                    .unwrap_or_default();
                let score = rank(&name, query);
                let latest = room
                    .latest_event_timestamp()
                    .map_or(0, |ts| u64::from(ts.get()));
                (score > 0).then_some((score, latest, room))
            })
            .collect();
        // Best match first, then the most recent.
        rooms.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.cmp(&a.1)));
        Ok(rooms
            .iter()
            .take(limit as usize)
            .map(|(_, _, room)| Self::of(room))
            .collect())
    }
}

/// The joined room `id` names, if any.
fn joined(client: &Client, id: &str) -> Option<Room> {
    let room_id = RoomId::parse(id).ok()?;
    client
        .get_room(&room_id)
        .filter(|room| room.state() == RoomState::Joined)
}

/// Whether a room id can be an entity id (1 to 256 characters, none of
/// `/?#` or white space): the reference pattern refuses anything else.
fn addressable(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= ID_MAX_LENGTH as usize
        && !id.contains(|c: char| matches!(c, '/' | '?' | '#') || c.is_whitespace())
}

/// How well a conversation's name matches what was asked, ignoring case: 3
/// equal, 2 the name or one of its words starts with it, 1 it is in the name,
/// 0 not at all. An empty query matches everything (1).
fn rank(name: &str, query: &str) -> u8 {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return 1;
    }
    let name = name.to_lowercase();
    if name == query {
        3
    } else if name.starts_with(&query)
        || name
            .split(|c: char| !c.is_alphanumeric())
            .any(|word| word.starts_with(&query))
    {
        2
    } else {
        u8::from(name.contains(&query))
    }
}

/// `text` cut to `max` characters, with an ellipsis where it was cut.
fn cut(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_owned();
    }
    let mut out: String = text.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}

/// One message of a transcript.
#[keel::object]
#[derive(Serialize)]
pub struct Line {
    #[keel(max_length = 256, description = "Who wrote it, by display name")]
    from: String,
    #[keel(description = "True when the person using the phone wrote it")]
    own: bool,
    #[keel(
        format = "date-time",
        max_length = 32,
        description = "When it was sent (UTC)"
    )]
    time: String,
    #[keel(
        max_length = 2000,
        description = "Its text; an attachment, poll or unreadable message as a mark in brackets"
    )]
    text: String,
}

/// A conversation's latest messages, oldest first.
#[keel::object]
#[derive(Serialize)]
pub struct Transcript {
    conversation: EntityRef<Conversation>,
    #[keel(max_length = 512)]
    title: String,
    #[keel(max_items = 30)]
    messages: Vec<Line>,
}

#[keel::action(
    name = "messages.read",
    title = "Read a conversation",
    description = "Read the latest messages of a conversation in Shoal Messages, oldest first: who wrote each one, when, and its text. Use it before answering or summarising a conversation. Only messages already on the phone are read.",
    read_only = true,
    idempotent = true
)]
async fn read(
    conversation: EntityRef<Conversation>,
    #[keel(
        minimum = 1,
        maximum = 30,
        description = "How many messages (default 10)"
    )]
    limit: Option<u32>,
) -> keel::Result<Transcript> {
    let client = client()?;
    let room = joined(&client, conversation.id())
        .ok_or_else(|| Error::not_available("no such conversation"))?;
    let wanted = limit.unwrap_or(10).clamp(1, READ_MAX) as usize;
    let own = client.user_id().map(UserId::to_owned);

    let (cache, _drop_handles) = room
        .event_cache()
        .await
        .map_err(|_| Error::failed("the stored messages cannot be read"))?;
    let events = cache
        .events()
        .await
        .map_err(|_| Error::failed("the stored messages cannot be read"))?;

    // Newest first while collecting, so only the wanted ones are looked at.
    let mut lines = Vec::new();
    let mut names: HashMap<String, String> = HashMap::new();
    for stored in events.iter().rev() {
        if lines.len() == wanted {
            break;
        }
        let Ok(event) = serde_json::from_str::<Value>(stored.raw().json().get()) else {
            continue;
        };
        let Some(text) = message_text(&event) else {
            continue;
        };
        let sender = event["sender"].as_str().unwrap_or_default().to_owned();
        let from = match names.get(&sender) {
            Some(name) => name.clone(),
            None => {
                let name = sender_name(&room, &sender).await;
                names.insert(sender.clone(), name.clone());
                name
            }
        };
        lines.push(Line {
            from: cut(&from, 256),
            own: own.as_deref().is_some_and(|me| me.as_str() == sender),
            time: utc(event["origin_server_ts"].as_u64().unwrap_or(0)),
            text: cut(&text, TEXT_MAX_CHARS),
        });
    }
    lines.reverse();

    Ok(Transcript {
        conversation,
        title: Conversation::of(&room).title,
        messages: lines,
    })
}

#[keel::action(
    name = "message.send",
    title = "Send a message",
    description = "Send a text message now to a conversation in Shoal Messages, as the person using the phone. It leaves the device and cannot be taken back.",
    destructive = true,
    open_world = true,
    confirm = true,
    timeout_ms = 120000
)]
async fn send(
    conversation: EntityRef<Conversation>,
    #[keel(max_length = 16000, description = "The message, as plain text")] text: String,
) -> keel::Result<Sent> {
    let client = client()?;
    if text.trim().is_empty() {
        return Err(Error::invalid_arguments("the message is empty"));
    }
    if joined(&client, conversation.id()).is_none() {
        return Err(Error::not_available("no such conversation"));
    }
    // The way a forwarded text goes out: formatted where the composer would
    // format it, encrypted where the room is.
    crate::media::forward_text(&client, conversation.id(), text)
        .await
        .map_err(Error::failed)?;
    Ok(Sent {
        status: "sent".to_owned(),
    })
}

/// What `message.send` answers.
#[keel::object]
#[derive(Serialize)]
pub struct Sent {
    #[keel(values = ["sent"], max_length = 8)]
    status: String,
}

/// A sender's display name in `room`, else the user id.
async fn sender_name(room: &Room, sender: &str) -> String {
    let Ok(user) = UserId::parse(sender) else {
        return sender.to_owned();
    };
    room.get_member_no_sync(&user)
        .await
        .ok()
        .flatten()
        .and_then(|member| member.display_name().map(strip_bidi))
        .unwrap_or_else(|| sender.to_owned())
}

/// What a stored event says, as one transcript line: the text of a message,
/// a short mark for an attachment, poll or undecryptable message. `None` for
/// everything else, edits included (they repeat a message).
fn message_text(event: &Value) -> Option<String> {
    let content = &event["content"];
    let text = match event["type"].as_str()? {
        "m.room.message" => {
            if content["m.relates_to"]["rel_type"] == "m.replace" {
                return None;
            }
            let body = content["body"].as_str()?;
            // A caption is the body of an attachment that also names its file.
            let caption = content["filename"]
                .as_str()
                .filter(|name| *name != body)
                .map(|_| body);
            let mark = |kind: &str| match caption {
                Some(caption) => format!("[{kind}] {caption}"),
                None => format!("[{kind}]"),
            };
            match content["msgtype"].as_str().unwrap_or_default() {
                "m.image" => mark("picture"),
                "m.video" => mark("video"),
                "m.file" => mark("file"),
                "m.audio" if content.get("org.matrix.msc3245.voice").is_some() => {
                    "[voice message]".to_owned()
                }
                "m.audio" => mark("audio"),
                "m.location" => "[location]".to_owned(),
                "m.emote" => format!("* {}", without_quote(body)),
                _ => without_quote(body),
            }
        }
        "m.sticker" => "[sticker]".to_owned(),
        "m.poll.start" | "org.matrix.msc3381.poll.start" => {
            let question = content["org.matrix.msc1767.text"]
                .as_str()
                .or_else(|| content["m.text"][0]["body"].as_str())
                .unwrap_or_default();
            format!("[poll] {question}")
        }
        "m.room.encrypted" => "[a message this phone cannot decrypt]".to_owned(),
        _ => return None,
    };
    let text = strip_bidi(text.trim());
    (!text.is_empty()).then_some(text)
}

/// A reply's body without the quote older clients put in front of it
/// ("> <@anna:example.org> quoted", a blank line, then the reply).
fn without_quote(body: &str) -> String {
    if !body.starts_with("> ") {
        return body.to_owned();
    }
    let mut lines = body.lines().skip_while(|line| line.starts_with('>'));
    // The blank line that ends the quote.
    let rest: Vec<&str> = lines.by_ref().skip_while(|line| line.is_empty()).collect();
    rest.join("\n")
}

/// Milliseconds since the epoch as RFC 3339, UTC ("2026-10-08T14:03:00Z").
fn utc(ms: u64) -> String {
    let seconds = ms / 1000;
    let days = i64::try_from(seconds / 86_400).unwrap_or(0);
    let of_day = seconds % 86_400;
    // Howard Hinnant's civil_from_days: the proleptic Gregorian date.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let day_of_era = z.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_index + 2) / 5 + 1;
    let month = if month_index < 10 {
        month_index + 3
    } else {
        month_index - 9
    };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        of_day / 3600,
        of_day % 3600 / 60,
        of_day % 60
    )
}

// ---------------------------------------------------------------------------
// The checks the Keel.Actions runtime makes, and the replies.
// ---------------------------------------------------------------------------

fn manifest() -> keel::Result<Value> {
    keel::manifest_json()
        .and_then(|text| serde_json::from_str(text).ok())
        .ok_or_else(|| Error::failed("no Keel Actions manifest is built in"))
}

/// The manifest's tool for `action` (its name without the app ID).
fn tool(manifest: &Value, action: &str) -> Option<Value> {
    let name = format!("{}/{action}", manifest["appId"].as_str()?);
    manifest["tools"]
        .as_array()?
        .iter()
        .find(|tool| tool["name"] == name.as_str())
        .cloned()
}

fn entity_entry(manifest: &Value, entity_type: &str) -> Option<Value> {
    manifest["entities"]
        .as_array()?
        .iter()
        .find(|entity| entity["type"] == entity_type)
        .cloned()
}

fn unknown(what: impl std::fmt::Display) -> Error {
    Error::new(ErrorCode::UnknownAction, what)
}

/// The answer to a `keel.*` command: the value, or the failure with its
/// Keel error code beside the message.
pub fn reply(id: u64, result: keel::Result<Value>) -> Value {
    match result {
        Ok(value) => reply_ok(id, value),
        Err(error) => {
            let mut reply = reply_error(id, error.message);
            reply["code"] = json!(error.code.as_str());
            reply
        }
    }
}

/// `keel.describe`: the built-in manifest.
pub fn describe() -> keel::Result<Value> {
    manifest()
}

/// `keel.invoke`: checks `arguments`, runs the action with `client` (`None`
/// when nobody is signed in), and checks its result.
pub async fn invoke(client: Option<Client>, action: &str, arguments: Value) -> keel::Result<Value> {
    let manifest = manifest()?;
    let tool =
        tool(&manifest, action).ok_or_else(|| unknown(format!("no action named {action}")))?;
    let meta = &tool["_meta"];
    if meta.get(meta_key("find")).is_some() {
        return Err(unknown("entity searches go through FindEntities"));
    }
    validate(&tool["inputSchema"], &arguments)
        .map_err(|invalid| Error::invalid_arguments(invalid.to_string()))?;

    let invocation = keel::invoke(action, arguments)
        .ok_or_else(|| unknown(format!("no action named {action}")))?;
    let run = async move {
        match invocation {
            Invocation::Ready(result) => result,
            Invocation::Future(future) => future.await,
        }
    };
    let value = match client {
        Some(client) => CLIENT.scope(client, run).await,
        None => run.await,
    }?;

    // A result that does not match the schema is this app's bug, and is
    // never passed on.
    let value = match meta[meta_key("result")].as_str() {
        Some("none") => json!({}),
        Some("wrapped") => json!({ "result": value }),
        _ => value,
    };
    validate(&tool["outputSchema"], &value).map_err(|invalid| {
        Error::failed(format!("the result does not match its schema: {invalid}"))
    })?;
    Ok(value)
}

/// `keel.getEntity`: the entity with its uri, checked against its schema.
pub fn get_entity(client: Option<Client>, entity_type: &str, id: &str) -> keel::Result<Value> {
    let manifest = manifest()?;
    let entry = entity_entry(&manifest, entity_type)
        .ok_or_else(|| unknown(format!("no entity type {entity_type}")))?;
    let source = keel::entity_source(entity_type)
        .ok_or_else(|| unknown(format!("no entity type {entity_type}")))?;
    if id.is_empty() || id.chars().count() > ID_MAX_LENGTH as usize || id.contains('/') {
        return Err(Error::invalid_arguments(
            "an id has 1 to 256 characters and no '/'",
        ));
    }
    let found = with_client(client, || (source.get)(id))?;
    let Some(Value::Object(mut object)) = found else {
        return Err(Error::not_available("no such item"));
    };
    object.entry("id").or_insert_with(|| json!(id));
    object.insert("uri".into(), json!(uri(&manifest, entity_type, id)));
    let value = Value::Object(object);
    validate(&entry["schema"], &value).map_err(|invalid| {
        Error::failed(format!("the entity does not match its schema: {invalid}"))
    })?;
    Ok(value)
}

/// `keel.findEntities`: `{ items }` of references, each cut to what a model
/// may see.
pub fn find_entities(
    client: Option<Client>,
    entity_type: &str,
    query: &str,
    limit: u32,
) -> keel::Result<Value> {
    let manifest = manifest()?;
    if entity_entry(&manifest, entity_type).is_none() {
        return Err(unknown(format!("no entity type {entity_type}")));
    }
    let source = keel::entity_source(entity_type)
        .ok_or_else(|| unknown(format!("no entity type {entity_type}")))?;
    let find = tool(&manifest, &format!("{entity_type}.find"))
        .ok_or_else(|| unknown(format!("no entity type {entity_type}")))?;
    if query.chars().count() > FIND_QUERY_MAX_LENGTH as usize {
        return Err(Error::invalid_arguments(
            "the query is longer than 256 characters",
        ));
    }
    let limit = limit.clamp(1, FIND_LIMIT_MAX as u32);
    let found = with_client(client, || (source.find)(query, limit))?;
    let prefix = uri(&manifest, entity_type, "");
    let value = summaries(&find["outputSchema"], &found, &prefix, limit as usize);
    validate(&find["outputSchema"], &value).map_err(|invalid| {
        Error::failed(format!("the result does not match its schema: {invalid}"))
    })?;
    Ok(value)
}

fn with_client<T>(client: Option<Client>, f: impl FnOnce() -> keel::Result<T>) -> keel::Result<T> {
    match client {
        Some(client) => CLIENT.sync_scope(client, f),
        None => f(),
    }
}

fn uri(manifest: &Value, entity_type: &str, id: &str) -> String {
    format!(
        "keel://{}/{entity_type}/{id}",
        manifest["appId"].as_str().unwrap_or_default()
    )
}

/// The find tool's answer: at most `limit` items, each with only the
/// properties its output schema names, and its uri (`prefix` + id).
fn summaries(output: &Value, found: &[Value], prefix: &str, limit: usize) -> Value {
    let allowed = &output["properties"]["items"]["items"]["properties"];
    let items: Vec<Value> = found
        .iter()
        .take(limit)
        .map(|entity| {
            let mut item = serde_json::Map::new();
            if let Some(fields) = entity.as_object() {
                for (key, value) in fields {
                    if allowed.get(key).is_some() {
                        item.insert(key.clone(), value.clone());
                    }
                }
            }
            let id = item.get("id").and_then(Value::as_str).unwrap_or_default();
            let uri = format!("{prefix}{id}");
            item.insert("uri".into(), json!(uri));
            Value::Object(item)
        })
        .collect();
    json!({ "items": items })
}

#[cfg(test)]
mod tests {
    use super::*;

    const APP: &str = "org.shipwright.ShoalMessages";

    fn reference(id: &str) -> String {
        format!("keel://{APP}/conversation/{id}")
    }

    fn input(action: &str) -> Value {
        tool(&manifest().unwrap(), action).expect("declared")["inputSchema"].clone()
    }

    fn block_on<F: std::future::Future>(future: F) -> F::Output {
        tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap()
            .block_on(future)
    }

    #[test]
    fn the_manifest_declares_the_tools_and_the_context() {
        let manifest = manifest().unwrap();
        assert_eq!(manifest["appId"], APP);
        assert_eq!(manifest["context"]["uri"], format!("keel://{APP}/context"));
        let find = tool(&manifest, "conversation.find").expect("the entity's find tool");
        assert_eq!(find["annotations"]["readOnlyHint"], true);
        assert_eq!(find["annotations"]["idempotentHint"], true);
        let read = tool(&manifest, "messages.read").unwrap();
        assert_eq!(read["annotations"]["readOnlyHint"], true);
        assert_eq!(read["annotations"]["destructiveHint"], false);
        let send = tool(&manifest, "message.send").unwrap();
        assert_eq!(send["annotations"]["destructiveHint"], true);
        assert_eq!(send["annotations"]["openWorldHint"], true);
        assert_eq!(send["_meta"][meta_key("confirm")], true);
        let description = send["description"].as_str().unwrap();
        assert!(description.contains("leaves the device"), "{description}");
        assert!(
            description.contains("cannot be taken back"),
            "{description}"
        );
    }

    #[test]
    fn messages_read_schema() {
        let input = input("messages.read");
        validate(
            &input,
            &json!({ "conversation": reference("!a:example.org") }),
        )
        .unwrap();
        validate(
            &input,
            &json!({ "conversation": reference("!a:example.org"), "limit": 30 }),
        )
        .unwrap();
        assert!(validate(&input, &json!({})).is_err());
        assert!(validate(
            &input,
            &json!({ "conversation": reference("!a:example.org"), "limit": 31 })
        )
        .is_err());
        // Another app's reference, or a bare id, is not a conversation.
        assert!(validate(
            &input,
            &json!({ "conversation": "keel://org.example.notes/conversation/x" })
        )
        .is_err());
        assert!(validate(&input, &json!({ "conversation": "!a:example.org" })).is_err());
    }

    #[test]
    fn message_send_schema() {
        let input = input("message.send");
        let ok = json!({ "conversation": reference("!a:example.org"), "text": "On my way" });
        validate(&input, &ok).unwrap();
        assert!(validate(
            &input,
            &json!({ "conversation": reference("!a:example.org") })
        )
        .is_err());
        assert!(validate(&input, &json!({ "text": "hi" })).is_err());
        let long =
            json!({ "conversation": reference("!a:example.org"), "text": "x".repeat(16_001) });
        assert!(validate(&input, &long).is_err());
        let extra = json!({ "conversation": reference("!a:example.org"), "text": "hi", "to": "x" });
        assert!(validate(&input, &extra).is_err());
    }

    #[test]
    fn nothing_runs_without_a_session() {
        // No client: the action itself refuses, so nothing can be sent.
        let send = block_on(invoke(
            None,
            "message.send",
            json!({ "conversation": reference("!a:example.org"), "text": "hi" }),
        ));
        assert_eq!(send.unwrap_err().code, ErrorCode::NotAvailable);
        let read = block_on(invoke(
            None,
            "messages.read",
            json!({ "conversation": reference("!a:example.org") }),
        ));
        assert_eq!(read.unwrap_err().code, ErrorCode::NotAvailable);
        let find = find_entities(None, "conversation", "anna", 5);
        assert_eq!(find.unwrap_err().code, ErrorCode::NotAvailable);
        let get = get_entity(None, "conversation", "!a:example.org");
        assert_eq!(get.unwrap_err().code, ErrorCode::NotAvailable);
    }

    #[test]
    fn calls_are_checked_before_they_run() {
        let missing = block_on(invoke(None, "message.send", json!({ "text": "hi" })));
        assert_eq!(missing.unwrap_err().code, ErrorCode::InvalidArguments);
        let unknown = block_on(invoke(None, "message.delete", json!({})));
        assert_eq!(unknown.unwrap_err().code, ErrorCode::UnknownAction);
        // The find tool is FindEntities' alone.
        let find = block_on(invoke(None, "conversation.find", json!({ "query": "x" })));
        assert_eq!(find.unwrap_err().code, ErrorCode::UnknownAction);
        assert_eq!(
            get_entity(None, "conversation", "a/b").unwrap_err().code,
            ErrorCode::InvalidArguments
        );
        assert_eq!(
            get_entity(None, "note", "x").unwrap_err().code,
            ErrorCode::UnknownAction
        );
        assert_eq!(
            find_entities(None, "conversation", &"x".repeat(257), 5)
                .unwrap_err()
                .code,
            ErrorCode::InvalidArguments
        );
    }

    #[test]
    fn a_failure_reply_carries_its_code() {
        let reply = reply(7, Err(Error::not_available("not signed in")));
        assert_eq!(reply["id"], 7);
        assert_eq!(reply["ok"], false);
        assert_eq!(reply["code"], "NotAvailable");
        assert_eq!(reply["error"], "not signed in");
        assert_eq!(super::reply(8, Ok(json!({ "a": 1 })))["data"]["a"], 1);
    }

    #[test]
    fn find_results_carry_only_what_a_model_may_see() {
        let manifest = manifest().unwrap();
        let output = tool(&manifest, "conversation.find").unwrap()["outputSchema"].clone();
        let found = vec![
            json!({ "id": "!a:example.org", "title": "Anna", "kind": "direct", "unread": 2, "secret": "x" }),
            json!({ "id": "!b:example.org", "title": "Team", "kind": "group", "unread": 0 }),
        ];
        let prefix = uri(&manifest, "conversation", "");
        let value = summaries(&output, &found, &prefix, 1);
        validate(&output, &value).unwrap();
        let items = value["items"].as_array().unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0]["uri"], reference("!a:example.org"));
        assert!(items[0].get("secret").is_none());
    }

    #[test]
    fn names_rank_whole_then_word_then_part() {
        assert_eq!(rank("Anna", "anna"), 3);
        assert_eq!(rank("Anna Berg", "berg"), 2);
        assert_eq!(rank("Joanna", "anna"), 1);
        assert_eq!(rank("Team", "anna"), 0);
        assert_eq!(rank("Team", "  "), 1);
    }

    #[test]
    fn only_plain_room_ids_are_references() {
        assert!(addressable("!abc:example.org"));
        assert!(!addressable(""));
        assert!(!addressable("!a/b:example.org"));
        assert!(!addressable(&"x".repeat(257)));
    }

    #[test]
    fn messages_become_short_lines() {
        let text =
            |content: Value| message_text(&json!({ "type": "m.room.message", "content": content }));
        assert_eq!(
            text(json!({ "msgtype": "m.text", "body": "Hi" })).as_deref(),
            Some("Hi")
        );
        assert_eq!(
            text(json!({ "msgtype": "m.text", "body": "> <@a:x> Are you in?\n\nYes" })).as_deref(),
            Some("Yes")
        );
        assert_eq!(
            text(json!({ "msgtype": "m.image", "body": "IMG_1.jpg", "filename": "IMG_1.jpg" }))
                .as_deref(),
            Some("[picture]")
        );
        assert_eq!(
            text(json!({ "msgtype": "m.image", "body": "Look", "filename": "IMG_1.jpg" }))
                .as_deref(),
            Some("[picture] Look")
        );
        assert_eq!(
            text(json!({ "msgtype": "m.audio", "body": "Voice", "org.matrix.msc3245.voice": {} }))
                .as_deref(),
            Some("[voice message]")
        );
        // An edit repeats a message; a redaction has nothing left to say.
        assert_eq!(
            text(
                json!({ "msgtype": "m.text", "body": "* Hi", "m.relates_to": { "rel_type": "m.replace" } })
            ),
            None
        );
        assert_eq!(text(json!({})), None);
        assert_eq!(
            message_text(&json!({ "type": "m.reaction", "content": {} })),
            None
        );
        assert_eq!(
            message_text(&json!({ "type": "m.room.encrypted", "content": {} })).as_deref(),
            Some("[a message this phone cannot decrypt]")
        );
    }

    #[test]
    fn long_text_is_cut() {
        assert_eq!(cut("short", 10), "short");
        let long = cut(&"é".repeat(3000), TEXT_MAX_CHARS);
        assert_eq!(long.chars().count(), TEXT_MAX_CHARS);
        assert!(long.ends_with('…'));
    }

    #[test]
    fn times_are_utc() {
        assert_eq!(utc(0), "1970-01-01T00:00:00Z");
        assert_eq!(utc(951_782_400_000), "2000-02-29T00:00:00Z");
        assert_eq!(utc(1_791_461_000_000), "2026-10-08T12:03:20Z");
    }
}
