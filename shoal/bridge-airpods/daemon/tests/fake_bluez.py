#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 Patrick Quinn
# SPDX-License-Identifier: GPL-3.0-only
"""A minimal fake BlueZ for exercising the daemon's adapter selection.

Owns org.bluez on the bus named by DBUS_SYSTEM_BUS_ADDRESS and exports
org.freedesktop.DBus.ObjectManager at "/" plus org.bluez.Adapter1 objects.
No devices. Needs PyGObject (python3-gi).

    fake_bluez.py hci0:off hci1:on             adapters present at start
    fake_bluez.py --add 2:hci1:on              add hci1 (powered) after 2 s
    fake_bluez.py hci0:off --power 3:hci0:on   power hci0 on after 3 s
    fake_bluez.py hci1:on --remove 3:hci1      remove hci1 after 3 s
"""

import argparse
import sys

from gi.repository import Gio, GLib


def register(conn, path, info, call, get=None, set_=None):
    """register_object_with_closures2 where PyGObject has it (3.52+, where
    register_object is deprecated and warns), else register_object."""
    f = getattr(conn, "register_object_with_closures2", None) or conn.register_object
    return f(path, info, call, get, set_)

ADAPTER_XML = """
<node>
  <interface name="org.bluez.Adapter1">
    <method name="StartDiscovery"/>
    <method name="StopDiscovery"/>
    <method name="SetDiscoveryFilter"><arg name="filter" type="a{sv}" direction="in"/></method>
    <property name="Powered" type="b" access="readwrite"/>
    <property name="Address" type="s" access="read"/>
  </interface>
</node>
"""

OM_XML = """
<node>
  <interface name="org.freedesktop.DBus.ObjectManager">
    <method name="GetManagedObjects">
      <arg name="objects" type="a{oa{sa{sv}}}" direction="out"/>
    </method>
    <signal name="InterfacesAdded">
      <arg name="object" type="o"/><arg name="interfaces" type="a{sa{sv}}"/>
    </signal>
    <signal name="InterfacesRemoved">
      <arg name="object" type="o"/><arg name="interfaces" type="as"/>
    </signal>
  </interface>
</node>
"""

adapter_info = Gio.DBusNodeInfo.new_for_xml(ADAPTER_XML).interfaces[0]
om_info = Gio.DBusNodeInfo.new_for_xml(OM_XML).interfaces[0]


def log(msg):
    print(f"fake-bluez: {msg}", flush=True)


class FakeBluez:
    def __init__(self, conn):
        self.conn = conn
        self.adapters = {}  # path -> {"powered": bool, "reg": id}
        register(conn, "/", om_info, self.on_om_call)

    def props(self, path):
        return {
            "Powered": GLib.Variant("b", self.adapters[path]["powered"]),
            "Address": GLib.Variant("s", "00:11:22:33:44:%02X" % (len(path) % 256)),
        }

    def on_om_call(self, conn, sender, path, iface, method, params, invocation):
        objs = {p: {"org.bluez.Adapter1": self.props(p)} for p in self.adapters}
        invocation.return_value(GLib.Variant("(a{oa{sa{sv}}})", (objs,)))

    def on_adapter_call(self, conn, sender, path, iface, method, params, invocation):
        log(f"{path}: {method}")
        invocation.return_value(None)

    def get_prop(self, conn, sender, path, iface, name):
        return self.props(path)[name]

    def set_prop(self, conn, sender, path, iface, name, value):
        if name == "Powered":
            self.set_powered(path, value.unpack())
        return True

    def add(self, path, powered, announce):
        reg = register(
            self.conn, path, adapter_info, self.on_adapter_call, self.get_prop, self.set_prop)
        self.adapters[path] = {"powered": powered, "reg": reg}
        log(f"add {path} powered={powered}")
        if announce:
            self.conn.emit_signal(
                None, "/", "org.freedesktop.DBus.ObjectManager", "InterfacesAdded",
                GLib.Variant("(oa{sa{sv}})", (path, {"org.bluez.Adapter1": self.props(path)})))

    def remove(self, path):
        self.conn.unregister_object(self.adapters.pop(path)["reg"])
        log(f"remove {path}")
        self.conn.emit_signal(
            None, "/", "org.freedesktop.DBus.ObjectManager", "InterfacesRemoved",
            GLib.Variant("(oas)", (path, ["org.bluez.Adapter1"])))

    def set_powered(self, path, powered):
        self.adapters[path]["powered"] = powered
        log(f"power {path} {powered}")
        self.conn.emit_signal(
            None, path, "org.freedesktop.DBus.Properties", "PropertiesChanged",
            GLib.Variant("(sa{sv}as)", ("org.bluez.Adapter1", {"Powered": GLib.Variant("b", powered)}, [])))


def parse_adapter(spec):
    name, _, state = spec.partition(":")
    return "/org/bluez/" + name, state != "off"


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("adapters", nargs="*", help="name:on|off present at start")
    ap.add_argument("--add", action="append", default=[], help="SECONDS:name:on|off")
    ap.add_argument("--remove", action="append", default=[], help="SECONDS:name")
    ap.add_argument("--power", action="append", default=[], help="SECONDS:name:on|off")
    args = ap.parse_args()

    loop = GLib.MainLoop()
    conn = Gio.bus_get_sync(Gio.BusType.SYSTEM, None)
    fake = FakeBluez(conn)
    for spec in args.adapters:
        fake.add(*parse_adapter(spec), announce=False)

    def later(spec, fn):
        secs, _, rest = spec.partition(":")
        GLib.timeout_add(int(float(secs) * 1000), lambda: (fn(rest), False)[1])

    for spec in args.add:
        later(spec, lambda r: fake.add(*parse_adapter(r), announce=True))
    for spec in args.remove:
        later(spec, lambda r: fake.remove("/org/bluez/" + r))
    for spec in args.power:
        later(spec, lambda r: fake.set_powered(*parse_adapter(r)))

    def owned(*_):
        log("owns org.bluez")

    Gio.bus_own_name_on_connection(conn, "org.bluez", Gio.BusNameOwnerFlags.NONE, owned, None)
    loop.run()


if __name__ == "__main__":
    sys.exit(main())
