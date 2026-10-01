#!/usr/bin/env python3
"""Check two actual Herdr binaries using disposable, isolated named sessions.

Usage: python3 scripts/check_fork_handoff.py OLD_BINARY NEW_BINARY
"""

import json
import os
from pathlib import Path
import shutil
import socket
import subprocess
import sys
import tempfile
import time


def request(path, method, params=None, allow_error=False):
    with socket.socket(socket.AF_UNIX) as stream:
        stream.settimeout(30)
        stream.connect(str(path))
        stream.sendall((json.dumps({"id": "fork-check", "method": method, "params": params or {}}) + "\n").encode())
        response = json.loads(stream.makefile("rb").readline())
    if not allow_error:
        assert "result" in response, response
    return response


def wait_until(check, timeout=15):
    deadline = time.monotonic() + timeout
    last_error = None
    while time.monotonic() < deadline:
        try:
            result = check()
            if result:
                return result
        except (OSError, ValueError, AssertionError) as error:
            last_error = error
        time.sleep(0.05)
    raise AssertionError(f"timed out: {last_error}")


def pane_text(path, pane):
    return request(path, "pane.read", {"pane_id": pane, "source": "visible", "format": "text"})["result"]["read"]["text"]


def main():
    old, new = (str(Path(arg).resolve(strict=True)) for arg in sys.argv[1:])
    root = Path(tempfile.mkdtemp(prefix="herdr-fork-handoff-", dir="/var/tmp"))
    os.chmod(root, 0o700)
    env = {key: value for key, value in os.environ.items() if not key.startswith("HERDR_")}
    env.update(HOME=str(root), XDG_CONFIG_HOME=str(root / "config"), XDG_STATE_HOME=str(root / "state"),
               XDG_RUNTIME_DIR=str(root / "runtime"), SHELL="/bin/sh", TERM="xterm-256color")
    (root / "runtime").mkdir()
    config = root / "config.toml"
    config.write_text('onboarding = false\n[ui]\nconfirm_close_running = true\npane_border_shows_osc_title = true\nrounded_pane_borders = true\n')
    env["HERDR_CONFIG_PATH"] = str(config)
    source_exe = root / "herdr-installed"
    shutil.copy2(old, source_exe)
    processes = []
    sockets = []
    logs = []
    try:
        for name in ("migration", "untouched"):
            api = root / f"{name}.sock"
            server_env = dict(env, HERDR_SESSION=f"fork-check-{name}", HERDR_SOCKET_PATH=str(api),
                              HERDR_CLIENT_SOCKET_PATH=str(root / f"{name}-client.sock"))
            log = (root / f"{name}.log").open("wb")
            logs.append(log)
            processes.append(subprocess.Popen([str(source_exe), "server"], env=server_env, stdin=subprocess.DEVNULL,
                                               stdout=log, stderr=log))
            sockets.append(api)
            wait_until(lambda: api.exists() and request(api, "ping"))
        api, other = sockets
        original_ping = request(api, "ping")["result"]
        other_ping = request(other, "ping")["result"]
        created = request(api, "workspace.create", {"cwd": str(root), "label": "handoff-probe", "focus": True})
        pane = created["result"]["workspace"]["workspace_id"]
        panes = request(api, "pane.list")["result"]["panes"]
        pane = next(item["pane_id"] for item in panes if item["workspace_id"] == pane)
        wait_until(lambda: request(api, "pane.process_info", {"pane_id": pane})["result"]["process_info"].get("shell_pid"))
        request(api, "pane.rename", {"pane_id": pane, "label": "persistent-probe"})
        marker = root / "probe.json"
        program = root / "probe.py"
        program.write_text('import os, json, sys\nfrom pathlib import Path\n'
                           f'Path({str(marker)!r}).write_text(json.dumps({{"pid": os.getpid(), "pane": os.environ.get("HERDR_PANE_ID")}}))\n'
                           'print("probe-ready", flush=True)\n'
                           'for line in sys.stdin: print("reply:" + line.strip(), flush=True)\n')
        request(api, "pane.send_text", {"pane_id": pane, "text": f"python3 {program}\n"})
        wait_until(lambda: marker.exists() and "probe-ready" in pane_text(api, pane))
        before = json.loads(marker.read_text())
        before_info = request(api, "pane.process_info", {"pane_id": pane})["result"]["process_info"]
        print(f"source={original_ping.get('version')} probe_pid={before['pid']} pane={pane}", flush=True)

        # Reproduce an atomic installation replacing the running executable's inode.
        replacement = root / "herdr-replacement"
        shutil.copy2(new, replacement)
        os.replace(replacement, source_exe)

        # A rejected import must leave the source PTY and server usable.
        rejected = request(api, "server.live_handoff", {"import_exe": new, "expected_version": "invalid-fork-check"}, allow_error=True)
        assert "error" in rejected, rejected
        wait_until(lambda: request(api, "ping")["result"].get("version") == original_ping.get("version"))
        request(api, "pane.send_text", {"pane_id": pane, "text": "rollback-ok\n"})
        wait_until(lambda: "reply:rollback-ok" in pane_text(api, pane))

        version = subprocess.check_output([new, "--version"], env=env, text=True).strip().split()[-1]
        request(api, "server.live_handoff", {"import_exe": new, "expected_version": version})
        wait_until(lambda: request(api, "ping")["result"].get("version") == version)
        processes[0].wait(timeout=15)
        os.kill(before["pid"], 0)
        assert json.loads(marker.read_text()) == before
        after_info = request(api, "pane.process_info", {"pane_id": pane})["result"]["process_info"]
        assert after_info["shell_pid"] == before_info["shell_pid"], (before_info, after_info)
        assert after_info["foreground_process_group_id"] == before_info["foreground_process_group_id"]
        request(api, "pane.send_text", {"pane_id": pane, "text": "migration-ok\n"})
        wait_until(lambda: "reply:migration-ok" in pane_text(api, pane))
        restored = request(api, "pane.get", {"pane_id": pane})["result"]["pane"]
        assert restored["label"] == "persistent-probe"
        assert request(other, "ping")["result"].get("version") == other_ping.get("version")
        assert processes[1].poll() is None
        print(f"PASS: {original_ping.get('version')} -> {version}; process and shell PIDs, PTY I/O, label, rollback, and session isolation preserved after replacing the source executable", flush=True)
    finally:
        for api in sockets:
            try:
                request(api, "server.stop")
            except (OSError, ValueError, AssertionError):
                pass
        for process in processes:
            try:
                process.wait(timeout=10)
            except subprocess.TimeoutExpired:
                process.terminate()
                process.wait(timeout=5)
        for log in logs:
            log.close()
        # Preserve diagnostic logs on failure; remove only this check's directory on success.
        if sys.exc_info()[0] is None:
            shutil.rmtree(root)
        else:
            print(f"diagnostics: {root}", file=sys.stderr)


if __name__ == "__main__":
    if len(sys.argv) != 3:
        raise SystemExit(__doc__)
    main()
