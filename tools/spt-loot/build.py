"""Свободный лут Лабы с шансами из базы SPT → assets/spt/labs_loose.json.

Источник: sp-tarkov/server (лицензия NCSA), project/assets/database/locations/laboratory/looseLoot.json.
Файл лежит в Git LFS на сервере SPT (lfs.sp-tushonka.com), а не на GitHub: берём указатель
из репозитория и качаем объект через LFS batch API.

    python tools/spt-loot/build.py                 # скачать и собрать
    python tools/spt-loot/build.py looseLoot.json  # собрать из уже скачанного файла

Формат результата:
    {"source": ..., "fetched": "YYYY-MM-DD", "spawnpoint_count": среднее число точек за рейд,
     "points": [{"pos": [x, y, z], "p": шанс, что на месте что-то появится за рейд,
                 "items": [[tpl, шанс предмета за рейд], ...]}]}

У SPT в одном месте часто несколько точек спавна (до 8 в пределах метра) — склеиваем их
в одно место радиусом MERGE_M: шансы событий объединяем как независимые, 1 - Π(1 - p).
Шанс предмета на точке SPT = вероятность точки × доля предмета в её распределении.
"""

import datetime
import json
import math
import os
import sys
import urllib.request

REPO = "sp-tarkov/server"
PATH = "project/assets/database/locations/laboratory/looseLoot.json"
UA = {"User-Agent": "tarkov-helper/0.1"}
MERGE_M = 0.75
# Границы Лабы (x, z): у SPT есть несколько мусорных точек далеко за картой.
BOUNDS = ((-290.0, -70.0), (-455.0, -240.0))
OUT = os.path.join(os.path.dirname(__file__), "..", "..", "assets", "spt", "labs_loose.json")


def http(url, data=None, headers=None):
    req = urllib.request.Request(url, data=data, headers={**UA, **(headers or {})})
    with urllib.request.urlopen(req, timeout=120) as r:
        return r.read()


def download():
    pointer = http(f"https://raw.githubusercontent.com/{REPO}/master/{PATH}").decode()
    fields = dict(line.split(" ", 1) for line in pointer.strip().splitlines())
    oid, size = fields["oid"].removeprefix("sha256:"), int(fields["size"])
    lfs = http(f"https://raw.githubusercontent.com/{REPO}/master/.lfsconfig").decode()
    url = next(l.split("=", 1)[1].strip() for l in lfs.splitlines() if l.strip().startswith("url"))
    body = json.dumps({"operation": "download", "transfers": ["basic"], "objects": [{"oid": oid, "size": size}]})
    lfs_headers = {"Accept": "application/vnd.git-lfs+json", "Content-Type": "application/vnd.git-lfs+json"}
    batch = json.loads(http(url.rstrip("/") + "/objects/batch", body.encode(), lfs_headers))
    obj = batch["objects"][0]
    if obj.get("error"):
        sys.exit(f"LFS: {obj['error']}")
    return json.loads(http(obj["actions"]["download"]["href"]))


def compact(loose):
    """Точки SPT -> места: позиция, шанс что-то найти, шансы предметов за рейд."""
    spots = []  # [pos, [вероятности точек], {tpl: [вероятности]}]
    for pos, p, shares in spawnpoints(loose):
        spot = next(
            (s for s in spots if abs(s[0][1] - pos[1]) < 1.5 and math.dist((s[0][0], s[0][2]), (pos[0], pos[2])) < MERGE_M),
            None,
        )
        if spot is None:
            spot = [pos, [], {}]
            spots.append(spot)
        spot[1].append(p)
        for tpl, share in shares.items():
            spot[2].setdefault(tpl, []).append(p * share)

    def union(ps):
        q = 1.0
        for x in ps:
            q *= 1 - x
        return 1 - q

    points = []
    for pos, ps, items in spots:
        chances = sorted(([tpl, round(union(cs), 6)] for tpl, cs in items.items()), key=lambda x: -x[1])
        points.append({
            "pos": [round(c, 2) for c in pos],
            "p": round(union(ps), 4),
            "items": [c for c in chances if c[1] > 0],
        })
    return points


def spawnpoints(loose):
    """(позиция, вероятность точки, {tpl: доля}) по всем точкам SPT, включая принудительные."""
    for sp in loose["spawnpoints"] + loose.get("spawnpointsForced", []):
        t = sp["template"]
        tpl = {i["_id"]: i["_tpl"] for i in t["Items"]}
        # у «принудительных» точек распределения нет — один предмет (корень шаблона)
        dist = sp.get("itemDistribution") or [
            {"composedKey": {"key": i["_id"]}, "relativeProbability": 1} for i in t["Items"] if not i.get("parentId")
        ]
        total = sum(d["relativeProbability"] for d in dist) or 1
        shares = {}
        for d in dist:
            item = tpl.get(d["composedKey"]["key"])
            if item:
                shares[item] = shares.get(item, 0) + d["relativeProbability"] / total
        pos = t["Position"]
        (x0, x1), (z0, z1) = BOUNDS
        if shares and x0 <= pos["x"] <= x1 and z0 <= pos["z"] <= z1:
            yield (pos["x"], pos["y"], pos["z"]), sp["probability"], shares


def main():
    loose = json.load(open(sys.argv[1], encoding="utf-8")) if len(sys.argv) > 1 else download()
    points = compact(loose)
    out = {
        "source": f"{REPO} {PATH} (NCSA)",
        "fetched": datetime.date.today().isoformat(),
        "spawnpoint_count": round(loose["spawnpointCount"]["mean"], 1),
        "points": points,
    }
    os.makedirs(os.path.dirname(OUT), exist_ok=True)
    with open(OUT, "w", encoding="utf-8") as f:
        json.dump(out, f, separators=(",", ":"))
    print(f"{len(points)} точек, {os.path.getsize(OUT) // 1024} КБ -> {os.path.normpath(OUT)}")


if __name__ == "__main__":
    main()
