"""Contract check for the Layanan Integrasi: calls a running integration-service and validates each
response against services/integration-service/openapi.yaml.

    pip install jsonschema pyyaml
    python3 scripts/validate_integration.py services/integration-service/openapi.yaml \
        http://localhost:8090/api <API_KEYS key of the integration service>
"""
import json, sys, urllib.request, yaml
from jsonschema import Draft202012Validator
spec = yaml.safe_load(open(sys.argv[1])); base = sys.argv[2]; key = sys.argv[3]
def conv(s):
    if isinstance(s, dict):
        s = {k: conv(v) for k, v in s.items() if k != "example"}
        if "$ref" in s: return {"$ref": "#/$defs/" + s["$ref"].split("/")[-1]}
        if s.pop("nullable", False) and "type" in s:
            s["type"] = [s["type"], "null"]
            if "enum" in s: s["enum"] = s["enum"] + [None]
        return s
    if isinstance(s, list): return [conv(x) for x in s]
    return s
defs = {k: conv(v) for k, v in spec["components"]["schemas"].items()}
resp = {k: conv(v) for k, v in spec["components"]["responses"].items()}
def schema_for(path, method, status):
    r = spec["paths"][path][method]["responses"][str(status)]
    if "$ref" in r: r = spec["components"]["responses"][r["$ref"].split("/")[-1]]
    return conv(r["content"]["application/json"]["schema"])
def call(method, path, body=None, auth=True):
    req = urllib.request.Request(base + path, method=method, data=json.dumps(body).encode() if body else None)
    req.add_header("content-type", "application/json")
    if auth: req.add_header("X-API-Key", key)
    try:
        with urllib.request.urlopen(req) as r: return r.status, json.load(r)
    except urllib.error.HTTPError as e: return e.code, json.load(e)
bad = 0
for method, spath, path, body, auth, want in [
    ("get", "/health", "/health", None, False, 200),
    ("get", "/integration/status", "/integration/status", None, False, 200),
    ("get", "/monitor-points", "/monitor-points?limit=200", None, True, 200),
    ("get", "/monitor-points", "/monitor-points?risk_level=safe&category=nature", None, True, 200),
    ("get", "/destination-points", "/destination-points?limit=200", None, True, 200),
    ("get", "/monitor-points", "/monitor-points", None, False, 401),
    ("get", "/monitor-points", "/monitor-points?limit=0", None, True, 422),
    ("post", "/routes/safe", "/routes/safe", {"origin": {"lat": -6.9, "lon": 107.6}, "destination_id": "dnr_evt_1"}, True, 200),
    ("post", "/routes/safe", "/routes/safe", {"origin": {"lat": -6.9, "lon": 107.6}, "destination_id": "dnr_evt_404"}, True, 404),
    ("post", "/routes/safe", "/routes/safe", {"origin": {"lat": -6.9, "lon": 107.6}}, True, 422),
]:
    status, doc = call(method.upper(), path, body, auth)
    errs = []
    if status != want: errs.append(f"status {status} != {want}")
    else:
        sch = dict(schema_for(spath, method, status)); sch["$defs"] = defs
        errs += [e.message for e in Draft202012Validator(sch).iter_errors(doc)][:3]
    bad += bool(errs)
    print(("ok " if not errs else "BAD"), method.upper().ljust(5), path.ljust(52), "->", status, "; ".join(errs))
print("ALL OK" if not bad else f"{bad} BAD"); sys.exit(1 if bad else 0)
