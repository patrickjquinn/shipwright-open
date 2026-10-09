// SPDX-FileCopyrightText: 2026 Patrick Quinn
// SPDX-License-Identifier: Apache-2.0
//
// New file, added by Shipwright in 2026 (not part of upstream xmatic).

//! "Mark all read", for the cover action (`rooms.markAllRead`).
//!
//! Every joined room that is not a space and has something unread (a count,
//! or the manual "unread" flag) goes through the same per-room path as the
//! chat list's "mark as read" (`roomlist::mark_read`): fully-read marker
//! always, a public receipt only where the user allows it, a private one
//! otherwise. At most `CONCURRENCY` rooms are in flight at once, so a long
//! list does not fire a hundred requests into the server's rate limiter.
//!
//! The scope is the whole account, not the space the chat list shows: the
//! cover counts the whole account (`RoomListModel::recountUnread`), and the
//! action clears what the cover counts.

use std::future::Future;

use futures_util::StreamExt;
use matrix_sdk::Client;

/// Rooms marked at once.
pub const CONCURRENCY: usize = 4;

/// What the walk needs to know about one room.
#[derive(Clone, Debug)]
pub struct RoomUnread {
    pub room_id: String,
    pub is_space: bool,
    pub unread_messages: u64,
    pub unread_notifications: u64,
    pub marked_unread: bool,
}

/// The rooms worth a receipt, in the order given.
pub fn select(rooms: &[RoomUnread]) -> Vec<String> {
    rooms
        .iter()
        .filter(|room| !room.is_space)
        .filter(|room| {
            room.unread_messages > 0 || room.unread_notifications > 0 || room.marked_unread
        })
        .map(|room| room.room_id.clone())
        .collect()
}

/// The outcome, for the reply.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Summary {
    /// Rooms that got a receipt.
    pub marked: usize,
    /// Rooms that had nothing to point a receipt at (no remote latest event).
    pub skipped: usize,
    /// Rooms whose receipt the server refused.
    pub failed: usize,
}

/// Runs `mark` over `rooms`, at most `limit` at a time.
pub async fn mark_each<F, Fut>(rooms: Vec<String>, limit: usize, mark: F) -> Summary
where
    F: Fn(String) -> Fut,
    Fut: Future<Output = Result<bool, String>>,
{
    let results: Vec<Result<bool, String>> = futures_util::stream::iter(rooms)
        .map(mark)
        .buffer_unordered(limit.max(1))
        .collect()
        .await;
    let mut summary = Summary::default();
    for result in results {
        match result {
            Ok(true) => summary.marked += 1,
            Ok(false) => summary.skipped += 1,
            Err(_) => summary.failed += 1,
        }
    }
    summary
}

/// Marks every unread room of the account read.
pub async fn mark_all(client: &Client, receipt: bool) -> Summary {
    let rooms: Vec<RoomUnread> = client
        .joined_rooms()
        .iter()
        .map(|room| RoomUnread {
            room_id: room.room_id().as_str().to_owned(),
            is_space: room.is_space(),
            unread_messages: room.num_unread_messages(),
            unread_notifications: room.num_unread_notifications(),
            marked_unread: room.is_marked_unread(),
        })
        .collect();
    let selected = select(&rooms);
    mark_each(selected, CONCURRENCY, |room_id| async move {
        crate::roomlist::mark_read(client, &room_id, receipt).await
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    fn room(id: &str, messages: u64, notifications: u64, flag: bool, space: bool) -> RoomUnread {
        RoomUnread {
            room_id: id.to_owned(),
            is_space: space,
            unread_messages: messages,
            unread_notifications: notifications,
            marked_unread: flag,
        }
    }

    #[test]
    fn selects_rooms_with_anything_unread_and_no_spaces() {
        let rooms = vec![
            room("!read", 0, 0, false, false),
            room("!messages", 3, 0, false, false),
            // Mentions-only rooms count notifications, not messages.
            room("!mention", 0, 1, false, false),
            // The manual "unread" flag has no count behind it.
            room("!flag", 0, 0, true, false),
            room("!space", 5, 5, true, true),
        ];
        assert_eq!(select(&rooms), vec!["!messages", "!mention", "!flag"]);
    }

    #[test]
    fn marks_every_room_with_bounded_concurrency() {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(4)
            .enable_time()
            .build()
            .unwrap();
        let running = Arc::new(AtomicUsize::new(0));
        let peak = Arc::new(AtomicUsize::new(0));
        let rooms: Vec<String> = (0..20).map(|n| format!("!r{n}")).collect();
        let summary = runtime.block_on(mark_each(rooms, CONCURRENCY, |room_id| {
            let running = running.clone();
            let peak = peak.clone();
            async move {
                let now = running.fetch_add(1, Ordering::SeqCst) + 1;
                peak.fetch_max(now, Ordering::SeqCst);
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
                running.fetch_sub(1, Ordering::SeqCst);
                match room_id.as_str() {
                    "!r3" => Err("refused".to_owned()),
                    "!r7" => Ok(false),
                    _ => Ok(true),
                }
            }
        }));
        assert_eq!(
            summary,
            Summary {
                marked: 18,
                skipped: 1,
                failed: 1
            }
        );
        let peak = peak.load(Ordering::SeqCst);
        assert!(peak <= CONCURRENCY, "peak {peak}");
        assert!(peak > 1, "ran one at a time");
    }

    #[test]
    fn nothing_to_do_is_a_quick_empty_summary() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        let summary = runtime.block_on(mark_each(Vec::new(), 0, |_| async { Ok(true) }));
        assert_eq!(summary, Summary::default());
    }
}
