#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 Patrick Quinn
# SPDX-License-Identifier: GPL-3.0-only
"""Fake MPRIS media players for the daemon's ear detection pause tests.

Each player is a separate connection to the bus named by
DBUS_SESSION_BUS_ADDRESS (so each has its own unique name, like real
players), owns org.mpris.MediaPlayer2.<name> and exports
org.mpris.MediaPlayer2.Player at /org/mpris/MediaPlayer2 with Play, Pause,
PlayPause, Stop and PlaybackStatus. A status change emits PropertiesChanged.
Every call is logged as "fake-mpris: <name> <Method>", so a test can check
which player was asked to do what. Needs PyGObject (python3-gi).

    fake_mpris.py music=Playing app=Paused
"""

import sys

from gi.repository import Gio, GLib


def register(conn, path, info, call, get=None, set_=None):
    """register_object_with_closures2 where PyGObject has it (3.52+, where
    register_object is deprecated and warns), else register_object."""
    f = getattr(conn, "register_object_with_closures2", None) or conn.register_object
    return f(path, info, call, get, set_)

PATH = "/org/mpris/MediaPlayer2"
PLAYER = "org.mpris.MediaPlayer2.Player"
PLAYER_XML = """
<node>
  <interface name="org.mpris.MediaPlayer2.Player">
    <method name="Play"/>
    <method name="Pause"/>
    <method name="PlayPause"/>
    <method name="Stop"/>
    <property name="PlaybackStatus" type="s" access="read"/>
  </interface>
</node>
"""
player_info = Gio.DBusNodeInfo.new_for_xml(PLAYER_XML).interfaces[0]


def log(msg):
    print(f"fake-mpris: {msg}", flush=True)


class Player:
    def __init__(self, address, name, status):
        self.name = name
        self.status = status
        self.conn = Gio.DBusConnection.new_for_address_sync(
            address,
            Gio.DBusConnectionFlags.AUTHENTICATION_CLIENT | Gio.DBusConnectionFlags.MESSAGE_BUS_CONNECTION,
            None, None)
        register(self.conn, PATH, player_info, self.on_call, self.on_get)
        reply = self.conn.call_sync(
            "org.freedesktop.DBus", "/org/freedesktop/DBus", "org.freedesktop.DBus", "RequestName",
            GLib.Variant("(su)", ("org.mpris.MediaPlayer2." + name, 4)), None,
            Gio.DBusCallFlags.NONE, -1, None)
        if reply.unpack()[0] != 1:
            sys.exit(f"fake-mpris: cannot own org.mpris.MediaPlayer2.{name}")

    def set_status(self, status):
        if status == self.status:
            return
        self.status = status
        self.conn.emit_signal(
            None, PATH, "org.freedesktop.DBus.Properties", "PropertiesChanged",
            GLib.Variant("(sa{sv}as)", (PLAYER, {"PlaybackStatus": GLib.Variant("s", status)}, [])))

    def on_call(self, conn, sender, path, iface, method, params, invocation):
        log(f"{self.name} {method}")
        if method == "Play":
            self.set_status("Playing")
        elif method == "Pause":
            self.set_status("Paused")
        elif method == "PlayPause":
            self.set_status("Paused" if self.status == "Playing" else "Playing")
        elif method == "Stop":
            self.set_status("Stopped")
        invocation.return_value(None)

    def on_get(self, conn, sender, path, iface, prop):
        return GLib.Variant("s", self.status)


def main():
    address = GLib.getenv("DBUS_SESSION_BUS_ADDRESS")
    if not address:
        sys.exit("fake-mpris: DBUS_SESSION_BUS_ADDRESS is not set")
    players = []
    for arg in sys.argv[1:]:
        name, _, status = arg.partition("=")
        players.append(Player(address, name, status or "Playing"))
    log("ready " + " ".join(f"{p.name}={p.status}" for p in players))
    GLib.MainLoop().run()


if __name__ == "__main__":
    main()
