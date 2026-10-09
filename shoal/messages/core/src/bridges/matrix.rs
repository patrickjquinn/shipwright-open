// SPDX-FileCopyrightText: 2026 Patrick Quinn
// SPDX-License-Identifier: Apache-2.0
//
// New file, added by Shipwright in 2026 (not part of upstream xmatic).

//! The Matrix side of the bridges: find or make the bot's direct chat, send a
//! command with our marker, and hand every message event of that room to the
//! registry. Thin on purpose; every decision is made in the parent module.

use std::sync::Arc;

use matrix_sdk::ruma::events::room::member::MembershipState;
use matrix_sdk::ruma::events::room::message::SyncRoomMessageEvent;
use matrix_sdk::ruma::serde::Raw;
use matrix_sdk::ruma::{RoomId, TransactionId, UserId};
use matrix_sdk::{Client, Room, RoomState};
use serde_json::{json, Value};

use super::{
    display_name, Endpoint, FollowUp, Network, Prepared, Registry, Request, JOIN_TIMEOUT_SECS,
    NONCE_FIELD,
};
use crate::runtime::Sink;
use crate::text::scrub_ids;

/// Sends `prepared` into the bot's room. The text is copied into the event
/// once; nothing else keeps it.
pub async fn transmit(room: &Room, prepared: &Prepared) -> Result<(), String> {
    let content = json!({
        "msgtype": "m.text",
        "body": prepared.text.as_str(),
        NONCE_FIELD: prepared.nonce,
    });
    room.send_raw("m.room.message", content)
        .await
        .map(|_| ())
        .map_err(|error| {
            format!(
                "could not reach the bridge: {}",
                scrub_ids(&error.to_string())
            )
        })
}

/// Records and sends one request.
pub async fn send(
    registry: &Registry,
    room: &Room,
    network: Network,
    endpoint: Endpoint,
    request: &Request,
) -> Result<(), String> {
    let prepared =
        registry.prepare(network, endpoint, request, TransactionId::new().to_string())?;
    transmit(room, &prepared).await
}

/// Waits until `bot` has joined `room`: a command sent before the bot is in
/// the room never reaches it.
pub async fn wait_for_join(room: &Room, bot: &UserId, network: Network) -> Result<(), String> {
    let start = tokio::time::Instant::now();
    let deadline = start + std::time::Duration::from_secs(JOIN_TIMEOUT_SECS);
    let mut asked = false;
    loop {
        // The join normally arrives with the sync; if it has not after a few
        // seconds, the member list is asked for once (lazy-loaded members).
        if !asked && start.elapsed() > std::time::Duration::from_secs(5) {
            asked = true;
            let _ = room.sync_members().await;
        }
        if let Ok(Some(member)) = room.get_member_no_sync(bot).await {
            match member.membership() {
                MembershipState::Join => return Ok(()),
                MembershipState::Leave | MembershipState::Ban => {
                    return Err(format!(
                        "the {} bridge left the chat; it may not be available on this server",
                        display_name(network)
                    ))
                }
                _ => {}
            }
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(format!(
                "the {} bridge did not answer; it may be down or not available on this server",
                display_name(network)
            ));
        }
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }
}

/// Sends what a reply asked for.
async fn follow(registry: Arc<Registry>, room: Room, network: Network, follow_up: FollowUp) {
    let Some(endpoint) = registry.endpoint(network) else {
        return;
    };
    let requests = match follow_up {
        FollowUp::Refresh => vec![Request::Status],
        FollowUp::Restart { flow } => vec![Request::Cancel, Request::Login { flow }],
        FollowUp::JoinChat { .. } => return,
    };
    for (at, request) in requests.iter().enumerate() {
        if at > 0 {
            // bridgev2 handles each command in its own goroutine: a login sent
            // on the heels of its cancel can overtake it.
            tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
        }
        if send(&registry, &room, network, endpoint.clone(), request)
            .await
            .is_err()
        {
            return;
        }
    }
}

/// Joins the chat `start-chat` named and tells the front end it can open
/// it. The bridge invites the user to a new portal; joining here is what the
/// user asked for by starting the chat.
async fn join_chat(client: Client, sink: Arc<Sink>, network: Network, room_id: String) {
    let joined = match RoomId::parse(room_id.as_str()) {
        Ok(parsed) => match client.get_room(&parsed) {
            Some(room) if room.state() == RoomState::Joined => Ok(()),
            _ => client
                .join_room_by_id(&parsed)
                .await
                .map(|_| ())
                .map_err(|error| scrub_ids(&format!("could not join the chat: {error}"))),
        },
        Err(_) => Err("the bridge named no valid chat".to_owned()),
    };
    let payload = match joined {
        Ok(()) => json!({ "bridge": network.id(), "roomId": room_id }),
        Err(error) => json!({ "bridge": network.id(), "error": error }),
    };
    sink.emit(crate::protocol::event("bridge.chatReady", payload));
}

/// Feeds every message event of a bridge room to the registry. Installed with
/// the other handlers when a client is made; rooms that are not a bridge's
/// are dismissed with one map lookup.
pub fn install(client: &Client, registry: Arc<Registry>, sink: Arc<Sink>) {
    client.add_event_handler(
        move |raw: Raw<SyncRoomMessageEvent>, room: Room, client: Client| {
            let registry = registry.clone();
            let sink = sink.clone();
            async move {
                let Some(own) = client.user_id() else { return };
                let Ok(event) = serde_json::from_str::<Value>(raw.json().get()) else {
                    return;
                };
                let observed = registry.observe(room.room_id().as_str(), own.as_str(), &event);
                for value in observed.events {
                    sink.emit(crate::protocol::event("bridge.state", value));
                }
                for (network, follow_up) in observed.follow_ups {
                    // Spawned: handlers run inside sync processing, and a send
                    // or a join from here would hold the sync up.
                    if let FollowUp::JoinChat { room_id } = follow_up {
                        tokio::spawn(join_chat(client.clone(), sink.clone(), network, room_id));
                    } else {
                        tokio::spawn(follow(registry.clone(), room.clone(), network, follow_up));
                    }
                }
            }
        },
    );
}
