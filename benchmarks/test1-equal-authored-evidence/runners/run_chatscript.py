from __future__ import print_function
import argparse
import json
import os
import re
import shutil
import socket
import subprocess
import sys
import tempfile
import time
from pathlib import Path

from common import load_spec, score_rows, token_to_intent, write_json, write_jsonl


def wait_port(host, port, timeout=30.0):
    deadline = time.time() + timeout
    while time.time() < deadline:
        try:
            with socket.create_connection((host, port), timeout=0.5):
                return
        except OSError:
            time.sleep(0.1)
    raise RuntimeError("ChatScript server did not open %s:%s" % (host, port))


def volley(host, port, user, bot, message, timeout=15.0):
    payload = user.encode("utf-8") + b"\0" + bot.encode("utf-8") + b"\0" + message.encode("utf-8") + b"\0"
    chunks = []
    t0 = time.perf_counter()
    with socket.create_connection((host, port), timeout=timeout) as sock:
        sock.settimeout(timeout)
        sock.sendall(payload)
        try:
            sock.shutdown(socket.SHUT_WR)
        except OSError:
            pass
        while True:
            data = sock.recv(65536)
            if not data:
                break
            chunks.append(data)
    ms = (time.perf_counter() - t0) * 1000.0
    raw = b"".join(chunks).replace(b"\0", b"").decode("utf-8", errors="replace").strip()
    return raw, ms


def start_conversation(host, port, user, bot):
    # Official ChatScript protocol requires a null/empty third component for a new user first.
    try:
        volley(host, port, user, bot, "")
    except Exception:
        # Let the actual scored volley surface a useful error if startup produced no body/closed early.
        pass


def extract_chatscript_token(response):
    # ChatScript renders underscores in ordinary output as spaces (for example,
    # BENCH:ACCOUNT_CREATE becomes BENCH:ACCOUNT CREATE). The frozen semantic
    # source is unchanged; this transport adapter canonicalizes only the returned
    # benchmark marker back to its authored token form.
    text = str(response or "").strip()
    if not text.upper().startswith("BENCH:"):
        return None
    body = text[6:].strip().upper()
    if not body or not re.match(r"^[A-Z0-9_ ]+$", body):
        return None
    return "BENCH:" + re.sub(r"[ _]+", "_", body)


def run_pass(host, port, bot, spec, pass_no):
    rows = []
    for idx, case in enumerate(spec["evaluation"]):
        user = "bench_p%d_%04d" % (pass_no, idx + 1)
        start_conversation(host, port, user, bot)
        response, ms = volley(host, port, user, bot, case["text"])
        token = extract_chatscript_token(response)
        predicted = token_to_intent(token, spec)
        rows.append({
            "id": case["id"],
            "track": case["track"],
            "text": case["text"],
            "expected": case["expected"],
            "predicted": predicted,
            "response_token": token,
            "raw_response": response,
            "ms": ms,
        })
    return rows


def dir_bytes(path):
    total = 0
    p = Path(path)
    if not p.exists():
        return None
    for x in p.rglob("*"):
        if x.is_file():
            total += x.stat().st_size
    return total


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--root", required=True, help="Harness root")
    ap.add_argument("--chatscript-root", required=True, help="Official ChatScript 14.1 checkout")
    ap.add_argument("--port", type=int, default=10240)
    args = ap.parse_args()

    root = Path(args.root).resolve()
    cs = Path(args.chatscript_root).resolve()
    spec = load_spec(str(root / "frozen" / "benchmark-spec.json"))
    semantic = root / "frozen" / "bots" / "chatscript" / "benchmark.top"
    control = root / "chatscript" / "control.top"
    filelist = root / "chatscript" / "filesbenchmark.txt"
    results = root / "results"
    results.mkdir(exist_ok=True)

    binary_candidates = [cs / "BINARIES" / "LinuxChatScript64", cs / "BINARIES" / "ChatScript"]
    binary = next((p for p in binary_candidates if p.is_file()), None)
    if binary is None:
        raise RuntimeError("ChatScript Linux binary not found; build official 14.1 standalone first")
    os.chmod(str(binary), os.stat(str(binary)).st_mode | 0o111)

    rawdir = cs / "RAWDATA" / "GVYA_BENCH"
    rawdir.mkdir(parents=True, exist_ok=True)
    shutil.copy2(str(semantic), str(rawdir / "benchmark.top"))
    shutil.copy2(str(control), str(rawdir / "control.top"))
    shutil.copy2(str(filelist), str(rawdir / "filesbenchmark.txt"))

    env = os.environ.copy()
    build_cmd = [str(binary), "local", "build1=RAWDATA/GVYA_BENCH/filesbenchmark.txt"]
    t0 = time.perf_counter()
    build = subprocess.run(build_cmd, cwd=str(cs), env=env, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
    build_ms = (time.perf_counter() - t0) * 1000.0
    (results / "chatscript.build.log").write_text(build.stdout, encoding="utf-8", errors="replace")
    if build.returncode != 0:
        raise RuntimeError("ChatScript build1 failed with code %d; see chatscript.build.log" % build.returncode)

    users = root / ".work" / "chatscript-users"
    logs = root / ".work" / "chatscript-logs"
    tmp = root / ".work" / "chatscript-tmp"
    for p in (users, logs, tmp):
        p.mkdir(parents=True, exist_ok=True)

    server_log = open(str(results / "chatscript.server.log"), "w", encoding="utf-8")
    server_cmd = [
        str(binary),
        "port=%d" % args.port,
        "defaultbot=benchmarkbot",
        "users=%s" % users,
        "logs=%s" % logs,
        "tmp=%s" % tmp,
        "userlogging=none",
        "serverlogging=none",
    ]
    proc = subprocess.Popen(server_cmd, cwd=str(cs), env=env, stdout=server_log, stderr=subprocess.STDOUT, text=True)
    try:
        wait_port("127.0.0.1", args.port, 45.0)
        first = run_pass("127.0.0.1", args.port, "benchmarkbot", spec, 1)
        second = run_pass("127.0.0.1", args.port, "benchmarkbot", spec, 2)
    finally:
        proc.terminate()
        try:
            proc.wait(timeout=10)
        except subprocess.TimeoutExpired:
            proc.kill()
            proc.wait(timeout=5)
        server_log.close()

    p1 = [r["predicted"] for r in first]
    p2 = [r["predicted"] for r in second]
    if p1 != p2:
        for a, b in zip(first, second):
            if a["predicted"] != b["predicted"]:
                raise RuntimeError("Non-deterministic ChatScript result at %s: %r vs %r" % (a["id"], a["predicted"], b["predicted"]))
        raise RuntimeError("Non-deterministic ChatScript predictions")

    summary = score_rows(
        "ChatScript",
        "ChatScript 14.1",
        spec,
        first,
        generated_source_bytes=semantic.stat().st_size,
        extra={
            "implementation_version": "14.1",
            "git_commit": "9f5eec4736ba22bd992a6498c1e0052e2a795125",
            "build_ms": build_ms,
            "semantic_source": "frozen/bots/chatscript/benchmark.top",
            "control_infrastructure_bytes": control.stat().st_size,
            "compiled_build1_bytes": dir_bytes(cs / "TOPIC" / "BUILD1"),
            "session_isolation": "unique ChatScript user per evaluation case; official null-message conversation start is sent and ignored before the scored volley",
            "transport": "official ChatScript TCP protocol over localhost",
        },
    )
    write_jsonl(str(results / "chatscript.raw.jsonl"), first)
    write_json(str(results / "chatscript.summary.json"), summary)
    print(json.dumps(summary, indent=2, sort_keys=True))


if __name__ == "__main__":
    main()
