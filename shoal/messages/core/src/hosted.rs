// SPDX-FileCopyrightText: 2026 Patrick Quinn
// SPDX-License-Identifier: Apache-2.0
//
// New file, added by Shipwright in 2026 (not part of upstream xmatic).

//! The Shipwright subscription's push account: the `push` section of the
//! onboarding bundle printed by `services/deploy/provision-subscriber.sh`,
//! handed to the Shoal Push distributor over its private control interface
//! (`org.shipwright.ShoalPush1.Configure`, see `shoal/push/README.md`,
//! "Hosted server").
//!
//! The bundle's `token` is an ntfy access token: whoever holds it can read
//! this subscriber's push topics. It never reaches a log, a `Debug` string or
//! an error message; `redact` guards every error that could echo it.

use serde_json::Value;
use std::collections::HashMap;
use std::fmt;
use zbus::zvariant::Value as DbusValue;

/// Where the distributor serves its control interface. Only the `ShoalPush`
/// Sailjail permission grants talking to this name.
pub const SHOAL_PUSH_NAME: &str = "org.shipwright.ShoalPush";
pub const SHOAL_PUSH_PATH: &str = "/org/shipwright/ShoalPush";
pub const SHOAL_PUSH_IFACE: &str = "org.shipwright.ShoalPush1";

/// The bundle's `push` object.
#[derive(Clone, PartialEq, Eq)]
pub struct HostedPush {
    /// The ntfy server the distributor subscribes to.
    pub server: String,
    /// The Matrix push gateway the homeserver posts to (the same ntfy).
    pub gateway: String,
    /// `None` where the bundle says `null`: a re-provisioned subscriber, whose
    /// distributor keeps the token it already has.
    pub token: Option<String>,
    pub topic_prefix: Option<String>,
}

impl fmt::Debug for HostedPush {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HostedPush")
            .field("server", &self.server)
            .field("gateway", &self.gateway)
            .field("token", &self.token.as_ref().map(|_| "<redacted>"))
            .field("topic_prefix", &self.topic_prefix)
            .finish()
    }
}

fn https(field: &str, value: &str) -> Result<String, String> {
    let value = value.trim();
    match url::Url::parse(value) {
        Ok(parsed) if parsed.scheme() == "https" && parsed.host_str().is_some() => {
            Ok(value.to_owned())
        }
        _ => Err(format!("\"{field}\" has to be an https address")),
    }
}

fn optional_string(push: &Value, field: &str) -> Result<Option<String>, String> {
    match push.get(field) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(text)) => Ok(Some(text.trim().to_owned())),
        Some(_) => Err(format!("\"{field}\" has to be text")),
    }
}

/// Reads either the whole onboarding bundle (`{"push": {...}, ...}`) or just
/// its `push` object. Everything else in the bundle is ignored here; in
/// particular the Matrix initial password is neither kept nor echoed.
pub fn parse_bundle(text: &str) -> Result<HostedPush, String> {
    let parsed: Value = serde_json::from_str(text.trim())
        .map_err(|_| "this is not an onboarding bundle (not valid JSON)".to_owned())?;
    let push = match parsed.get("push") {
        Some(push) => push,
        None if parsed.get("server").is_some() => &parsed,
        None => return Err("the bundle has no \"push\" section".to_owned()),
    };
    if !push.is_object() {
        return Err("the bundle's \"push\" section is not an object".to_owned());
    }
    let server = optional_string(push, "server")?
        .ok_or_else(|| "the push section has no \"server\"".to_owned())?;
    let server = https("server", &server)?;
    let gateway = optional_string(push, "gateway")?
        .ok_or_else(|| "the push section has no \"gateway\"".to_owned())?;
    let gateway = https("gateway", &gateway)?;
    let token = optional_string(push, "token")?;
    let topic_prefix = optional_string(push, "topic_prefix")?.filter(|p| !p.is_empty());
    if let Some(prefix) = &topic_prefix {
        // The distributor enforces the exact shape; this only catches a paste
        // that is obviously something else, with a message the user can act on.
        let sound = prefix.starts_with("up")
            && prefix.len() > 2
            && prefix
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit());
        if !sound {
            return Err("the push section's \"topic_prefix\" is malformed".to_owned());
        }
    }
    Ok(HostedPush {
        server,
        gateway,
        token,
        topic_prefix,
    })
}

/// Removes the token from any text that could carry it (a D-Bus error from
/// the distributor echoing its argument, for instance), then the usual ids.
pub fn redact(text: &str, push: &HostedPush) -> String {
    let mut text = text.to_owned();
    if let Some(token) = push.token.as_deref().filter(|t| !t.is_empty()) {
        text = text.replace(token, "<token>");
    }
    crate::text::scrub_ids(&text)
}

/// `Configure`'s argument: the bundle's keys. `gateway` is not the
/// distributor's business, and a null token is left out so the distributor
/// keeps its current one.
fn configure_args(push: &HostedPush) -> HashMap<&'static str, DbusValue<'_>> {
    let mut args = HashMap::new();
    args.insert("server", DbusValue::from(push.server.as_str()));
    if let Some(token) = &push.token {
        args.insert("token", DbusValue::from(token.as_str()));
    }
    if let Some(prefix) = &push.topic_prefix {
        args.insert("topic_prefix", DbusValue::from(prefix.as_str()));
    }
    args
}

/// Calls `Configure` on `connection`. Answers with the number of
/// registrations the distributor moved to new topics.
pub async fn configure_on(connection: &zbus::Connection, push: &HostedPush) -> Result<u32, String> {
    let reply = connection
        .call_method(
            Some(SHOAL_PUSH_NAME),
            SHOAL_PUSH_PATH,
            Some(SHOAL_PUSH_IFACE),
            "Configure",
            &(configure_args(push),),
        )
        .await
        .map_err(|error| describe(error, push))?;
    let body: HashMap<String, zbus::zvariant::OwnedValue> = reply
        .body()
        .deserialize()
        .map_err(|_| "Shoal Push answered in a form this version does not know".to_owned())?;
    Ok(body
        .get("moved")
        .and_then(|moved| u32::try_from(moved).ok())
        .unwrap_or(0))
}

/// A D-Bus failure as something the user can act on, token removed.
fn describe(error: zbus::Error, push: &HostedPush) -> String {
    if let zbus::Error::MethodError(name, detail, _) = &error {
        match name.as_str() {
            "org.freedesktop.DBus.Error.ServiceUnknown"
            | "org.freedesktop.DBus.Error.NameHasNoOwner" => {
                return "Shoal Push is not installed or not running (install shipwright-shoal-push; \
                        this app also needs its ShoalPush permission)"
                    .to_owned()
            }
            "org.freedesktop.DBus.Error.AccessDenied" => {
                return "Shoal Push refused the request: this app lacks the ShoalPush permission"
                    .to_owned()
            }
            "org.freedesktop.DBus.Error.InvalidArgs" => {
                return redact(
                    &format!(
                        "Shoal Push rejected the bundle: {}",
                        detail.as_deref().unwrap_or("invalid arguments")
                    ),
                    push,
                )
            }
            _ => {}
        }
    }
    redact(&format!("could not configure Shoal Push: {error}"), push)
}

/// `configure_on` against the session bus, on a thread of its own driven by
/// `zbus::block_on`, the same arrangement as the UnifiedPush connector in
/// `push.rs` (zbus here runs on its own executor, never tokio's).
pub async fn configure(push: HostedPush) -> Result<u32, String> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    std::thread::spawn(move || {
        let result = zbus::block_on(async {
            let connection = zbus::Connection::session()
                .await
                .map_err(|_| "no session bus: Shoal Push cannot be reached".to_owned())?;
            configure_on(&connection, &push).await
        });
        let _ = tx.send(result);
    });
    rx.await
        .unwrap_or_else(|_| Err("the Shoal Push call ended unexpectedly".to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    const TOKEN: &str = "tk_s3cr3tvalue0123456789";

    fn bundle(token: &str) -> String {
        format!(
            r#"{{"licence_id":"lic-1","new_account":true,
                "matrix":{{"user_id":"@a:shoal.example","homeserver":"https://matrix.shoal.example",
                           "initial_password":"pw-not-for-here"}},
                "push":{{"server":"https://push.shoal.example",
                         "gateway":"https://push.shoal.example/_matrix/push/v1/notify",
                         "token":{token},"topic_prefix":"upabcdef012345"}},
                "bridges":{{}}}}"#
        )
    }

    #[test]
    fn the_whole_bundle_parses() {
        let push = parse_bundle(&bundle(&format!("\"{TOKEN}\""))).unwrap();
        assert_eq!(push.server, "https://push.shoal.example");
        assert_eq!(
            push.gateway,
            "https://push.shoal.example/_matrix/push/v1/notify"
        );
        assert_eq!(push.token.as_deref(), Some(TOKEN));
        assert_eq!(push.topic_prefix.as_deref(), Some("upabcdef012345"));
    }

    #[test]
    fn the_push_section_alone_parses_and_a_null_token_is_kept_absent() {
        let push = parse_bundle(
            r#"{"server":"https://push.shoal.example","gateway":"https://push.shoal.example/_matrix/push/v1/notify","token":null}"#,
        )
        .unwrap();
        assert_eq!(push.token, None);
        assert_eq!(push.topic_prefix, None);
        assert!(!configure_args(&push).contains_key("token"));
    }

    #[test]
    fn bad_bundles_are_refused_without_echoing_them() {
        for (probe, expected) in [
            ("not json", "not valid JSON"),
            (r#"{"matrix":{}}"#, "no \"push\" section"),
            (r#"{"push":"x"}"#, "not an object"),
            (
                r#"{"push":{"gateway":"https://g.example/n"}}"#,
                "no \"server\"",
            ),
            (
                r#"{"push":{"server":"https://s.example"}}"#,
                "no \"gateway\"",
            ),
            (
                r#"{"push":{"server":"http://s.example","gateway":"https://g.example"}}"#,
                "https",
            ),
            (
                r#"{"push":{"server":"https://s.example","gateway":"https://g.example","token":5}}"#,
                "text",
            ),
            (
                r#"{"push":{"server":"https://s.example","gateway":"https://g.example","topic_prefix":"Nope!"}}"#,
                "topic_prefix",
            ),
        ] {
            let error = parse_bundle(probe).unwrap_err();
            assert!(error.contains(expected), "{probe} -> {error}");
        }
        // The password in the rest of the bundle never lands in an error.
        let broken = bundle("7").replace("https://push.shoal.example\"", "ftp://x\"");
        let error = parse_bundle(&broken).unwrap_err();
        assert!(!error.contains("pw-not-for-here"));
    }

    #[test]
    fn the_token_never_shows_in_debug_or_errors() {
        let push = parse_bundle(&bundle(&format!("\"{TOKEN}\""))).unwrap();
        let debug = format!("{push:?}");
        assert!(!debug.contains(TOKEN), "{debug}");
        assert!(debug.contains("<redacted>"));
        let echoed = format!("server said: bad token {TOKEN} for prefix");
        let redacted = redact(&echoed, &push);
        assert!(!redacted.contains(TOKEN), "{redacted}");
        assert!(redacted.contains("<token>"));
    }

    // ---- against a private dbus-daemon ----------------------------------

    use std::io::{BufRead as _, BufReader};
    use std::process::{Child, Command, Stdio};
    use std::sync::{Arc, Mutex};

    struct TestBus {
        child: Child,
        address: String,
    }

    impl TestBus {
        /// `None` where no dbus-daemon is installed: the test then says so and passes.
        fn start() -> Option<TestBus> {
            let mut child = Command::new("dbus-daemon")
                .args(["--session", "--nofork", "--nopidfile", "--print-address=1"])
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()
                .ok()?;
            let mut line = String::new();
            BufReader::new(child.stdout.take()?)
                .read_line(&mut line)
                .ok()?;
            Some(TestBus {
                child,
                address: line.trim().to_owned(),
            })
        }

        async fn connect(&self) -> zbus::Connection {
            zbus::connection::Builder::address(self.address.as_str())
                .unwrap()
                .build()
                .await
                .unwrap()
        }
    }

    impl Drop for TestBus {
        fn drop(&mut self) {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }

    /// Stands in for the distributor's control interface and records what it got.
    struct FakeShoalPush {
        seen: Arc<Mutex<Vec<HashMap<String, String>>>>,
    }

    #[zbus::interface(name = "org.shipwright.ShoalPush1")]
    impl FakeShoalPush {
        async fn configure(
            &self,
            args: HashMap<String, zbus::zvariant::OwnedValue>,
        ) -> zbus::fdo::Result<HashMap<String, zbus::zvariant::OwnedValue>> {
            let flat = args
                .iter()
                .map(|(key, value)| {
                    (
                        key.clone(),
                        String::try_from(value.clone()).unwrap_or_default(),
                    )
                })
                .collect::<HashMap<_, _>>();
            if flat.get("topic_prefix").map(String::as_str) == Some("upreject") {
                return Err(zbus::fdo::Error::InvalidArgs(format!(
                    "bad account for token {}",
                    flat.get("token").cloned().unwrap_or_default()
                )));
            }
            self.seen.lock().unwrap().push(flat);
            let mut reply = HashMap::new();
            reply.insert("moved".to_owned(), zbus::zvariant::OwnedValue::from(2u32));
            Ok(reply)
        }
    }

    #[test]
    fn configure_reaches_the_distributor_and_absence_is_explained() {
        let Some(bus) = TestBus::start() else {
            eprintln!("dbus-daemon not installed; skipping the bus test");
            return;
        };
        zbus::block_on(async {
            let client = bus.connect().await;
            let push = parse_bundle(&bundle(&format!("\"{TOKEN}\""))).unwrap();

            // Nobody owns the name yet.
            let error = configure_on(&client, &push).await.unwrap_err();
            assert!(error.contains("not installed or not running"), "{error}");

            let seen = Arc::new(Mutex::new(Vec::new()));
            let _service = zbus::connection::Builder::address(bus.address.as_str())
                .unwrap()
                .name(SHOAL_PUSH_NAME)
                .unwrap()
                .serve_at(SHOAL_PUSH_PATH, FakeShoalPush { seen: seen.clone() })
                .unwrap()
                .build()
                .await
                .unwrap();

            assert_eq!(configure_on(&client, &push).await, Ok(2));
            let got = seen.lock().unwrap().pop().unwrap();
            assert_eq!(
                got.get("server").map(String::as_str),
                Some("https://push.shoal.example")
            );
            assert_eq!(got.get("token").map(String::as_str), Some(TOKEN));
            assert_eq!(
                got.get("topic_prefix").map(String::as_str),
                Some("upabcdef012345")
            );
            assert!(!got.contains_key("gateway"));

            // A rejection that echoes the token comes back without it.
            let mut rejected = push.clone();
            rejected.topic_prefix = Some("upreject".into());
            let error = configure_on(&client, &rejected).await.unwrap_err();
            assert!(error.contains("rejected the bundle"), "{error}");
            assert!(!error.contains(TOKEN), "{error}");
        });
    }
}
