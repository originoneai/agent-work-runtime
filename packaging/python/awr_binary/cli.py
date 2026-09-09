import os
from pathlib import Path
import subprocess
import sys


def _run(command):
    suffix = ".exe" if os.name == "nt" else ""
    executable = Path(__file__).parent / "bin" / (command + suffix)
    args = [str(executable), *sys.argv[1:]]
    if os.name != "nt":
        os.execv(str(executable), args)
    try:
        return subprocess.call(args)
    except KeyboardInterrupt:
        return 130


def awr():
    return _run("awr")


def awr_mcp():
    return _run("awr-mcp")
