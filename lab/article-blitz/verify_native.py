#!/usr/bin/env python3
"""Run only the isolated fixture binary; capture and scroll its native texture."""
import hashlib, json, os, pathlib, shutil, socket, subprocess, sys, time, urllib.parse, urllib.request
ROOT = pathlib.Path(__file__).resolve().parents[2]
binary = pathlib.Path(sys.argv[1]).resolve() if len(sys.argv) > 1 else ROOT / "target-blitz/debug/examples/native_preview"
output = ROOT / "lab/article-blitz/evidence"
output.mkdir(parents=True, exist_ok=True)
with socket.socket() as sock:
    sock.bind(("127.0.0.1", 0))
    port = sock.getsockname()[1]
def request(path, **params):
    url = f"http://127.0.0.1:{port}{path}?" + urllib.parse.urlencode(params)
    with urllib.request.urlopen(url, timeout=10) as response:
        return json.load(response)
env = dict(os.environ, MAKEPAD_REMOTE=str(port), MAKEPAD_HIDE_WINDOWS="1", MAKEPAD_NO_FOCUS="1")
with (output / "native-run.log").open("w") as log:
    app = subprocess.Popen([str(binary)], cwd=ROOT, env=env, stdout=log, stderr=subprocess.STDOUT)
    try:
        for _ in range(100):
            if app.poll() is not None:
                raise RuntimeError("Fixture app exited during startup")
            try:
                status = request("/s")
                assert status["pid"] == app.pid, "Bridge belongs to another process"
                if status["w"]:
                    rows = request("/snap")["s"]
                    if any("Offline fixture" in row.get("t", "") for row in rows):
                        break
            except (OSError, ValueError):
                pass
            time.sleep(.2)
        else:
            raise RuntimeError("Fixture did not become ready")
        request("/m", k="move", x=1, y=1, wait=1)
        time.sleep(.3)
        top = output / "native-top.png"
        shutil.copyfile(request("/g")["png"], top)
        request("/m", k="scroll", x=200, y=400, dy=1600, wait=1)
        time.sleep(.3)
        bottom = output / "native-bottom.png"
        shutil.copyfile(request("/g")["png"], bottom)
        hashes = {p.name: hashlib.sha256(p.read_bytes()).hexdigest() for p in (top,bottom)}
        assert hashes[top.name] != hashes[bottom.name], "Native scrolling did not change pixels"
        ocr = ROOT / "target-blitz/article-ocr"
        if not ocr.exists():
            subprocess.run(["swiftc", str(ROOT / "tools/wechat-ux/live/ocr.swift"), "-o", str(ocr)], check=True)
        top_text = " ".join(row["text"] for row in json.loads(subprocess.check_output([str(ocr), str(top)], text=True)))
        bottom_text = " ".join(row["text"] for row in json.loads(subprocess.check_output([str(ocr), str(bottom)], text=True)))
        assert "把周末还给山野" in top_text, top_text
        assert "呈现方式" in bottom_text and "PingFang" in bottom_text, bottom_text
        report = {"passed": True, "ocr": {"top_contains_chinese_title": True, "bottom_contains_table": True}, "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(), "window": status["w"], "screenshots": hashes, "checks": ["isolated fixture process and owned loopback bridge", "native Makepad widget ready", "Blitz RGBA uploaded as native texture", "native scroll changes displayed article pixels"], "personal_accounts_used": False}
        (output / "native-validation.json").write_text(json.dumps(report, indent=2))
        print(json.dumps(report))
    finally:
        if app.poll() is None:
            try:
                assert request("/s")["pid"] == app.pid
                request("/gq")
            except OSError:
                app.terminate()
            app.wait(timeout=15)
