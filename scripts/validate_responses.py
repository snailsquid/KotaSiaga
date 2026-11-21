"""Validate live responses against openapi.yaml 200/201/202 schemas (OpenAPI 3.0 -> JSON Schema)."""
import json, sys, urllib.error, urllib.request, yaml
from jsonschema import Draft202012Validator

spec = yaml.safe_load(open(sys.argv[1]))
base, key, admin = sys.argv[2], sys.argv[3], sys.argv[4]

def conv(s):
    if isinstance(s, dict):
        s = {k: conv(v) for k, v in s.items() if k not in ("example",)}
        if "$ref" in s:
            return {"$ref": "#/$defs/" + s["$ref"].split("/")[-1]}
        if s.pop("nullable", False) and "type" in s:
            s["type"] = [s["type"], "null"]
        if s.get("type") == "object" and "properties" in s:
            s.setdefault("additionalProperties", False)  # catch fields the contract does not have
        return s
    if isinstance(s, list):
        return [conv(x) for x in s]
    return s

defs = {k: conv(v) for k, v in spec["components"]["schemas"].items()}

def schema_for(path, method, code):
    op = spec["paths"][path][method]
    sch = op["responses"][code]["content"]["application/json"]["schema"]
    return {"$defs": defs, **conv(sch)}

def call(method, url, body=None, k=key):
    req = urllib.request.Request(base + url, method=method.upper(), data=json.dumps(body).encode() if body else None,
                                 headers={"X-API-Key": k, "content-type": "application/json"})
    try:
        with urllib.request.urlopen(req) as r:
            return r.status, json.load(r)
    except urllib.error.HTTPError as e:  # non-2xx: report it, do not crash the run
        try:
            doc = json.load(e)
        except ValueError:
            doc = {}
        return e.code, doc

cases = [
    ("/health", "get", "/health", None, key),
    ("/status", "get", "/status?lat=-6.9&lon=107.6&radius_m=2000", None, key),
    ("/risk", "get", "/risk?lat=-6.9&lon=107.6", None, key),
    ("/risk", "get", "/risk?bbox=107.60,-6.91,107.62,-6.89", None, key),
    ("/weather", "get", "/weather?lat=-6.9&lon=107.6", None, key),
    ("/disasters", "get", "/disasters", None, key),
    ("/reports", "get", "/reports?limit=200", None, key),
    ("/changes", "get", "/changes?since=0&limit=200", None, key),
    ("/_mock/state", "get", "/_mock/state", None, admin),
    ("/_mock/scenario", "post", "/_mock/scenario", {"name": "disaster_signal"}, admin),
]
bad = 0
for path, method, url, body, k in cases:
    status, doc = call(method, url, body, k)
    if not 200 <= status < 300:
        bad += 1
        err = doc.get("error", {}) if isinstance(doc, dict) else {}
        print(f"BAD {method.upper():4} {url} -> {status} {err.get('code', '')}: {err.get('message', '')}")
        continue
    errs = list(Draft202012Validator(schema_for(path, method, str(status))).iter_errors(doc))
    n = len(doc["data"]) if isinstance(doc.get("data"), list) else 1
    print(f"{'OK ' if not errs else 'BAD'} {method.upper():4} {url} -> {status} ({n} item(s))")
    for e in errs[:3]:
        bad += 1
        print("    ", list(e.absolute_path), e.message[:160])
sys.exit(1 if bad else 0)
