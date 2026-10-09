// SPDX-FileCopyrightText: 2026 Patrick Quinn
// SPDX-License-Identifier: Apache-2.0
//
// New file, added by Shipwright in 2026 (not part of upstream xmatic).

//! Linking the hosted Signal and Telegram bridges (`services/deploy`).
//!
//! The bridges are mautrix bridgev2 (`mautrix-signal`, `mautrix-telegram`,
//! v0.2609). Their provisioning API (`/_matrix/provision/v3/...`) is not
//! proxied by the cell's Caddy, so the app drives the bridges the way a person
//! would: commands in a direct chat with the bridge bot (`@signalbot`,
//! `@telegrambot`). bridgev2's command texts are stable and few; this module
//! is the whole contract with them:
//!
//! * `!signal login` / `!tg login phone` / `!tg login qr` start a login.
//!   A QR step arrives as an `m.image` whose `body` is the QR payload, and
//!   is refreshed by editing that image. A user-input step arrives as
//!   "Please enter your <field>" plus a description line; the answer is the
//!   next message (prefixed, so it works outside the management room too).
//! * `list-logins`, `logout <id>`, `cancel`.
//!
//! Source of every string matched below: mautrix/go `bridgev2/commands`
//! (`login.go`, `processor.go`, `event.go`), `bridgev2/matrixinvite.go`,
//! mautrix/signal `pkg/connector/login.go`, mautrix/telegram
//! `pkg/connector/login*.go`.
//!
//! Split in two: everything that decides (parsing, the per-bridge state,
//! what to send next) is synchronous and tested here against a scripted bot;
//! `runtime.rs` only carries messages to and from the room.
//!
//! Nothing typed into a login step (phone number, code, two-factor password)
//! is logged or kept: it lives in a `Zeroizing` buffer until it is sent.

use std::collections::BTreeMap;
use std::sync::Mutex as StdMutex;

use serde_json::{json, Value};
use zeroize::Zeroizing;

mod matrix;

pub use matrix::{install, send, transmit, wait_for_join};

/// Our marker on every command we send, so the event handler can tell where
/// live replies start: a room subscription replays the last events, and an
/// old QR code from an earlier attempt must not be shown as the current one.
pub const NONCE_FIELD: &str = "org.shipwright.bridge_request";

/// How long a status question waits for the bot.
pub const STATUS_TIMEOUT_SECS: u64 = 20;
/// How long a freshly invited bot has to join its direct chat.
pub const JOIN_TIMEOUT_SECS: u64 = 30;

/// One hosted bridge.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Network {
    Signal,
    Telegram,
}

impl Network {
    pub const ALL: [Network; 2] = [Network::Signal, Network::Telegram];

    pub fn id(self) -> &'static str {
        match self {
            Network::Signal => "signal",
            Network::Telegram => "telegram",
        }
    }

    pub fn parse(text: &str) -> Option<Network> {
        match text.trim().to_ascii_lowercase().as_str() {
            "signal" => Some(Network::Signal),
            "telegram" => Some(Network::Telegram),
            _ => None,
        }
    }

    /// bridgev2's default `appservice.bot.username` is `<network id>bot`;
    /// `services/deploy` keeps it.
    fn bot_localpart(self) -> &'static str {
        match self {
            Network::Signal => "signalbot",
            Network::Telegram => "telegrambot",
        }
    }

    /// The default `bridge.command_prefix`: the connector's
    /// `DefaultCommandPrefix`, else `!<network id>`. Telegram sets `!tg`.
    pub fn command_prefix(self) -> &'static str {
        match self {
            Network::Signal => "!signal",
            Network::Telegram => "!tg",
        }
    }

    /// The local part every ghost of this bridge starts with: bridgev2's
    /// `appservice.username_template` default `<network id>_{{.}}`, which
    /// `services/deploy` keeps.
    fn ghost_prefix(self) -> &'static str {
        match self {
            Network::Signal => "signal_",
            Network::Telegram => "telegram_",
        }
    }

    /// The bridge a user belongs to: one of its ghosts or its bot.
    pub fn of_user(user_id: &str) -> Option<Network> {
        let local = user_id.strip_prefix('@')?.split(':').next()?;
        Network::ALL.into_iter().find(|network| {
            local == network.bot_localpart()
                || local
                    .strip_prefix(network.ghost_prefix())
                    .is_some_and(|rest| !rest.is_empty())
        })
    }

    /// The bridge a room belongs to, judged by the people the room list
    /// knows without loading members: its heroes, its direct-chat targets
    /// and its service members (`io.element.functional_members`, which
    /// bridgev2 sets to the bot in every portal). A room with ghosts of two
    /// bridges, or none, is not marked.
    pub fn of_room<'a>(members: impl IntoIterator<Item = &'a str>) -> Option<Network> {
        let mut found = None;
        for network in members.into_iter().filter_map(Network::of_user) {
            match found {
                None => found = Some(network),
                Some(earlier) if earlier != network => return None,
                Some(_) => {}
            }
        }
        found
    }

    /// The flows the app knows how to drive. Telegram's `bot` and `manual`
    /// flows are for operators, not subscribers.
    pub fn flows(self) -> &'static [&'static str] {
        match self {
            Network::Signal => &["qr"],
            Network::Telegram => &["phone", "qr"],
        }
    }
}

/// The bot of `network` on the server of `own_user_id`, where the bundle did
/// not name one.
pub fn default_bot(network: Network, own_user_id: &str) -> Option<String> {
    let server = own_user_id.strip_prefix('@')?.split_once(':')?.1;
    if server.is_empty() {
        return None;
    }
    Some(format!("@{}:{}", network.bot_localpart(), server))
}

/// The bridge bots from an onboarding bundle's `bridges` section
/// (`{"signal": {"bot": "@signalbot:server", "logins": 0}, ...}`), keyed by
/// network id. Only well-formed user ids are taken; nothing else in the
/// bundle is read.
pub fn bots_from_bundle(bundle: &Value) -> BTreeMap<String, String> {
    let mut bots = BTreeMap::new();
    let Some(bridges) = bundle.get("bridges").and_then(Value::as_object) else {
        return bots;
    };
    for network in Network::ALL {
        let bot = bridges
            .get(network.id())
            .and_then(|entry| entry.get("bot"))
            .and_then(Value::as_str)
            .map(str::trim)
            .unwrap_or_default();
        if looks_like_user_id(bot) {
            bots.insert(network.id().to_owned(), bot.to_owned());
        }
    }
    bots
}

fn looks_like_user_id(text: &str) -> bool {
    matches!(text.strip_prefix('@').and_then(|rest| rest.split_once(':')),
        Some((local, server)) if !local.is_empty() && !server.is_empty()
            && !text.chars().any(char::is_whitespace))
}

/// What the app asks a bot.
pub enum Request {
    /// `list-logins`.
    Status,
    /// `login <flow>`.
    Login { flow: String },
    /// The answer to the current input step.
    Submit(Zeroizing<String>),
    /// `cancel`.
    Cancel,
    /// `logout <login id>`.
    Logout { login_id: String },
    /// `start-chat <identifier>`: a direct chat with the person behind a
    /// phone number (international form). Signal looks the number up on
    /// Signal's servers; Telegram finds only numbers the bridge has already
    /// seen on the linked account.
    StartChat { identifier: Zeroizing<String> },
}

impl std::fmt::Debug for Request {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Request::Status => f.write_str("Status"),
            Request::Login { flow } => write!(f, "Login({flow})"),
            Request::Submit(_) => f.write_str("Submit(<redacted>)"),
            Request::Cancel => f.write_str("Cancel"),
            Request::Logout { .. } => f.write_str("Logout"),
            Request::StartChat { .. } => f.write_str("StartChat(<redacted>)"),
        }
    }
}

/// The message text for `request`. Always prefixed: the prefix works in any
/// room, while a bare command only works in the bot's management room, which
/// a user who talked to the bot before may have elsewhere.
pub fn command_text(network: Network, request: &Request) -> Result<Zeroizing<String>, String> {
    let prefix = network.command_prefix();
    let text = match request {
        Request::Status => format!("{prefix} list-logins"),
        Request::Login { flow } => {
            if !network.flows().contains(&flow.as_str()) {
                return Err(format!("unknown {} login method", network.id()));
            }
            // Signal has a single flow; naming it is accepted all the same.
            format!("{prefix} login {flow}")
        }
        Request::Submit(value) => {
            let value = value.trim();
            if value.is_empty() {
                return Err("nothing to send".to_owned());
            }
            // One line: the bot reads the whole message as the value.
            if value.contains('\n') {
                return Err("the answer has to be on one line".to_owned());
            }
            format!("{prefix} {value}")
        }
        Request::Cancel => format!("{prefix} cancel"),
        Request::Logout { login_id } => {
            let id = login_id.trim();
            if id.is_empty() || id.chars().any(char::is_whitespace) {
                return Err("not a login of this bridge".to_owned());
            }
            format!("{prefix} logout {id}")
        }
        Request::StartChat { identifier } => {
            let number = identifier.trim();
            let digits = number.strip_prefix('+').unwrap_or_default();
            if digits.len() < 5 || !digits.chars().all(|c| c.is_ascii_digit()) {
                return Err("the number needs its country code".to_owned());
            }
            format!("{prefix} start-chat {number}")
        }
    };
    Ok(Zeroizing::new(text))
}

/// The kind of value an input step wants.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Field {
    Phone,
    Code,
    Password,
    Other,
}

impl Field {
    fn from_name(name: &str) -> Field {
        let lower = name.to_lowercase();
        if lower.contains("phone") {
            Field::Phone
        } else if lower.contains("password") {
            Field::Password
        } else if lower.contains("code") {
            Field::Code
        } else {
            Field::Other
        }
    }

    fn id(self) -> &'static str {
        match self {
            Field::Phone => "phone",
            Field::Code => "code",
            Field::Password => "password",
            Field::Other => "other",
        }
    }
}

/// One login of the user on a bridge, as `list-logins` reports it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Login {
    pub id: String,
    pub name: String,
    /// bridgev2 `status.BridgeStateEvent`: CONNECTED, CONNECTING,
    /// BACKFILLING, TRANSIENT_DISCONNECT, BAD_CREDENTIALS, UNKNOWN_ERROR,
    /// LOGGED_OUT, BRIDGE_UNREACHABLE, ...
    pub state: String,
}

impl Login {
    /// What the state means for the user.
    pub fn health(&self) -> &'static str {
        match self.state.as_str() {
            "CONNECTED" | "BACKFILLING" => "connected",
            "CONNECTING" | "TRANSIENT_DISCONNECT" | "STARTING" | "UNCONFIGURED" => "connecting",
            "BAD_CREDENTIALS" | "LOGGED_OUT" => "relink",
            _ => "error",
        }
    }
}

/// What one bot message means.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Reply {
    /// The greeting when the bot joins a new direct chat.
    Welcome,
    /// A line to show while the step continues ("Scan the QR code ...").
    Instructions(String),
    /// A QR code to show; `data` is what it encodes.
    Qr(String),
    /// A pairing code to show (bridgev2 `LoginDisplayTypeCode`).
    Code(String),
    /// The bot wants a value.
    Prompt {
        field: Field,
        name: String,
        description: String,
    },
    /// The last value was refused; the same step continues.
    Rejected(String),
    LoggedIn {
        remote_name: String,
    },
    Failed(String),
    Cancelled,
    NothingToCancel,
    AlreadyLoggingIn,
    TooManyLogins,
    Logins(Vec<Login>),
    NotLoggedIn,
    LoggedOut,
    LoginNotFound,
    UnknownCommand,
    /// `start-chat` made or found the direct chat.
    ChatStarted {
        room_id: String,
    },
    /// `start-chat`: nobody on the network has that number (or, on
    /// Telegram, the bridge has not seen it).
    ChatNotFound,
    /// `start-chat` failed, or the bridge cannot start chats.
    ChatFailed(String),
    /// Not addressed to the flow (warnings about the management room).
    Ignore,
    /// Anything else, shown as it is.
    Other(String),
}

/// Reads a bot message's JSON (a sync timeline event, decrypted). Edits are
/// read through their `m.new_content`: that is how a QR code is refreshed.
pub fn parse_event(event: &Value) -> Option<Reply> {
    if event.get("type").and_then(Value::as_str) != Some("m.room.message") {
        return None;
    }
    let content = event.get("content")?;
    let is_edit = content
        .get("m.relates_to")
        .and_then(|relation| relation.get("rel_type"))
        .and_then(Value::as_str)
        == Some("m.replace");
    let content = if is_edit {
        content.get("m.new_content")?
    } else {
        content
    };
    let msgtype = content
        .get("msgtype")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let body = content
        .get("body")
        .and_then(Value::as_str)
        .unwrap_or_default();
    Some(parse_reply(msgtype, body))
}

/// The meaning of one bot message, from its `msgtype` and `body`.
pub fn parse_reply(msgtype: &str, body: &str) -> Reply {
    let text = body.trim();
    if msgtype == "m.image" {
        // bridgev2 `sendQR` puts the QR payload itself in the body. Any other
        // image (a user-input attachment) is not something we can act on.
        return if text.contains("://") && !text.contains(char::is_whitespace) {
            Reply::Qr(text.to_owned())
        } else {
            Reply::Ignore
        };
    }
    if text.is_empty() {
        return Reply::Ignore;
    }

    if text.starts_with("Hello, I'm a ") && text.contains("bridge bot") {
        return Reply::Welcome;
    }
    if text.contains("This is not your management room") {
        return Reply::Ignore;
    }
    if let Some(rest) = text.strip_prefix("Please enter your ") {
        let mut lines = rest.lines();
        let name = lines.next().unwrap_or_default().trim().to_owned();
        let description = lines
            .filter(|line| !line.trim_start().starts_with("Options:"))
            .map(str::trim)
            .collect::<Vec<_>>()
            .join(" ");
        return Reply::Prompt {
            field: Field::from_name(&name),
            name,
            description,
        };
    }
    if let Some(rest) = text.strip_prefix("Successfully logged in as ") {
        return Reply::LoggedIn {
            remote_name: remote_name(rest),
        };
    }
    for prefix in [
        "Login failed: ",
        "Failed to start login: ",
        "Failed to prepare login process: ",
        "Failed to submit input: ",
        "Failed to send QR code: ",
    ] {
        if let Some(reason) = text.strip_prefix(prefix) {
            return Reply::Failed(reason.trim().to_owned());
        }
    }
    if text == "This login flow requires a client that supports client HTTP requests" {
        return Reply::Failed(text.to_owned());
    }
    if text.starts_with("Incorrect code") || text.starts_with("Incorrect password") {
        return Reply::Rejected(text.to_owned());
    }
    for prefix in ["Invalid value: ", "Invalid value for "] {
        if text.starts_with(prefix) {
            return Reply::Rejected(text.to_owned());
        }
    }
    if text.starts_with("You already have an ongoing ") {
        return Reply::AlreadyLoggingIn;
    }
    if text.starts_with("You have reached the maximum number of logins") {
        return Reply::TooManyLogins;
    }
    if text == "No ongoing command." {
        return Reply::NothingToCancel;
    }
    if text.ends_with(" cancelled.") && !text.contains('\n') {
        return Reply::Cancelled;
    }
    if text == "You're not logged in" {
        return Reply::NotLoggedIn;
    }
    if text == "Logged out" {
        return Reply::LoggedOut;
    }
    if text.starts_with("Login `") && text.ends_with("` not found") {
        return Reply::LoginNotFound;
    }
    // bridgev2 `fnResolveIdentifier`. The room is a matrix.to link, which
    // the bot's HTML-to-text keeps as "name (https://matrix.to/#/!id:server)".
    if text.starts_with("Created chat with ")
        || text.starts_with("You already have a direct chat with ")
    {
        return match room_link(text) {
            Some(room_id) => Reply::ChatStarted { room_id },
            None => Reply::ChatFailed("the bridge did not name the chat".to_owned()),
        };
    }
    if text.starts_with("Identifier `") && text.ends_with("` not found") {
        return Reply::ChatNotFound;
    }
    if let Some(reason) = text.strip_prefix("Failed to resolve identifier: ") {
        return Reply::ChatFailed(reason.trim().to_owned());
    }
    if text.starts_with("This bridge does not support ") {
        return Reply::ChatFailed("this bridge cannot start chats".to_owned());
    }
    if text.starts_with("Unknown command") {
        return Reply::UnknownCommand;
    }
    if text.starts_with("Please specify a login flow") || text.starts_with("Invalid login flow") {
        return Reply::Failed("this bridge does not offer that login method".to_owned());
    }
    let logins: Vec<Login> = text.lines().filter_map(login_line).collect();
    if !logins.is_empty() {
        return Reply::Logins(logins);
    }
    // A display-and-wait code step: `<code>ABCD-EFGH</code>`, which the
    // bridge's HTML-to-text turns into backticks.
    if let Some(code) = text
        .strip_prefix('`')
        .and_then(|rest| rest.strip_suffix('`'))
    {
        if !code.is_empty() && !code.contains('`') && !code.contains('\n') {
            return Reply::Code(code.to_owned());
        }
    }
    if text.starts_with("Scan the QR code")
        || text.starts_with("You have two-factor authentication enabled")
        || text.ends_with("to log in")
    {
        return Reply::Instructions(text.to_owned());
    }
    Reply::Other(text.to_owned())
}

/// The room ID of the first `https://matrix.to/#/!...` link in `text`,
/// percent-escaped or not.
fn room_link(text: &str) -> Option<String> {
    const LINK: &str = "matrix.to/#/";
    let mut rest = text;
    while let Some(at) = rest.find(LINK) {
        rest = &rest[at + LINK.len()..];
        let end = rest
            .find(|c: char| c.is_whitespace() || matches!(c, ')' | '/' | '?' | '"' | '>' | ']'))
            .unwrap_or(rest.len());
        let candidate = rest[..end]
            .replace("%21", "!")
            .replace("%3A", ":")
            .replace("%3a", ":");
        let well_formed = candidate
            .strip_prefix('!')
            .and_then(|id| id.split_once(':'))
            .is_some_and(|(local, server)| !local.is_empty() && !server.is_empty());
        if well_formed {
            return Some(candidate);
        }
    }
    None
}

/// "+4912345 / 0f1e..." (Signal) or "Jane Doe (`12345`)" (Telegram) down to
/// the part a person recognises.
fn remote_name(rest: &str) -> String {
    let rest = rest.trim();
    let name = match rest.find(" / ") {
        Some(at) => &rest[..at],
        None => match rest.rfind(" (`") {
            Some(at) => &rest[..at],
            None => rest,
        },
    };
    name.trim().to_owned()
}

/// One line of `list-logins`: "* `<id>` (<name>) - `<STATE>`". The list
/// marker may be "*", "-" or gone, depending on how the bot's HTML was
/// turned back into text.
fn login_line(line: &str) -> Option<Login> {
    let line = line.trim().trim_start_matches(['*', '-', '•']).trim_start();
    let rest = line.strip_prefix('`')?;
    let (id, rest) = rest.split_once('`')?;
    let rest = rest.trim_start().strip_prefix('(')?;
    let (name, state) = rest.rsplit_once(") - ")?;
    let state = state.trim().trim_matches('`');
    if id.is_empty()
        || state.is_empty()
        || !state.chars().all(|c| c.is_ascii_uppercase() || c == '_')
    {
        return None;
    }
    Some(Login {
        id: id.to_owned(),
        name: name.trim().to_owned(),
        state: state.to_owned(),
    })
}

/// Where a link attempt stands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Step {
    Idle,
    /// Sent `login`, waiting for the first step.
    Starting,
    Qr(String),
    Code(String),
    Input {
        field: Field,
        name: String,
        description: String,
    },
    /// Sent a value, waiting for the next step.
    Submitting,
    Done {
        remote_name: String,
    },
    Failed(String),
}

impl Step {
    fn in_progress(&self) -> bool {
        matches!(
            self,
            Step::Starting | Step::Qr(_) | Step::Code(_) | Step::Input { .. } | Step::Submitting
        )
    }
}

/// Something the core sends without being asked, because of a reply.
#[derive(Debug)]
pub enum FollowUp {
    /// The bot still holds an earlier login: cancel it, then start again.
    Restart { flow: String },
    /// Linking finished or a login went away: ask for the list again.
    Refresh,
    /// `start-chat` named the chat: join it, so it opens as a conversation
    /// rather than an invitation.
    JoinChat { room_id: String },
}

/// Where a `start-chat` stands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Chat {
    Idle,
    Resolving,
    Ready(String),
    NotFound,
    Failed(String),
}

/// One bridge as the app sees it.
#[derive(Clone, Debug)]
pub struct BridgeState {
    pub network: Network,
    /// `None` until the bot answered a status question.
    pub logins: Option<Vec<Login>>,
    pub step: Step,
    /// The latest line from the bot worth showing.
    pub message: String,
    /// The last value was refused; the same step asks again.
    pub retry: bool,
    /// The flow of the login in progress, for a restart.
    flow: Option<String>,
    restarted: bool,
    /// The input step last shown, for a refusal that comes without a prompt.
    last_input: Option<Step>,
    /// The latest `start-chat`.
    pub chat: Chat,
}

impl BridgeState {
    pub fn new(network: Network) -> Self {
        Self {
            network,
            logins: None,
            step: Step::Idle,
            message: String::new(),
            retry: false,
            flow: None,
            restarted: false,
            last_input: None,
            chat: Chat::Idle,
        }
    }

    /// Records what is being asked, before it is sent.
    pub fn on_request(&mut self, request: &Request) {
        match request {
            Request::Status => {}
            Request::Login { flow } => {
                self.step = Step::Starting;
                self.message.clear();
                self.retry = false;
                if self.flow.as_deref() != Some(flow.as_str()) {
                    self.restarted = false;
                }
                self.flow = Some(flow.clone());
            }
            Request::Submit(_) => {
                self.step = Step::Submitting;
                self.retry = false;
                self.message.clear();
            }
            Request::Cancel => {
                self.step = Step::Idle;
                self.message.clear();
            }
            Request::Logout { .. } => {}
            Request::StartChat { .. } => self.chat = Chat::Resolving,
        }
    }

    /// Takes one reply in. Returns whether anything visible changed and what
    /// to send next, if anything.
    pub fn on_reply(&mut self, reply: Reply) -> (bool, Option<FollowUp>) {
        let mut follow_up = None;
        match reply {
            Reply::Welcome | Reply::Ignore => return (false, None),
            Reply::Instructions(text) => self.message = text,
            Reply::Qr(data) => {
                self.step = Step::Qr(data);
                self.retry = false;
            }
            Reply::Code(code) => self.step = Step::Code(code),
            Reply::Prompt {
                field,
                name,
                description,
            } => {
                // The message stays: "Incorrect code" or "two-factor
                // authentication enabled" come just before the prompt they explain.
                self.step = Step::Input {
                    field,
                    name,
                    description,
                };
            }
            Reply::Rejected(text) => {
                self.retry = true;
                self.message = text;
                // "Invalid value" comes without a new prompt: the step that was
                // answered is still the one waiting.
                if self.step == Step::Submitting {
                    if let Some(previous) = self.last_input.take() {
                        self.step = previous;
                    }
                }
            }
            Reply::LoggedIn { remote_name } => {
                self.step = Step::Done { remote_name };
                self.message.clear();
                self.retry = false;
                self.flow = None;
                follow_up = Some(FollowUp::Refresh);
            }
            Reply::Failed(reason) => {
                self.step = Step::Failed(reason.clone());
                self.message = reason;
                self.flow = None;
            }
            Reply::Cancelled | Reply::NothingToCancel => {
                if self.step.in_progress() {
                    self.step = Step::Idle;
                }
            }
            Reply::AlreadyLoggingIn => {
                // Left over from an earlier attempt (the app was closed during a
                // login). Cancelled once and started again; a second refusal is
                // shown.
                match (&self.flow, self.restarted) {
                    (Some(flow), false) => {
                        self.restarted = true;
                        self.step = Step::Starting;
                        follow_up = Some(FollowUp::Restart { flow: flow.clone() });
                    }
                    _ => {
                        let reason = "the bridge is still busy with an earlier login".to_owned();
                        self.step = Step::Failed(reason.clone());
                        self.message = reason;
                    }
                }
            }
            Reply::TooManyLogins => {
                let reason = format!(
                    "this account is already linked to {}; unlink it first",
                    display_name(self.network)
                );
                self.step = Step::Failed(reason.clone());
                self.message = reason;
                self.flow = None;
            }
            Reply::Logins(logins) => self.logins = Some(logins),
            Reply::NotLoggedIn => {
                self.logins = Some(Vec::new());
                if self.chat == Chat::Resolving {
                    self.chat =
                        Chat::Failed(format!("{} is not linked yet", display_name(self.network)));
                }
            }
            Reply::LoggedOut => {
                self.message = "Unlinked".to_owned();
                follow_up = Some(FollowUp::Refresh);
            }
            Reply::LoginNotFound => follow_up = Some(FollowUp::Refresh),
            Reply::ChatStarted { room_id } => {
                self.chat = Chat::Ready(room_id.clone());
                follow_up = Some(FollowUp::JoinChat { room_id });
            }
            Reply::ChatNotFound => self.chat = Chat::NotFound,
            Reply::ChatFailed(reason) => self.chat = Chat::Failed(reason),
            Reply::UnknownCommand => {
                self.message = "the bridge did not understand the request".to_owned();
                if self.chat == Chat::Resolving {
                    self.chat = Chat::Failed(self.message.clone());
                }
                if self.step.in_progress() {
                    self.step = Step::Failed(self.message.clone());
                }
            }
            Reply::Other(text) => self.message = text,
        }
        if let Step::Input { .. } = &self.step {
            self.last_input = Some(self.step.clone());
        }
        (true, follow_up)
    }

    /// Overall: unknown, unlinked, connected, connecting, relink or error.
    pub fn health(&self) -> &'static str {
        let Some(logins) = &self.logins else {
            return "unknown";
        };
        if logins.is_empty() {
            return "unlinked";
        }
        let healths: Vec<&str> = logins.iter().map(Login::health).collect();
        for worst in ["relink", "error", "connecting"] {
            if healths.contains(&worst) {
                return worst;
            }
        }
        "connected"
    }

    fn chat_json(&self) -> Value {
        match &self.chat {
            Chat::Idle => json!({ "state": "idle" }),
            Chat::Resolving => json!({ "state": "resolving" }),
            Chat::Ready(room_id) => json!({ "state": "ready", "roomId": room_id }),
            Chat::NotFound => json!({ "state": "notFound" }),
            Chat::Failed(reason) => json!({ "state": "failed", "reason": reason }),
        }
    }

    /// The `bridge.state` payload. `qr` carries the code as module rows, so
    /// the page draws it without a QR library of its own.
    pub fn to_json(&self) -> Value {
        let (step, extra) = match &self.step {
            Step::Idle => ("idle", json!({})),
            Step::Starting => ("starting", json!({})),
            Step::Qr(data) => (
                "qr",
                json!({ "qr": qr_rows(data), "qrLink": qr_link(data) }),
            ),
            Step::Code(code) => ("code", json!({ "code": code })),
            Step::Input {
                field,
                name,
                description,
            } => (
                "input",
                json!({
                    "field": field.id(),
                    "fieldName": name,
                    "fieldDescription": description,
                }),
            ),
            Step::Submitting => ("submitting", json!({})),
            Step::Done { remote_name } => ("done", json!({ "remoteName": remote_name })),
            Step::Failed(reason) => ("failed", json!({ "reason": reason })),
        };
        let logins: Vec<Value> = self
            .logins
            .iter()
            .flatten()
            .map(|login| {
                json!({
                    "id": login.id,
                    "name": login.name,
                    "state": login.state,
                    "health": login.health(),
                })
            })
            .collect();
        let mut value = json!({
            "bridge": self.network.id(),
            "health": self.health(),
            "logins": logins,
            "step": step,
            "message": self.message,
            "retry": self.retry,
            "chat": self.chat_json(),
        });
        if let (Some(object), Value::Object(extra)) = (value.as_object_mut(), extra) {
            object.extend(extra);
        }
        value
    }
}

fn display_name(network: Network) -> &'static str {
    match network {
        Network::Signal => "Signal",
        Network::Telegram => "Telegram",
    }
}

/// The QR code as rows of '1' (dark) and '0' (light), quiet zone excluded.
/// Empty if the payload does not fit a QR code.
pub fn qr_rows(data: &str) -> Vec<String> {
    let Ok(code) = qrcode::QrCode::with_error_correction_level(data, qrcode::EcLevel::L) else {
        return Vec::new();
    };
    let width = code.width();
    let colors = code.to_colors();
    colors
        .chunks(width)
        .map(|row| {
            row.iter()
                .map(|color| {
                    if *color == qrcode::Color::Dark {
                        '1'
                    } else {
                        '0'
                    }
                })
                .collect()
        })
        .collect()
}

/// The payload as a link the phone itself may open, where it is one that a
/// Signal or Telegram app on this device (Android App Support) would take:
/// `sgnl://linkdevice?...` or `tg://login?...`. Otherwise empty.
fn qr_link(data: &str) -> String {
    if data.starts_with("sgnl://linkdevice") || data.starts_with("tg://login") {
        data.to_owned()
    } else {
        String::new()
    }
}

/// Where a bridge's commands go.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Endpoint {
    pub room_id: String,
    pub bot: String,
}

struct Session {
    endpoint: Endpoint,
    state: BridgeState,
    /// Bot messages count only after the echo of one of our commands: what
    /// came before is history the room subscription replays.
    armed: bool,
    /// Our commands not seen back yet, by marker.
    nonces: Vec<String>,
    /// Status questions waiting for a list.
    waiters: Vec<tokio::sync::oneshot::Sender<()>>,
}

/// A command ready to go out.
pub struct Prepared {
    pub text: Zeroizing<String>,
    pub nonce: String,
}

/// What one timeline event caused.
#[derive(Debug, Default)]
pub struct Observed {
    /// `bridge.state` payloads to emit.
    pub events: Vec<Value>,
    /// Commands to send next, per network.
    pub follow_ups: Vec<(Network, FollowUp)>,
}

/// Every bridge of the signed-in account. Shared between the commands and
/// the sync event handler; a plain mutex, never held across an await.
#[derive(Default)]
pub struct Registry {
    sessions: StdMutex<BTreeMap<Network, Session>>,
}

impl Registry {
    pub fn new() -> Self {
        Self::default()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, BTreeMap<Network, Session>> {
        self.sessions
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Forgets everything: on sign-out.
    pub fn clear(&self) {
        self.lock().clear();
    }

    /// The bridge's room, if one was set up in this run.
    pub fn endpoint(&self, network: Network) -> Option<Endpoint> {
        self.lock()
            .get(&network)
            .map(|session| session.endpoint.clone())
    }

    /// The current `bridge.state` payload.
    pub fn snapshot(&self, network: Network) -> Value {
        let sessions = self.lock();
        let mut value = match sessions.get(&network) {
            Some(session) => session.state.to_json(),
            None => BridgeState::new(network).to_json(),
        };
        if let (Some(object), Some(session)) = (value.as_object_mut(), sessions.get(&network)) {
            object.insert("bot".to_owned(), json!(session.endpoint.bot));
            object.insert("roomId".to_owned(), json!(session.endpoint.room_id));
        }
        value
    }

    /// Records `request` and returns the text to send. The endpoint may change
    /// (a new bot from a bundle, or a new room): the bridge state stays.
    pub fn prepare(
        &self,
        network: Network,
        endpoint: Endpoint,
        request: &Request,
        nonce: String,
    ) -> Result<Prepared, String> {
        let text = command_text(network, request)?;
        let mut sessions = self.lock();
        let session = sessions.entry(network).or_insert_with(|| Session {
            endpoint: endpoint.clone(),
            state: BridgeState::new(network),
            armed: false,
            nonces: Vec::new(),
            waiters: Vec::new(),
        });
        if session.endpoint != endpoint {
            session.endpoint = endpoint;
            session.armed = false;
            session.nonces.clear();
        }
        session.state.on_request(request);
        session.nonces.push(nonce.clone());
        // Bounded: a marker whose echo never comes must not pile up.
        if session.nonces.len() > 16 {
            session.nonces.remove(0);
        }
        Ok(Prepared { text, nonce })
    }

    /// A receiver that fires when the bot next reports the list of logins.
    pub fn wait_for_status(&self, network: Network) -> tokio::sync::oneshot::Receiver<()> {
        let (sender, receiver) = tokio::sync::oneshot::channel();
        if let Some(session) = self.lock().get_mut(&network) {
            session.waiters.push(sender);
        }
        receiver
    }

    /// Takes one `m.room.message` event from a sync. `own_user_id` tells our
    /// own echoes from the bot's replies.
    pub fn observe(&self, room_id: &str, own_user_id: &str, event: &Value) -> Observed {
        let mut observed = Observed::default();
        let sender = event
            .get("sender")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let mut sessions = self.lock();
        let Some((&network, session)) = sessions
            .iter_mut()
            .find(|(_, session)| session.endpoint.room_id == room_id)
        else {
            return observed;
        };

        if sender == own_user_id {
            let nonce = event
                .get("content")
                .and_then(|content| content.get(NONCE_FIELD))
                .and_then(Value::as_str);
            if let Some(nonce) = nonce {
                if let Some(at) = session.nonces.iter().position(|known| known == nonce) {
                    session.nonces.drain(..=at);
                    session.armed = true;
                }
            }
            return observed;
        }
        if sender != session.endpoint.bot || !session.armed {
            return observed;
        }
        let Some(reply) = parse_event(event) else {
            return observed;
        };
        let answers_status = matches!(reply, Reply::Logins(_) | Reply::NotLoggedIn);
        let (changed, follow_up) = session.state.on_reply(reply);
        if answers_status {
            for waiter in session.waiters.drain(..) {
                let _ = waiter.send(());
            }
        }
        if changed {
            let mut value = session.state.to_json();
            if let Some(object) = value.as_object_mut() {
                object.insert("bot".to_owned(), json!(session.endpoint.bot));
                object.insert("roomId".to_owned(), json!(session.endpoint.room_id));
            }
            observed.events.push(value);
        }
        if let Some(follow_up) = follow_up {
            observed.follow_ups.push((network, follow_up));
        }
        observed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ME: &str = "@alice:shoal.example";
    const SIGNAL_BOT: &str = "@signalbot:shoal.example";
    const TELEGRAM_BOT: &str = "@telegrambot:shoal.example";
    const SIGNAL_ROOM: &str = "!sig:shoal.example";
    const TELEGRAM_ROOM: &str = "!tg:shoal.example";

    /// A scripted bridgev2 bot. It answers the commands the app sends with the
    /// messages mautrix-go's command processor sends for them (texts copied
    /// from bridgev2/commands and the two connectors), so the tests run the
    /// real request/reply loop without a homeserver.
    #[derive(Default)]
    struct MockBot {
        network: Option<Network>,
        logins: Vec<Login>,
        /// What the ongoing login expects next.
        ongoing: Option<&'static str>,
        /// Telegram: the account has a two-factor password.
        two_factor: bool,
        events: u32,
    }

    impl MockBot {
        fn new(network: Network) -> Self {
            Self {
                network: Some(network),
                ..Self::default()
            }
        }

        fn event(&mut self, msgtype: &str, body: &str, edit_of: Option<&str>) -> Value {
            self.events += 1;
            let bot = match self.network {
                Some(Network::Signal) => SIGNAL_BOT,
                _ => TELEGRAM_BOT,
            };
            let content = match edit_of {
                Some(target) => json!({
                    "msgtype": msgtype,
                    "body": format!("* {body}"),
                    "m.new_content": { "msgtype": msgtype, "body": body },
                    "m.relates_to": { "rel_type": "m.replace", "event_id": target },
                }),
                None => json!({ "msgtype": msgtype, "body": body }),
            };
            json!({
                "type": "m.room.message",
                "sender": bot,
                "event_id": format!("$bot{}", self.events),
                "content": content,
            })
        }

        fn notice(&mut self, body: &str) -> Value {
            self.event("m.notice", body, None)
        }

        /// bridgev2's processor: strip the prefix, then a known command or
        /// the reply to the command state.
        fn handle(&mut self, text: &str) -> Vec<Value> {
            let network = self.network.unwrap();
            let message = text
                .strip_prefix(&format!("{} ", network.command_prefix()))
                .unwrap_or(text);
            let mut args = message.split_whitespace();
            let command = args.next().unwrap_or_default().to_lowercase();
            match command.as_str() {
                "list-logins" => {
                    if self.logins.is_empty() {
                        vec![self.notice("You're not logged in")]
                    } else {
                        // GetFormattedUserLogins starts with empty entries.
                        let lines: Vec<String> = self
                            .logins
                            .iter()
                            .map(|l| format!("* `{}` ({}) - `{}`", l.id, l.name, l.state))
                            .collect();
                        let body = format!("\n{}", lines.join("\n"));
                        vec![self.notice(&body)]
                    }
                }
                "cancel" => {
                    if self.ongoing.take().is_some() {
                        vec![self.notice("Login cancelled.")]
                    } else {
                        vec![self.notice("No ongoing command.")]
                    }
                }
                "logout" => {
                    let id = args.next().unwrap_or_default();
                    let before = self.logins.len();
                    self.logins.retain(|login| login.id != id);
                    if self.logins.len() < before {
                        vec![self.notice("Logged out")]
                    } else {
                        vec![self.notice(&format!("Login `{id}` not found"))]
                    }
                }
                "start-chat" | "pm" => {
                    // bridgev2 fnResolveIdentifier, as the bot's HTML-to-text
                    // renders it: the portal link keeps its URL in brackets.
                    if self.logins.is_empty() {
                        return vec![self.notice("You're not logged in")];
                    }
                    let number = args.next().unwrap_or_default();
                    match number {
                        "+491701234567" => vec![self.notice(
                            "Created chat with `0f1e` / Anna: Anna (https://matrix.to/#/!portal:shoal.example)",
                        )],
                        "+441234567890" => vec![self.notice(
                            "You already have a direct chat with `ab12` / Ben at Ben (https://matrix.to/#/!old:shoal.example)",
                        )],
                        "+15550100200" => vec![self.notice(
                            "Failed to resolve identifier: error looking up number on server: rate limited",
                        )],
                        other => vec![self.notice(&format!("Identifier `{other}` not found"))],
                    }
                }
                "login" => {
                    if self.ongoing.is_some() {
                        return vec![self.notice(&format!(
                            "You already have an ongoing login. You can use `{} cancel` to cancel it.",
                            network.command_prefix()
                        ))];
                    }
                    let flow = args.next().unwrap_or_default();
                    match (network, flow) {
                        (Network::Signal, "qr" | "") => {
                            self.ongoing = Some("scan");
                            vec![
                                self.notice("Scan the QR code on your Signal app to log in"),
                                self.event(
                                    "m.image",
                                    "sgnl://linkdevice?uuid=AAAA&pub_key=BBBB",
                                    None,
                                ),
                            ]
                        }
                        (Network::Telegram, "phone") => {
                            self.ongoing = Some("phone");
                            vec![self.notice(
                                "Please enter your Phone number\nInclude the country code with +",
                            )]
                        }
                        (Network::Telegram, "qr") => {
                            self.ongoing = Some("scan");
                            vec![
                                self.notice("Scan the QR code on your phone to log in"),
                                self.event("m.image", "tg://login?token=abc", None),
                            ]
                        }
                        _ => vec![self.notice(&format!(
                            "Invalid login flow `{flow}`. Available options:\n\n* `phone` - Login using your Telegram phone number"
                        ))],
                    }
                }
                _ => match self.ongoing {
                    Some("phone") => {
                        if !message.starts_with('+') {
                            return vec![
                                self.notice("Invalid value: phone number must start with +")
                            ];
                        }
                        self.ongoing = Some("code");
                        vec![self.notice(
                            "Please enter your Code\nThe code was sent to the Telegram app on your phone",
                        )]
                    }
                    Some("code") => {
                        if message != "12345" {
                            return vec![
                                self.notice("Incorrect code"),
                                self.notice("Please enter your Code"),
                            ];
                        }
                        if self.two_factor {
                            self.ongoing = Some("password");
                            return vec![
                                self.notice("You have two-factor authentication enabled."),
                                self.notice("Please enter your Password"),
                            ];
                        }
                        self.finish_telegram()
                    }
                    Some("password") => {
                        if message != "hunter2" {
                            return vec![
                                self.notice("Incorrect password, please try again. Use the official Telegram app to reset your password if you've forgotten it."),
                                self.notice("Please enter your Password"),
                            ];
                        }
                        self.finish_telegram()
                    }
                    _ => vec![self.notice("Unknown command, use the `help` command for help.")],
                },
            }
        }

        fn finish_telegram(&mut self) -> Vec<Value> {
            self.ongoing = None;
            self.logins.push(Login {
                id: "777".to_owned(),
                name: "Jane Doe".to_owned(),
                state: "CONNECTED".to_owned(),
            });
            vec![self.notice("Successfully logged in as Jane Doe (`777`)")]
        }

        /// The phone scanned the code (Signal).
        fn scanned(&mut self) -> Vec<Value> {
            self.ongoing = None;
            self.logins.push(Login {
                id: "0f1e2d3c".to_owned(),
                name: "+491701234567".to_owned(),
                state: "CONNECTED".to_owned(),
            });
            vec![self.notice("Successfully logged in as +491701234567 / 0f1e2d3c")]
        }
    }

    /// The app side of the loop: the registry plus what the runtime does with
    /// a command (send it, see its echo, see the bot's answer).
    struct Harness {
        registry: Registry,
        bot: MockBot,
        network: Network,
        room: &'static str,
        endpoint: Endpoint,
        emitted: Vec<Value>,
        sent: Vec<String>,
        nonces: u32,
        /// Chats the core would join (`FollowUp::JoinChat`).
        joined: Vec<String>,
    }

    impl Harness {
        fn new(network: Network) -> Self {
            let (room, bot) = match network {
                Network::Signal => (SIGNAL_ROOM, SIGNAL_BOT),
                Network::Telegram => (TELEGRAM_ROOM, TELEGRAM_BOT),
            };
            Self {
                registry: Registry::new(),
                bot: MockBot::new(network),
                network,
                room,
                endpoint: Endpoint {
                    room_id: room.to_owned(),
                    bot: bot.to_owned(),
                },
                emitted: Vec::new(),
                sent: Vec::new(),
                nonces: 0,
                joined: Vec::new(),
            }
        }

        fn send(&mut self, request: Request) {
            self.nonces += 1;
            let nonce = format!("n{}", self.nonces);
            let prepared = self
                .registry
                .prepare(self.network, self.endpoint.clone(), &request, nonce)
                .expect("command");
            let text = prepared.text.to_string();
            self.sent.push(text.clone());
            let echo = json!({
                "type": "m.room.message",
                "sender": ME,
                "content": { "msgtype": "m.text", "body": text, NONCE_FIELD: prepared.nonce },
            });
            self.deliver(vec![echo]);
            let replies = self.bot.handle(&text);
            self.deliver(replies);
        }

        fn deliver(&mut self, events: Vec<Value>) {
            for event in events {
                let observed = self.registry.observe(self.room, ME, &event);
                self.emitted.extend(observed.events);
                for (network, follow_up) in observed.follow_ups {
                    assert_eq!(network, self.network);
                    match follow_up {
                        FollowUp::Refresh => self.send(Request::Status),
                        FollowUp::JoinChat { room_id } => self.joined.push(room_id),
                        FollowUp::Restart { flow } => {
                            self.send(Request::Cancel);
                            self.send(Request::Login { flow });
                        }
                    }
                }
            }
        }

        fn state(&self) -> Value {
            self.registry.snapshot(self.network)
        }
    }

    #[test]
    fn signal_links_with_a_qr_code_that_refreshes() {
        let mut h = Harness::new(Network::Signal);
        h.send(Request::Status);
        assert_eq!(h.state()["health"], "unlinked");

        h.send(Request::Login {
            flow: "qr".to_owned(),
        });
        assert_eq!(h.sent.last().unwrap(), "!signal login qr");
        let state = h.state();
        assert_eq!(state["step"], "qr");
        assert_eq!(
            state["message"],
            "Scan the QR code on your Signal app to log in"
        );
        let rows = state["qr"].as_array().unwrap();
        assert!(rows.len() >= 21, "a real QR code: {} rows", rows.len());
        assert!(rows
            .iter()
            .all(|row| row.as_str().unwrap().len() == rows.len()));
        assert_eq!(state["qrLink"], "sgnl://linkdevice?uuid=AAAA&pub_key=BBBB");

        // After 45 s the bridge fetches a new code and edits the image.
        let first = state["qr"].clone();
        let refresh = h.bot.event(
            "m.image",
            "sgnl://linkdevice?uuid=CCCC&pub_key=DDDD",
            Some("$bot2"),
        );
        h.deliver(vec![refresh]);
        let state = h.state();
        assert_eq!(state["step"], "qr");
        assert_ne!(state["qr"], first);
        assert_eq!(state["qrLink"], "sgnl://linkdevice?uuid=CCCC&pub_key=DDDD");

        let done = h.bot.scanned();
        h.deliver(done);
        let state = h.state();
        assert_eq!(state["step"], "done");
        assert_eq!(state["remoteName"], "+491701234567");
        // The list was asked for again on its own.
        assert_eq!(h.sent.last().unwrap(), "!signal list-logins");
        assert_eq!(state["health"], "connected");
        assert_eq!(state["logins"][0]["id"], "0f1e2d3c");
    }

    #[test]
    fn telegram_links_with_phone_code_and_password() {
        let mut h = Harness::new(Network::Telegram);
        h.bot.two_factor = true;
        h.send(Request::Login {
            flow: "phone".to_owned(),
        });
        assert_eq!(h.sent.last().unwrap(), "!tg login phone");
        let state = h.state();
        assert_eq!(state["step"], "input");
        assert_eq!(state["field"], "phone");
        assert_eq!(state["fieldName"], "Phone number");
        assert_eq!(state["fieldDescription"], "Include the country code with +");

        // Refused without a new prompt: the same field again, with the reason.
        h.send(Request::Submit(Zeroizing::new("01701234567".to_owned())));
        let state = h.state();
        assert_eq!(state["step"], "input");
        assert_eq!(state["field"], "phone");
        assert_eq!(state["retry"], true);
        assert!(state["message"]
            .as_str()
            .unwrap()
            .contains("must start with +"));

        h.send(Request::Submit(Zeroizing::new("+491701234567".to_owned())));
        assert_eq!(h.sent.last().unwrap(), "!tg +491701234567");
        let state = h.state();
        assert_eq!(state["field"], "code");
        assert_eq!(state["retry"], false);

        h.send(Request::Submit(Zeroizing::new("99999".to_owned())));
        let state = h.state();
        assert_eq!(state["field"], "code");
        assert_eq!(state["retry"], true);
        assert_eq!(state["message"], "Incorrect code");

        h.send(Request::Submit(Zeroizing::new("12345".to_owned())));
        let state = h.state();
        assert_eq!(state["field"], "password");
        assert_eq!(
            state["message"],
            "You have two-factor authentication enabled."
        );

        h.send(Request::Submit(Zeroizing::new("hunter2".to_owned())));
        let state = h.state();
        assert_eq!(state["step"], "done");
        assert_eq!(state["remoteName"], "Jane Doe");
        assert_eq!(state["health"], "connected");
    }

    #[test]
    fn telegram_qr_flow_and_cancel() {
        let mut h = Harness::new(Network::Telegram);
        h.send(Request::Login {
            flow: "qr".to_owned(),
        });
        assert_eq!(h.state()["step"], "qr");
        assert_eq!(h.state()["qrLink"], "tg://login?token=abc");
        h.send(Request::Cancel);
        assert_eq!(h.state()["step"], "idle");
        assert!(h.bot.ongoing.is_none());
    }

    #[test]
    fn a_login_left_over_from_an_earlier_run_is_cancelled_and_restarted() {
        let mut h = Harness::new(Network::Telegram);
        h.bot.ongoing = Some("code");
        h.send(Request::Login {
            flow: "phone".to_owned(),
        });
        assert_eq!(
            h.sent,
            vec!["!tg login phone", "!tg cancel", "!tg login phone"]
        );
        assert_eq!(h.state()["step"], "input");
        assert_eq!(h.state()["field"], "phone");
    }

    #[test]
    fn unlinking_refreshes_the_list() {
        let mut h = Harness::new(Network::Telegram);
        h.bot.logins.push(Login {
            id: "777".to_owned(),
            name: "Jane Doe".to_owned(),
            state: "BAD_CREDENTIALS".to_owned(),
        });
        h.send(Request::Status);
        assert_eq!(h.state()["health"], "relink");
        h.send(Request::Logout {
            login_id: "777".to_owned(),
        });
        assert_eq!(h.sent.last().unwrap(), "!tg list-logins");
        assert_eq!(h.state()["health"], "unlinked");
    }

    #[test]
    fn replies_before_our_first_echo_are_history() {
        let registry = Registry::new();
        let endpoint = Endpoint {
            room_id: SIGNAL_ROOM.to_owned(),
            bot: SIGNAL_BOT.to_owned(),
        };
        registry
            .prepare(
                Network::Signal,
                endpoint,
                &Request::Login {
                    flow: "qr".to_owned(),
                },
                "n1".to_owned(),
            )
            .unwrap();
        let mut bot = MockBot::new(Network::Signal);
        // An old QR code replayed by the room subscription, before our echo.
        let old = bot.event("m.image", "sgnl://linkdevice?uuid=OLD&pub_key=OLD", None);
        assert!(registry.observe(SIGNAL_ROOM, ME, &old).events.is_empty());
        assert_eq!(registry.snapshot(Network::Signal)["step"], "starting");
        // Somebody else's echo, or ours without the marker, does not arm.
        let foreign = json!({ "type": "m.room.message", "sender": ME,
            "content": { "msgtype": "m.text", "body": "hi" } });
        registry.observe(SIGNAL_ROOM, ME, &foreign);
        assert!(registry.observe(SIGNAL_ROOM, ME, &old).events.is_empty());
        // Messages from another sender in the bot room are never the bot's.
        let echo = json!({ "type": "m.room.message", "sender": ME,
            "content": { "msgtype": "m.text", "body": "!signal login qr", NONCE_FIELD: "n1" } });
        registry.observe(SIGNAL_ROOM, ME, &echo);
        let mut impostor = bot.event("m.image", "sgnl://linkdevice?uuid=X&pub_key=Y", None);
        impostor["sender"] = json!("@mallory:shoal.example");
        assert!(registry
            .observe(SIGNAL_ROOM, ME, &impostor)
            .events
            .is_empty());
        let fresh = bot.event("m.image", "sgnl://linkdevice?uuid=NEW&pub_key=NEW", None);
        let observed = registry.observe(SIGNAL_ROOM, ME, &fresh);
        assert_eq!(observed.events.len(), 1);
        assert_eq!(
            observed.events[0]["qrLink"],
            "sgnl://linkdevice?uuid=NEW&pub_key=NEW"
        );
        assert_eq!(observed.events[0]["bot"], SIGNAL_BOT);
        // Other rooms are not ours.
        assert!(registry.observe("!other:x", ME, &fresh).events.is_empty());
    }

    #[test]
    fn a_status_waiter_fires_on_the_list() {
        let registry = Registry::new();
        let endpoint = Endpoint {
            room_id: TELEGRAM_ROOM.to_owned(),
            bot: TELEGRAM_BOT.to_owned(),
        };
        registry
            .prepare(
                Network::Telegram,
                endpoint,
                &Request::Status,
                "n1".to_owned(),
            )
            .unwrap();
        let mut waiter = registry.wait_for_status(Network::Telegram);
        let echo = json!({ "type": "m.room.message", "sender": ME,
            "content": { "body": "!tg list-logins", NONCE_FIELD: "n1" } });
        registry.observe(TELEGRAM_ROOM, ME, &echo);
        assert!(waiter.try_recv().is_err());
        let mut bot = MockBot::new(Network::Telegram);
        let reply = bot.notice("You're not logged in");
        registry.observe(TELEGRAM_ROOM, ME, &reply);
        assert!(waiter.try_recv().is_ok());
    }

    #[test]
    fn parses_the_bots_texts() {
        assert_eq!(
            parse_reply("m.notice", "Hello, I'm a Signal bridge bot.\n\nUse `help` for help or `login` to log in.\n\nThis room has been marked as your management room."),
            Reply::Welcome
        );
        assert_eq!(
            parse_reply("m.notice", "Login failed: login timed out"),
            Reply::Failed("login timed out".to_owned())
        );
        assert_eq!(
            parse_reply("m.notice", "Login cancelled."),
            Reply::Cancelled
        );
        assert_eq!(
            parse_reply("m.notice", "You have reached the maximum number of logins (1). Please logout from an existing login before creating a new one."),
            Reply::TooManyLogins
        );
        assert_eq!(
            parse_reply("m.notice", "`ABCD-1234`"),
            Reply::Code("ABCD-1234".to_owned())
        );
        assert_eq!(
            parse_reply("m.notice", "\u{26a0}\u{fe0f} This is not your management room. Entering login info must be prefixed with `!tg` like other commands."),
            Reply::Ignore
        );
        // Both list markers the HTML-to-text conversion may produce, and a name
        // with brackets of its own.
        assert_eq!(
            parse_reply(
                "m.notice",
                "- `1` (Jane (work)) - `CONNECTED`\n* `2` (+4917) - `TRANSIENT_DISCONNECT`"
            ),
            Reply::Logins(vec![
                Login {
                    id: "1".into(),
                    name: "Jane (work)".into(),
                    state: "CONNECTED".into()
                },
                Login {
                    id: "2".into(),
                    name: "+4917".into(),
                    state: "TRANSIENT_DISCONNECT".into()
                },
            ])
        );
        assert_eq!(
            parse_reply("m.notice", "Please enter your Phone number\nInclude the country code with +\nOptions: `a`, `b`"),
            Reply::Prompt {
                field: Field::Phone,
                name: "Phone number".into(),
                description: "Include the country code with +".into()
            }
        );
        assert_eq!(parse_reply("m.image", "qr.png"), Reply::Ignore);
        assert!(matches!(
            parse_reply("m.notice", "Something new"),
            Reply::Other(_)
        ));
    }

    #[test]
    fn commands_are_prefixed_and_checked() {
        let text = |request| command_text(Network::Telegram, &request).map(|t| t.to_string());
        assert_eq!(text(Request::Status).unwrap(), "!tg list-logins");
        assert!(text(Request::Login { flow: "bot".into() }).is_err());
        assert!(text(Request::Submit(Zeroizing::new("  ".into()))).is_err());
        assert!(text(Request::Submit(Zeroizing::new("a\nb".into()))).is_err());
        assert!(text(Request::Logout {
            login_id: "1 2".into()
        })
        .is_err());
        assert_eq!(
            command_text(Network::Signal, &Request::Login { flow: "qr".into() })
                .unwrap()
                .as_str(),
            "!signal login qr"
        );
        // The value never shows in a debug print.
        let debug = format!("{:?}", Request::Submit(Zeroizing::new("hunter2".into())));
        assert!(!debug.contains("hunter2"));
    }

    #[test]
    fn bots_come_from_the_bundle_or_the_users_server() {
        let bundle = json!({
            "matrix": { "user_id": "@a:s" },
            "bridges": {
                "signal": { "bot": "@signalbot:shoal.example", "logins": 0 },
                "telegram": { "bot": "not a user", "logins": 0 }
            }
        });
        let bots = bots_from_bundle(&bundle);
        assert_eq!(
            bots.get("signal").map(String::as_str),
            Some("@signalbot:shoal.example")
        );
        assert!(!bots.contains_key("telegram"));
        assert!(bots_from_bundle(&json!({ "push": {} })).is_empty());
        assert_eq!(
            default_bot(Network::Telegram, "@alice:shoal.example:8448").as_deref(),
            Some("@telegrambot:shoal.example:8448")
        );
        assert_eq!(default_bot(Network::Signal, "alice"), None);
    }

    #[test]
    fn health_takes_the_worst_login() {
        let mut state = BridgeState::new(Network::Signal);
        assert_eq!(state.health(), "unknown");
        state.logins = Some(vec![
            Login {
                id: "1".into(),
                name: "a".into(),
                state: "CONNECTED".into(),
            },
            Login {
                id: "2".into(),
                name: "b".into(),
                state: "CONNECTING".into(),
            },
        ]);
        assert_eq!(state.health(), "connecting");
        state.logins.as_mut().unwrap()[1].state = "UNKNOWN_ERROR".into();
        assert_eq!(state.health(), "error");
    }

    #[test]
    fn starts_a_chat_by_phone_number() {
        let mut h = Harness::new(Network::Signal);
        h.bot.scanned();
        h.send(Request::StartChat {
            identifier: Zeroizing::new("+491701234567".to_owned()),
        });
        assert_eq!(h.sent.last().unwrap(), "!signal start-chat +491701234567");
        let state = h.state();
        assert_eq!(state["chat"]["state"], "ready");
        assert_eq!(state["chat"]["roomId"], "!portal:shoal.example");
        assert_eq!(h.joined, vec!["!portal:shoal.example".to_owned()]);
        // A login step in progress is not disturbed by a chat.
        assert_eq!(state["step"], "idle");

        h.send(Request::StartChat {
            identifier: Zeroizing::new("+441234567890".to_owned()),
        });
        assert_eq!(h.state()["chat"]["roomId"], "!old:shoal.example");

        h.send(Request::StartChat {
            identifier: Zeroizing::new("+33612345678".to_owned()),
        });
        assert_eq!(h.state()["chat"]["state"], "notFound");

        h.send(Request::StartChat {
            identifier: Zeroizing::new("+15550100200".to_owned()),
        });
        let state = h.state();
        assert_eq!(state["chat"]["state"], "failed");
        assert!(state["chat"]["reason"]
            .as_str()
            .unwrap()
            .contains("rate limited"));
        assert_eq!(h.joined.len(), 2);
    }

    #[test]
    fn starting_a_chat_needs_a_link_and_a_full_number() {
        let mut h = Harness::new(Network::Telegram);
        h.send(Request::StartChat {
            identifier: Zeroizing::new("+491701234567".to_owned()),
        });
        let state = h.state();
        assert_eq!(state["chat"]["state"], "failed");
        assert_eq!(state["chat"]["reason"], "Telegram is not linked yet");
        assert_eq!(state["health"], "unlinked");
        for refused in ["01701234567", "+49 170", "+49abc", ""] {
            let request = Request::StartChat {
                identifier: Zeroizing::new(refused.to_owned()),
            };
            assert!(
                command_text(Network::Signal, &request).is_err(),
                "{refused}"
            );
            assert!(!format!("{request:?}").contains("0170"));
        }
    }

    #[test]
    fn reads_the_room_from_a_start_chat_reply() {
        assert_eq!(
            parse_reply(
                "m.notice",
                "Created chat with `x` / Y: Y (https://matrix.to/#/%21abc%3Ashoal.example)"
            ),
            Reply::ChatStarted {
                room_id: "!abc:shoal.example".to_owned()
            }
        );
        assert_eq!(
            parse_reply(
                "m.notice",
                "Created chat with `x`: [Y](https://matrix.to/#/!abc:s)"
            ),
            Reply::ChatStarted {
                room_id: "!abc:s".to_owned()
            }
        );
        // A user link before the room link is skipped.
        assert_eq!(
            room_link("Created chat with https://matrix.to/#/@y:s at https://matrix.to/#/!r:s")
                .as_deref(),
            Some("!r:s")
        );
        assert!(matches!(
            parse_reply("m.notice", "Created chat with somebody"),
            Reply::ChatFailed(_)
        ));
        assert!(matches!(
            parse_reply(
                "m.notice",
                "This bridge does not support resolving identifiers"
            ),
            Reply::ChatFailed(_)
        ));
    }

    #[test]
    fn tells_bridged_rooms_by_their_members() {
        assert_eq!(
            Network::of_user("@signal_0f1e:shoal.example"),
            Some(Network::Signal)
        );
        assert_eq!(
            Network::of_user("@telegram_777:shoal.example"),
            Some(Network::Telegram)
        );
        assert_eq!(
            Network::of_user("@signalbot:shoal.example"),
            Some(Network::Signal)
        );
        assert_eq!(
            Network::of_user("@telegrambot:shoal.example"),
            Some(Network::Telegram)
        );
        assert_eq!(Network::of_user("@signal_:shoal.example"), None);
        assert_eq!(Network::of_user("@signalfan:shoal.example"), None);
        assert_eq!(Network::of_user("@alice:shoal.example"), None);
        assert_eq!(Network::of_user("signal_x"), None);

        // A portal: ghosts as heroes, the bot as service member.
        assert_eq!(
            Network::of_room(["@signal_a:s", "@signal_b:s", "@signalbot:s"]),
            Some(Network::Signal)
        );
        // A DM with a ghost and a native user's room.
        assert_eq!(Network::of_room(["@telegram_1:s"]), Some(Network::Telegram));
        assert_eq!(Network::of_room(["@bob:s", "@carol:s"]), None);
        assert_eq!(Network::of_room([]), None);
        // Mixed: a native user in a bridged group is still the bridge's room;
        // ghosts of two bridges cannot be told apart.
        assert_eq!(
            Network::of_room(["@bob:s", "@signal_a:s"]),
            Some(Network::Signal)
        );
        assert_eq!(Network::of_room(["@signal_a:s", "@telegram_1:s"]), None);
    }
}
