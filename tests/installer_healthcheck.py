#!/usr/bin/env python3
"""Exercise the standalone installer's real health-check client against fake sockets."""
from pathlib import Path
import socket
import subprocess
import sys
import tempfile
import threading
import time
import unittest


class InstallerHealthCheck(unittest.TestCase):
    def probe(self, delay=0, handshake=b"OK protocol=2 daemon=test\n", timeout=30.0, fragment_delay=0):
        source = (Path(__file__).resolve().parents[1] / "install.sh").read_text()
        code = source.split("ping_control_service() {", 1)[1].split("<<'PY'\n", 1)[1].split("\nPY\n", 1)[0]
        with tempfile.TemporaryDirectory(prefix="asense-health-") as directory:
            path = str(Path(directory) / "control.sock")
            code = code.replace('"/run/asense-control.sock"', repr(path)).replace("+ 30.0", f"+ {timeout!r}")
            with socket.socket(socket.AF_UNIX) as listener:
                listener.bind(path)
                listener.listen(1)
                listener.settimeout(10)
                errors = []
                def server():
                    try:
                        with listener.accept()[0] as client:
                            client.settimeout(10)
                            with client.makefile("rb") as reader:
                                assert reader.readline() == b"HELLO 2\n"
                                time.sleep(delay)
                                if fragment_delay:
                                    for byte in handshake:
                                        client.sendall(bytes([byte]))
                                        time.sleep(fragment_delay)
                                else:
                                    client.sendall(handshake)
                                if handshake == b"OK protocol=2 daemon=test\n":
                                    assert reader.readline() == b"PING\n"
                                    client.sendall(b"OK ready\n")
                    except BrokenPipeError:
                        pass  # Expected after the bounded-deadline client exits.
                    except Exception as error:
                        errors.append(error)
                worker = threading.Thread(target=server)
                worker.start()
                result = subprocess.run([sys.executable, "-c", code], capture_output=True, text=True, timeout=10)
                worker.join(timeout=10)
                self.assertFalse(worker.is_alive())
                self.assertFalse(errors, errors)
                return result

    def test_slow_start_exceeding_old_five_second_timeout_succeeds(self):
        result = self.probe(delay=5.2)
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_bad_handshake_is_rejected(self):
        result = self.probe(handshake=b"OK protocol=1 daemon=test\n")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("unexpected ASense protocol handshake", result.stderr)

    def test_fragmented_reply_cannot_extend_absolute_deadline(self):
        result = self.probe(fragment_delay=0.03, timeout=0.06)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("control health check failed", result.stderr)

    def test_deadline_reports_clear_error_without_traceback(self):
        result = self.probe(delay=0.2, timeout=0.05)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("control health check failed", result.stderr)
        self.assertNotIn("Traceback", result.stderr)


if __name__ == "__main__":
    unittest.main()
