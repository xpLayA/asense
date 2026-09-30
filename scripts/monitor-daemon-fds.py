#!/usr/bin/env python3
"""Record ASense descriptor counts during an active/idle GPU soak test."""
import argparse
import csv
import datetime
import os
from pathlib import Path
import subprocess
import sys
import time


def daemon_pid():
    value = subprocess.check_output(
        ["systemctl", "show", "asense.service", "--property=MainPID", "--value"],
        text=True,
    ).strip()
    pid = int(value)
    if pid <= 0:
        raise RuntimeError("asense.service is not running")
    return pid


def descriptor_counts(pid):
    directory = Path(f"/proc/{pid}/fd")
    descriptors = list(directory.iterdir())
    nvidia = 0
    for descriptor in descriptors:
        try:
            nvidia += os.readlink(descriptor).startswith("/dev/nvidia")
        except FileNotFoundError:
            pass  # A descriptor can close between enumeration and readlink.
    return len(descriptors), nvidia


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--duration", type=int, default=3600, help="seconds (default: 3600)")
    parser.add_argument("--interval", type=int, default=10, help="seconds (1–60; default: 10)")
    args = parser.parse_args()
    if args.duration < 0 or not 1 <= args.interval <= 60:
        parser.error("duration must be nonnegative and interval must be 1–60")
    pid = daemon_pid()
    try:
        descriptor_counts(pid)
    except PermissionError:
        raise RuntimeError("run with sudo to read the root daemon's descriptors") from None
    limits = Path(f"/proc/{pid}/limits").read_text()
    for line in limits.splitlines():
        if line.startswith("Max open files"):
            print(f"PID {pid}: {line}", file=sys.stderr)
    writer = csv.writer(sys.stdout)
    writer.writerow(["utc", "pid", "descriptors", "nvidia_descriptors"])
    deadline = time.monotonic() + args.duration
    counts = []
    while True:
        if daemon_pid() != pid:
            raise RuntimeError("daemon restarted; rerun the monitor for the new process")
        total, nvidia = descriptor_counts(pid)
        counts.append(total)
        writer.writerow([datetime.datetime.now(datetime.timezone.utc).isoformat(), pid, total, nvidia])
        sys.stdout.flush()
        remaining = deadline - time.monotonic()
        if remaining <= 0:
            break
        time.sleep(min(args.interval, remaining))
    print(
        f"Descriptors: start={counts[0]}, end={counts[-1]}, min={min(counts)}, max={max(counts)}. "
        "Compare active and idle periods; this does not automatically certify stability.",
        file=sys.stderr,
    )


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError, RuntimeError, subprocess.CalledProcessError) as error:
        sys.exit(f"asense-fd-monitor: {error}")
