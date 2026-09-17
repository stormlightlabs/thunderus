#!/usr/bin/env python3
"""Print a ULID: 48-bit millisecond timestamp plus 80 random bits, Crockford base32."""

import os
import sys
import time

ALPHABET = "0123456789ABCDEFGHJKMNPQRSTVWXYZ"


def ulid(now_ms: int | None = None) -> str:
    now_ms = int(time.time() * 1000) if now_ms is None else now_ms
    value = (now_ms << 80) | int.from_bytes(os.urandom(10), "big")
    return "".join(ALPHABET[(value >> shift) & 0x1F] for shift in range(125, -5, -5))


if __name__ == "__main__":
    count = int(sys.argv[1]) if len(sys.argv) > 1 else 1
    for _ in range(count):
        print(ulid())
