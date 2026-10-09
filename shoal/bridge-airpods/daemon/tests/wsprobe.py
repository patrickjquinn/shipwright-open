#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 Patrick Quinn
# SPDX-License-Identifier: GPL-3.0-only
"""Dependency-free WebSocket probe for the daemon's JSON API.

    wsprobe.py [--url ws://127.0.0.1:2020/] [--hold SECONDS] [--no-pong]
               [--origin ORIGIN] [JSON ...]

Sends each JSON argument as a text frame after the connection opens, then
prints every frame received (one line each, prefixed with the elapsed time)
until --hold seconds pass or the server closes. With --no-pong, ping frames
are not answered, which shows the daemon's idle timeout. With --origin, the
handshake carries that Origin header, as a browser's would. Exit status 0 if
the server closed the connection, 2 if the hold time ran out first.
"""

import argparse
import base64
import os
import socket
import struct
import sys
import time
from urllib.parse import urlparse


def send_frame(sock, opcode, payload=b""):
    mask = os.urandom(4)
    head = bytes([0x80 | opcode])
    n = len(payload)
    if n < 126:
        head += bytes([0x80 | n])
    elif n < 65536:
        head += bytes([0x80 | 126]) + struct.pack(">H", n)
    else:
        head += bytes([0x80 | 127]) + struct.pack(">Q", n)
    sock.sendall(head + mask + bytes(b ^ mask[i % 4] for i, b in enumerate(payload)))


def recv_exact(sock, n):
    buf = b""
    while len(buf) < n:
        chunk = sock.recv(n - len(buf))
        if not chunk:
            raise ConnectionError("closed")
        buf += chunk
    return buf


def recv_frame(sock):
    b0, b1 = recv_exact(sock, 2)
    opcode = b0 & 0x0F
    n = b1 & 0x7F
    if n == 126:
        n = struct.unpack(">H", recv_exact(sock, 2))[0]
    elif n == 127:
        n = struct.unpack(">Q", recv_exact(sock, 8))[0]
    mask = recv_exact(sock, 4) if b1 & 0x80 else None
    data = recv_exact(sock, n)
    if mask:
        data = bytes(b ^ mask[i % 4] for i, b in enumerate(data))
    return opcode, data


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--url", default="ws://127.0.0.1:2020/")
    ap.add_argument("--hold", type=float, default=2.0)
    ap.add_argument("--no-pong", action="store_true")
    ap.add_argument("--origin", help="send this Origin header, as a web page would")
    ap.add_argument("messages", nargs="*")
    args = ap.parse_args()

    u = urlparse(args.url)
    sock = socket.create_connection((u.hostname, u.port or 80), timeout=5)
    key = base64.b64encode(os.urandom(16)).decode()
    sock.sendall((f"GET {u.path or '/'} HTTP/1.1\r\nHost: {u.hostname}:{u.port}\r\n"
                  f"Upgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: {key}\r\n"
                  f"Sec-WebSocket-Version: 13\r\n"
                  + (f"Origin: {args.origin}\r\n" if args.origin else "")
                  + "\r\n").encode())
    resp = b""
    while b"\r\n\r\n" not in resp:
        chunk = sock.recv(1)
        if not chunk:
            print("closed during handshake")
            return 1
        resp += chunk
    if b" 101 " not in resp.split(b"\r\n")[0]:
        print("handshake failed:", resp.split(b"\r\n")[0].decode())
        return 1

    start = time.monotonic()
    for m in args.messages:
        send_frame(sock, 0x1, m.encode())

    while True:
        left = args.hold - (time.monotonic() - start)
        if left <= 0:
            print(f"{time.monotonic() - start:6.2f} still open after {args.hold}s")
            return 2
        sock.settimeout(left)
        try:
            opcode, data = recv_frame(sock)
        except socket.timeout:
            continue
        except ConnectionError:
            print(f"{time.monotonic() - start:6.2f} connection closed by server (no close frame)")
            return 0
        t = time.monotonic() - start
        if opcode == 0x1:
            print(f"{t:6.2f} text {data.decode()}")
        elif opcode == 0x9:
            print(f"{t:6.2f} ping{' (not answered)' if args.no_pong else ''}")
            if not args.no_pong:
                send_frame(sock, 0xA, data)
        elif opcode == 0x8:
            code = struct.unpack(">H", data[:2])[0] if len(data) >= 2 else None
            print(f"{t:6.2f} close code={code} reason={data[2:].decode(errors='replace')!r}")
            return 0
        else:
            print(f"{t:6.2f} opcode {opcode} {data!r}")


if __name__ == "__main__":
    sys.exit(main())
