#!/usr/bin/env python3
"""Редактор зон Лабы: http://localhost:8790
Отдаёт подложки из кэша приложения, точки лута/двери из дампа и сохраняет
зоны прямо в assets/labs_rooms.json."""
import json, os, re, sys
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, "..", ".."))
ROOMS = os.path.join(ROOT, "assets", "labs_rooms.json")
DATA = os.path.expanduser("~/Library/Application Support/tarkov-helper")
MAPS = os.path.join(DATA, "maps")
FLOORS = ["technical", "1st", "2nd"]


def floor_of(p):
    if p["y"] < -0.9:
        return 0
    if p["y"] >= 3 and -271 <= p["x"] <= -101 and -422 <= p["z"] <= -270:
        return 2
    return 1


def meta():
    files = {}
    rng = None
    for f in os.listdir(MAPS):
        m = re.match(r"labs_(\w+?)_z(\d+)_(\d+)_(\d+)_(\d+)x(\d+)\.png$", f)
        if m:
            files[m.group(1)] = f
            rng = dict(zoom=int(m.group(2)), tx0=int(m.group(3)), ty0=int(m.group(4)), w=int(m.group(5)), h=int(m.group(6)))
    mode = "pve" if os.path.exists(os.path.join(DATA, "pve", "maps.json")) else "regular"
    maps = json.load(open(os.path.join(DATA, mode, "maps.json")))["data"]["maps"]
    ru = json.load(open(os.path.join(DATA, mode, "items_ru.json")))["data"]
    lab = next(v for v in maps.values() if v["normalizedName"] == "the-lab")
    name = lambda i: ru.get(f"{i} ShortName") or i
    locks = [dict(x=l["position"]["x"], z=l["position"]["z"], floor=floor_of(l["position"]), key=l["key"], name=name(l["key"]))
             for l in lab["locks"] if l.get("key")]
    loot = [dict(x=p["position"]["x"], z=p["position"]["z"], floor=floor_of(p["position"])) for p in lab["lootLoose"]]
    keys = {}
    for l in locks:
        keys[l["key"]] = ru.get(f"{l['key']} Name") or l["name"]
    return dict(range=rng, images=[files.get(f) for f in FLOORS], locks=locks, loot=loot, keys=keys)


class H(SimpleHTTPRequestHandler):
    def __init__(self, *a, **kw):
        super().__init__(*a, directory=HERE, **kw)

    def _json(self, obj, code=200):
        body = json.dumps(obj, ensure_ascii=False).encode()
        self.send_response(code)
        self.send_header("Content-Type", "application/json; charset=utf-8")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def do_GET(self):
        if self.path == "/api/meta":
            return self._json(meta())
        if self.path == "/api/rooms":
            return self._json(json.load(open(ROOMS)))
        if self.path.startswith("/maps/"):
            f = os.path.basename(self.path)
            p = os.path.join(MAPS, f)
            if not os.path.isfile(p):
                return self.send_error(404)
            data = open(p, "rb").read()
            self.send_response(200)
            self.send_header("Content-Type", "image/png")
            self.send_header("Content-Length", str(len(data)))
            self.send_header("Cache-Control", "max-age=3600")
            self.end_headers()
            return self.wfile.write(data)
        return super().do_GET()

    def do_PUT(self):
        if self.path != "/api/rooms":
            return self.send_error(404)
        rooms = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
        assert isinstance(rooms, list)
        tmp = ROOMS + ".tmp"
        with open(tmp, "w") as f:
            f.write("[\n" + ",\n".join("  " + json.dumps(r, ensure_ascii=False) for r in rooms) + "\n]\n")
        os.replace(tmp, ROOMS)
        self._json({"ok": True, "count": len(rooms)})

    def log_message(self, *a):
        pass


if __name__ == "__main__":
    port = int(sys.argv[1]) if len(sys.argv) > 1 else 8790
    print(f"Редактор зон: http://localhost:{port}", flush=True)
    ThreadingHTTPServer(("127.0.0.1", port), H).serve_forever()
