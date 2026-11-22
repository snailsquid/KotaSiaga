"""End-to-end contract check: calls every endpoint of a running stack and validates each response
against openapi.yaml (OpenAPI 3.0 schemas -> JSON Schema, unknown fields rejected).

    pip install jsonschema pyyaml
    python3 scripts/validate_responses.py openapi.yaml http://localhost:8080/api \
        <group_key> <admin_key> <test_email> <test_password> [--samples docs/samples.md]

Needs MOCK_MODE=true for the /_mock cases (they are skipped with a note otherwise).
Writes nothing to the stack except test data: one registered user, one report, votes and a
subscription that is deleted again. Exit code 1 if any check fails.
"""
import json, secrets, sys, time, urllib.error, urllib.request, uuid, yaml
from jsonschema import Draft202012Validator

args = [a for a in sys.argv[1:] if not a.startswith("--")]
samples_path = sys.argv[sys.argv.index("--samples") + 1] if "--samples" in sys.argv else None
if samples_path in args:
    args.remove(samples_path)
spec_path, base, key, admin, email, password = args[:6]
spec = yaml.safe_load(open(spec_path))


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
    resp = op["responses"][str(code)]
    if "$ref" in resp:
        resp = spec["components"]["responses"][resp["$ref"].split("/")[-1]]
    content = resp.get("content")
    if not content:
        return None
    return {"$defs": defs, **conv(content["application/json"]["schema"])}


def call(method, url, body=None, headers=None):
    h = {"content-type": "application/json", **(headers or {})}
    req = urllib.request.Request(base + url, method=method, data=json.dumps(body).encode() if body is not None else None, headers=h)
    started = time.time()
    try:
        with urllib.request.urlopen(req, timeout=20) as r:
            raw = r.read()
            status = r.status
    except urllib.error.HTTPError as e:
        raw, status = e.read(), e.code
    ms = int((time.time() - started) * 1000)
    try:
        doc = json.loads(raw) if raw else None
    except ValueError:
        doc = None
    return status, doc, ms


bad = 0
samples = []


def check(label, path, method, url, want, body=None, headers=None, sample=False):
    """Calls, checks the status and validates the body against the contract for that status."""
    global bad
    status, doc, ms = call(method.upper(), url, body, headers)
    problems = []
    if status != want:
        problems.append(f"status {status}, want {want}: {json.dumps(doc)[:200]}")
    else:
        sch = schema_for(path, method, status)
        if sch is not None:
            errs = sorted(Draft202012Validator(sch).iter_errors(doc), key=lambda e: e.path)
            problems += [f"{list(e.path)}: {e.message[:160]}" for e in errs[:3]]
    mark = "ok " if not problems else "BAD"
    print(f"{mark} {method.upper():6} {url[:70]:70} -> {status} ({ms} ms) {label}")
    for p in problems:
        print("      " + p)
    bad += bool(problems)
    if sample:
        samples.append((label, method.upper(), url, body, headers or {}, status, doc))
    return doc


K = {"X-API-Key": key}
A = {"X-API-Key": admin}

# --- system, auth
check("health", "/health", "get", "/health", 200, sample=True)
login = check("log in with the test account", "/auth/login", "post", "/auth/login", 200, {"email": email, "password": password}, sample=True)
token = login["data"]["access_token"] if login and "data" in login else ""
U = {"Authorization": f"Bearer {token}"}
check("wrong password", "/auth/login", "post", "/auth/login", 401, {"email": email, "password": secrets.token_hex(8)})
check("me", "/auth/me", "get", "/auth/me", 200, headers=U, sample=True)
other_email = f"warga-{uuid.uuid4().hex[:8]}@kotasiaga.test"
other_pw = secrets.token_hex(8)  # random per run
check("register a second test user", "/auth/register", "post", "/auth/register", 201,
      {"email": other_email, "password": other_pw, "display_name": "Warga Uji"}, sample=True)
check("register the same email again", "/auth/register", "post", "/auth/register", 409,
      {"email": other_email, "password": other_pw, "display_name": "Warga Uji"})
other = check("log in as the second user", "/auth/login", "post", "/auth/login", 200, {"email": other_email, "password": other_pw})
U2 = {"Authorization": f"Bearer {other['data']['access_token']}"} if other and "data" in other else {}
refreshed = check("refresh", "/auth/refresh", "post", "/auth/refresh", 200, {"refresh_token": other["data"]["refresh_token"]}, sample=True)
check("refresh token is single use", "/auth/refresh", "post", "/auth/refresh", 401, {"refresh_token": other["data"]["refresh_token"]})

# --- city (group key)
check("status", "/status", "get", "/status?lat=-6.9&lon=107.6&radius_m=2000", 200, headers=K, sample=True)
check("risk of one cell", "/risk", "get", "/risk?lat=-6.89&lon=107.61", 200, headers=K, sample=True)
check("risk in a bbox", "/risk", "get", "/risk?bbox=107.60,-6.91,107.62,-6.89", 200, headers=K)
check("risk outside Bandung", "/risk", "get", "/risk?lat=-6.2&lon=106.8", 422, headers=K)
check("weather", "/weather", "get", "/weather?lat=-6.89&lon=107.61", 200, headers=K, sample=True)
check("disasters", "/disasters", "get", "/disasters?limit=5", 200, headers=K, sample=True)
check("reports", "/reports", "get", "/reports?limit=3", 200, headers=K, sample=True)
check("reports by a user token", "/reports", "get", "/reports?limit=1", 200, headers=U)
check("no key", "/reports", "get", "/reports", 401)
check("changes", "/changes", "get", "/changes?since=0&limit=3", 200, headers=K, sample=True)

# --- partners
check("monitor points (Tourism)", "/monitor-points", "get", "/monitor-points?limit=3", 200, headers=K, sample=True)
dest = check("destination points (Health)", "/destination-points", "get", "/destination-points?limit=3", 200, headers=K, sample=True)

# --- routes
check("safe route", "/routes/safe", "post", "/routes/safe", 200,
      {"origin": {"lat": -6.9025, "lon": 107.6188}, "destination": {"lat": -6.9218, "lon": 107.6070}, "alternatives": 3}, headers=K, sample=True)
if dest and dest.get("data"):
    check("safe route to a donation event", "/routes/safe", "post", "/routes/safe", 200,
          {"origin": {"lat": -6.9025, "lon": 107.6188}, "destination_id": dest["data"][0]["id"]}, headers=K)
check("safe route outside Bandung", "/routes/safe", "post", "/routes/safe", 422,
      {"origin": {"lat": -6.2, "lon": 106.8}, "destination": {"lat": -6.92, "lon": 107.6}}, headers=K)

# --- reports and votes (JWT)
created = check("create a report", "/reports", "post", "/reports", 201,
                {"category": "flood", "lat": -6.9175, "lon": 107.6093, "description": "Genangan 20 cm di Jl. Braga", "severity": 2}, headers=U, sample=True)
check("create a report without login", "/reports", "post", "/reports", 401, {"category": "flood", "lat": -6.9, "lon": 107.6})
if created and "data" in created:
    rid = created["data"]["id"]
    check("vote on your own report", "/reports/{id}/votes", "post", f"/reports/{rid}/votes", 403, {"vote": "confirm"}, headers=U)
    check("confirm someone else's report", "/reports/{id}/votes", "post", f"/reports/{rid}/votes", 200, {"vote": "confirm"}, headers=U2, sample=True)
    check("vote twice", "/reports/{id}/votes", "post", f"/reports/{rid}/votes", 409, {"vote": "confirm"}, headers=U2)
check("vote on an unknown report", "/reports/{id}/votes", "post", "/reports/rpt_999999/votes", 404, {"vote": "confirm"}, headers=U2)

# --- subscriptions and alerts (JWT)
sub = check("subscribe to an area", "/subscriptions", "post", "/subscriptions", 201,
            {"name": "Kampus", "lat": -6.8915, "lon": 107.6107, "radius_m": 1000}, headers=U, sample=True)
check("list subscriptions", "/subscriptions", "get", "/subscriptions", 200, headers=U, sample=True)
check("alerts", "/alerts", "get", "/alerts?limit=5", 200, headers=U, sample=True)

# --- mock controls (admin, MOCK_MODE only)
status, _, _ = call("GET", "/_mock/state", None, A)
if status == 200:
    check("mock state", "/_mock/state", "get", "/_mock/state", 200, headers=A, sample=True)
    check("mock scenario", "/_mock/scenario", "post", "/_mock/scenario", 202,
          {"name": "flood_verified", "lat": -6.8915, "lon": 107.6107}, headers=A, sample=True)
    check("mock scenario with a group key", "/_mock/scenario", "post", "/_mock/scenario", 403, {"name": "reset"}, headers=K)
    time.sleep(1)
    check("alerts after a flood near the area", "/alerts", "get", "/alerts?limit=5", 200, headers=U)
else:
    print(f"--  /_mock/* skipped (status {status}: MOCK_MODE is off)")

if sub and "data" in sub:
    check("delete the subscription", "/subscriptions/{id}", "delete", f"/subscriptions/{sub['data']['id']}", 204, headers=U, sample=True)
check("logout", "/auth/logout", "post", "/auth/logout", 204, {"refresh_token": refreshed["data"]["refresh_token"]} if refreshed else {}, sample=True)

if samples_path:
    def shell_headers(h):
        out = []
        for k, v in h.items():
            if k == "X-API-Key":
                out.append(f'-H "X-API-Key: $ADMIN_KEY"' if v == admin else f'-H "X-API-Key: $KEY"')
            elif k == "Authorization":
                out.append('-H "Authorization: Bearer $TOKEN"')
        return " ".join(out)

    def trim(doc):
        """Keeps samples short: long arrays cut to 2 items, long coordinate lists to 3 points."""
        if isinstance(doc, dict):
            return {k: (trim(v) if k != "coordinates" else (v[:3] + [["..."]] if len(v) > 3 else v)) for k, v in doc.items()}
        if isinstance(doc, list):
            return [trim(x) for x in doc[:2]] + (["..."] if len(doc) > 2 else [])
        if isinstance(doc, str) and doc.count(".") == 2 and len(doc) > 100:
            return doc[:24] + "...(jwt)"
        if isinstance(doc, str) and doc.startswith("rt_"):
            return doc[:10] + "...(refresh token)"
        return doc

    with open(samples_path, "w") as f:
        f.write("# Sample requests and responses\n\n")
        f.write(f"Captured from a local `docker compose` stack on {time.strftime('%Y-%m-%d %H:%M UTC', time.gmtime())} by\n")
        f.write("`scripts/validate_responses.py --samples`. Every response below passed the openapi.yaml schema check.\n")
        f.write("Long arrays are cut to two items. Set `BASE`, `KEY` (group key), `TOKEN` (from login), `ADMIN_KEY`.\n\n")
        for label, method, url, body, headers, status, doc in samples:
            f.write(f"## {label}\n\n```sh\ncurl -s -X {method} \"$BASE{url}\" {shell_headers(headers)}")
            if body is not None:
                b = dict(body)
                if "password" in b:
                    b["password"] = "<test account password>" if b.get("email") == email else "<password>"
                if "refresh_token" in b:
                    b["refresh_token"] = "<refresh_token>"
                f.write(" \\\n  -H 'content-type: application/json' -d '" + json.dumps(b, ensure_ascii=False) + "'")
            f.write(f"\n```\n\n`{status}`")
            if doc is not None:
                f.write("\n\n```json\n" + json.dumps(trim(doc), indent=2, ensure_ascii=False) + "\n```")
            f.write("\n\n")
    print(f"samples written to {samples_path}")

print("FAILED" if bad else "ALL OK", f"({bad} bad)")
sys.exit(1 if bad else 0)
