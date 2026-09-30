#!/usr/bin/env python3
"""Read-only CPU sampling for comparing ASense fan modes; never changes cooling."""
import argparse
import csv
import datetime
import os
from pathlib import Path
import subprocess
import sys
import time


def daemon_pid():
    return int(subprocess.check_output(
        ["systemctl", "show", "asense.service", "--property=MainPID", "--value"], text=True
    ).strip())


def sample(pid):
    fields = Path(f"/proc/{pid}/stat").read_text().rsplit(")", 1)[1].split()
    return int(fields[11]), int(fields[12]), int(fields[19])


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--duration", type=int, default=120)
    parser.add_argument("--interval", type=int, default=1)
    parser.add_argument("--label", default="auto-curve")
    args = parser.parse_args()
    if args.duration < 1 or not 1 <= args.interval <= 60:
        parser.error("duration must be positive and interval must be 1–60")
    pid = daemon_pid()
    if pid <= 0:
        raise RuntimeError("asense.service is not running")
    before = sample(pid)
    start = previous_time = time.monotonic()
    deadline = start + args.duration
    hz = os.sysconf("SC_CLK_TCK")
    writer = csv.writer(sys.stdout)
    writer.writerow(["utc", "label", "pid", "user_cpu_percent", "kernel_cpu_percent"])
    while time.monotonic() < deadline:
        time.sleep(min(args.interval, max(0, deadline - time.monotonic())))
        after = sample(pid)
        now = time.monotonic()
        if after[2] != before[2] or daemon_pid() != pid:
            raise RuntimeError("daemon restarted; rerun the monitor")
        elapsed = now - previous_time
        writer.writerow([datetime.datetime.now(datetime.timezone.utc).isoformat(), args.label, pid,
                         round((after[0] - before[0]) / hz / elapsed * 100, 3),
                         round((after[1] - before[1]) / hz / elapsed * 100, 3)])
        sys.stdout.flush()
        before, previous_time = after, now


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError, RuntimeError, subprocess.CalledProcessError) as error:
        sys.exit(f"asense-latency-monitor: {error}")
