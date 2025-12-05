#!/usr/bin/env python3
"""Cross-platform rustc wrapper for the custom Windows target.

On Unix hosts, this rewrites requests for the custom i686-win7-windows-msvc
triple to the MinGW-compatible GNU triple so the build can link successfully.
On Windows hosts, it simply forwards all arguments to rustc unchanged so native
MSVC toolchains can be used without requiring a shell interpreter.
"""

from __future__ import annotations

import os
import subprocess
import sys
from typing import List


def rewrite_args(raw_args: List[str]) -> List[str]:
    next_is_target = False
    rewritten: List[str] = []
    for entry in raw_args:
        if next_is_target:
            rewritten.append(
                "i686-win7-windows-gnu" if entry == "i686-win7-windows-msvc" else entry
            )
            next_is_target = False
            continue

        if entry == "--target":
            next_is_target = True
            rewritten.append(entry)
        elif entry == "--target=i686-win7-windows-msvc":
            rewritten.append("--target=i686-win7-windows-gnu")
        else:
            rewritten.append(entry)

    return rewritten


def main() -> None:
    if len(sys.argv) < 2:
        sys.exit("rustc path not provided to wrapper")

    rustc_bin = sys.argv[1]
    args = sys.argv[2:]

    if os.name != "nt":
        args = rewrite_args(args)

    command = [rustc_bin, *args]

    if os.name == "posix":
        os.execv(command[0], command)
    else:
        sys.exit(subprocess.call(command))


if __name__ == "__main__":
    main()
