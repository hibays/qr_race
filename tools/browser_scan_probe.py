"""Drive the scan probe page through Edge CDP with a fake camera.

Start Edge with the fake-camera flags (see docs/WASM.md), then run this script.
It polls the page title and window.qrProbeState until a hit, an error, or a
timeout. EDGE_DEBUG_PORT selects the debugging port (default 9222).
"""

import json
import os
import sys
import time
import urllib.request

import websocket

DEBUG = f"http://127.0.0.1:{os.environ.get('EDGE_DEBUG_PORT', '9222')}"
PAGE = os.environ.get("PROBE_URL", "http://127.0.0.1:18450/scan_probe.html")


def request_json(path, method="GET"):
    request = urllib.request.Request(DEBUG + path, method=method)
    with urllib.request.urlopen(request, timeout=5) as response:
        return json.loads(response.read())


for _ in range(60):
    try:
        request_json("/json/version")
        break
    except Exception:
        time.sleep(0.5)
else:
    print("edge CDP endpoint never came up")
    sys.exit(2)

target = request_json("/json/new?" + PAGE, method="PUT")
connection = websocket.create_connection(
    target["webSocketDebuggerUrl"], timeout=5, suppress_origin=True
)


def evaluate(expression, request_id):
    connection.send(
        json.dumps(
            {
                "id": request_id,
                "method": "Runtime.evaluate",
                "params": {"expression": expression, "returnByValue": True},
            }
        )
    )
    while True:
        message = json.loads(connection.recv())
        if message.get("id") == request_id:
            return message


expression = (
    "JSON.stringify({"
    "title: document.title,"
    "out: document.getElementById('out')?.textContent,"
    "state: window.qrProbeState"
    "})"
)

request_id = 1
for _ in range(80):
    time.sleep(0.5)
    try:
        response = evaluate(expression, request_id)
    except Exception as error:
        print("evaluate failed:", error)
        continue
    request_id += 1
    value = response["result"]["result"].get("value")
    if not value:
        continue
    snapshot = json.loads(value)
    print(snapshot)
    title = snapshot["title"] or ""
    if title.startswith("HIT:"):
        connection.close()
        sys.exit(0)
    if title.startswith("ERROR:"):
        connection.close()
        sys.exit(1)

print("timed out waiting for a hit")
connection.close()
sys.exit(2)
