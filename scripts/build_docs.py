"""Builds the shareable documents from the repo sources:

- README.pdf                    README.md with docs/samples.md inlined as Appendix A
- docs/api/KotaSiaga-API.html   standalone API reference rendered from openapi.yaml (no JS, no CDN)
- docs/api/KotaSiaga-API.pdf    the same reference as PDF (for Drive / the API Registry)

    pip install markdown pyyaml playwright && python -m playwright install chromium
    python3 scripts/build_docs.py
"""
import html, pathlib, re, sys
import markdown, yaml
from playwright.sync_api import sync_playwright

ROOT = pathlib.Path(__file__).resolve().parent.parent
CSS = """
@page { size: A4; margin: 16mm 14mm; }
body { font-family: -apple-system, 'Segoe UI', Roboto, Helvetica, Arial, sans-serif; font-size: 10.5pt;
       line-height: 1.45; color: #1f2328; max-width: 980px; margin: 0 auto; padding: 0 12px; }
h1 { font-size: 20pt; border-bottom: 2px solid #d0d7de; padding-bottom: 4px; }
h2 { font-size: 15pt; border-bottom: 1px solid #d0d7de; padding-bottom: 3px; margin-top: 26px; }
h3 { font-size: 12pt; margin-top: 18px; }
h4 { font-size: 10.5pt; margin: 12px 0 4px; }
code { font-family: 'SF Mono', Menlo, Consolas, monospace; font-size: 9pt; background: #f6f8fa;
       padding: 1px 4px; border-radius: 4px; }
pre { background: #f6f8fa; border: 1px solid #d0d7de; border-radius: 6px; padding: 8px 10px;
      font-size: 8.3pt; line-height: 1.35; white-space: pre-wrap; word-break: break-word; }
pre code { background: none; padding: 0; font-size: inherit; }
table { border-collapse: collapse; width: 100%; margin: 8px 0; font-size: 9pt; }
th, td { border: 1px solid #d0d7de; padding: 4px 6px; vertical-align: top; text-align: left; }
th { background: #f6f8fa; }
a { color: #0969da; text-decoration: none; }
h2, h3, h4 { break-after: avoid; } pre, tr { break-inside: avoid; }
.op { border: 1px solid #d0d7de; border-radius: 6px; padding: 6px 12px 10px; margin: 14px 0; break-inside: avoid-page; }
.m { display: inline-block; min-width: 54px; text-align: center; color: #fff; border-radius: 4px;
     font-weight: 600; font-size: 9pt; padding: 1px 6px; margin-right: 6px; }
.get { background: #1f6feb; } .post { background: #1a7f37; } .delete { background: #cf222e; }
.muted { color: #57606a; } .auth { font-size: 9pt; color: #57606a; }
"""


def page(title, body):
    return f"<!doctype html><html lang='en'><head><meta charset='utf-8'><title>{html.escape(title)}</title><style>{CSS}</style></head><body>{body}</body></html>"


def to_pdf(html_path, pdf_path, title):
    with sync_playwright() as p:
        b = p.chromium.launch()
        pg = b.new_page()
        pg.goto(html_path.as_uri())
        pg.pdf(path=str(pdf_path), format="A4", print_background=True, display_header_footer=True,
               header_template="<span></span>",
               footer_template=f"<div style='font-size:8px;width:100%;text-align:center;color:#888'>{html.escape(title)} · <span class='pageNumber'></span>/<span class='totalPages'></span></div>",
               margin={"top": "14mm", "bottom": "16mm", "left": "12mm", "right": "12mm"})
        b.close()


# ---------- README.pdf ----------

def build_readme():
    readme = (ROOT / "README.md").read_text()
    samples = (ROOT / "docs/samples.md").read_text()
    samples = samples.split("\n", 1)[1]  # drop its H1
    samples = re.sub(r"^## ", "### ", samples, flags=re.M)
    readme = re.sub(r"<!-- samples:start -->.*<!-- samples:end -->", lambda _: samples, readme, flags=re.S)
    # Anchor links that only make sense on GitHub keep working in the PDF via the toc extension ids.
    body = markdown.markdown(readme, extensions=["tables", "fenced_code", "toc", "sane_lists"])
    out = ROOT / "docs/README.print.html"
    out.write_text(page("KotaSiaga README", body))
    to_pdf(out, ROOT / "README.pdf", "KotaSiaga README")
    if "--keep-html" not in sys.argv:
        out.unlink()
    print("README.pdf")


# ---------- API reference ----------

def build_api():
    spec = yaml.safe_load((ROOT / "openapi.yaml").read_text())
    comps = spec["components"]
    md = lambda s: markdown.markdown(s or "", extensions=["tables", "fenced_code"])

    def deref(s):
        while isinstance(s, dict) and "$ref" in s:
            s = comps["schemas"][s["$ref"].split("/")[-1]]
        return s

    def type_of(s):
        if "$ref" in s:
            return f"<a href='#schema-{s['$ref'].split('/')[-1]}'>{s['$ref'].split('/')[-1]}</a>"
        if "oneOf" in s:
            return " or ".join(type_of(x) for x in s["oneOf"])
        t = s.get("type", "object")
        if t == "array":
            return f"array of {type_of(s.get('items', {}))}"
        extra = []
        if "enum" in s:
            extra.append(" | ".join(f"<code>{html.escape(str(e))}</code>" for e in s["enum"]))
        if "format" in s:
            extra.append(s["format"])
        rng = []
        if "minimum" in s:
            rng.append(f"min {s['minimum']}")
        if "maximum" in s:
            rng.append(f"max {s['maximum']}")
        if "maxLength" in s:
            rng.append(f"max length {s['maxLength']}")
        if "default" in s:
            rng.append(f"default {s['default']}")
        if s.get("nullable"):
            rng.append("nullable")
        return " · ".join([t] + extra + rng)

    def fields_table(s):
        s = deref(s)
        props = s.get("properties")
        if not props:
            return f"<p class='muted'>{type_of(s)}</p>"
        req = set(s.get("required", []))
        rows = "".join(
            f"<tr><td><code>{k}</code>{' *' if k in req else ''}</td><td>{type_of(v)}</td><td>{md(v.get('description', '')) if v.get('description') else ''}</td></tr>"
            for k, v in props.items())
        return f"<table><tr><th>Field (* required)</th><th>Type</th><th>Notes</th></tr>{rows}</table>"

    def resp(r):
        if "$ref" in r:
            r = comps["responses"][r["$ref"].split("/")[-1]]
        content = r.get("content", {}).get("application/json", {}).get("schema")
        shape = ""
        if content:
            data = content.get("properties", {}).get("data")
            shape = f"<code>data</code>: {type_of(data)}" if data else type_of(content)
        desc = markdown.markdown(r.get("description", "").strip())
        return re.sub(r"^<p>|</p>$", "", desc), shape

    toc, ops = [], []
    tags = [t["name"] for t in spec.get("tags", [])]
    by_tag = {t: [] for t in tags}
    for path, item in spec["paths"].items():
        for method, op in item.items():
            by_tag.setdefault((op.get("tags") or ["other"])[0], []).append((path, method, op))
    for tag in by_tag:
        if not by_tag[tag]:
            continue
        desc = next((t.get("description", "") for t in spec.get("tags", []) if t["name"] == tag), "")
        ops.append(f"<h2 id='tag-{tag}'>{tag}</h2><div class='muted'>{md(desc)}</div>")
        toc.append(f"<li><a href='#tag-{tag}'>{tag}</a>: " + ", ".join(f"<code>{m.upper()} {p}</code>" for p, m, _ in by_tag[tag]) + "</li>")
        for path, method, op in by_tag[tag]:
            sec = op.get("security", spec.get("security"))
            auth = "none (public)" if sec == [] else " or ".join({"ApiKey": "X-API-Key", "BearerAuth": "Bearer JWT"}[list(s)[0]] for s in sec)
            parts = [f"<div class='op'><h3><span class='m {method}'>{method.upper()}</span><code>{path}</code></h3>",
                     f"<p><b>{html.escape(op.get('summary', ''))}</b> <span class='auth'>· auth: {auth}</span></p>",
                     md(op.get("description", ""))]
            params = []
            for prm in op.get("parameters", []):
                if "$ref" in prm:
                    prm = comps["parameters"][prm["$ref"].split("/")[-1]]
                params.append(f"<tr><td><code>{prm['name']}</code>{' *' if prm.get('required') else ''}</td><td>{prm['in']}</td><td>{type_of(prm.get('schema', {}))}</td><td>{html.escape(prm.get('description', ''))}</td></tr>")
            if params:
                parts.append("<h4>Parameters</h4><table><tr><th>Name</th><th>In</th><th>Type</th><th>Notes</th></tr>" + "".join(params) + "</table>")
            body = op.get("requestBody", {}).get("content", {}).get("application/json", {}).get("schema")
            if body:
                parts.append("<h4>Request body (JSON)</h4>" + fields_table(body))
            rows = "".join(f"<tr><td><code>{code}</code></td><td>{d}</td><td>{s}</td></tr>" for code, r in op["responses"].items() for d, s in [resp(r)])
            parts.append(f"<h4>Responses</h4><table><tr><th>Status</th><th>Meaning</th><th>Body</th></tr>{rows}</table></div>")
            ops.append("".join(parts))

    schemas = "".join(f"<h3 id='schema-{name}'>{name}</h3>{md(s.get('description', '')) if s.get('description') else ''}{fields_table(s)}"
                      for name, s in comps["schemas"].items())
    info = spec["info"]
    servers = "".join(f"<li><code>{s['url']}</code> — {html.escape(s.get('description', ''))}</li>" for s in spec["servers"])
    body = (f"<h1>{html.escape(info['title'])} <span class='muted'>v{info['version']}</span></h1>"
            f"{md(info.get('description', ''))}<h2>Servers</h2><ul>{servers}</ul>"
            "<h2>Authentication</h2><ul><li><b>X-API-Key</b>: one revocable key per group (request it from the KotaSiaga team).</li>"
            "<li><b>Bearer JWT</b>: <code>Authorization: Bearer &lt;access_token&gt;</code> from <code>POST /auth/login</code>.</li></ul>"
            "<h2>Envelopes</h2><pre>{ \"data\": ..., \"meta\": { \"seq\": 1042, \"updated_at\": \"2026-11-22T06:57:28Z\", \"source\": \"live\", \"stale\": false } }\n"
            "{ \"error\": { \"code\": \"validation_error\", \"message\": \"...\", \"request_id\": \"...\" } }</pre>"
            f"<h2>Endpoints</h2><ul>{''.join(toc)}</ul>{''.join(ops)}<h2>Schemas</h2>{schemas}")
    outdir = ROOT / "docs/api"
    outdir.mkdir(parents=True, exist_ok=True)
    out = outdir / "KotaSiaga-API.html"
    out.write_text(page(f"{info['title']} v{info['version']}", body))
    to_pdf(out, outdir / "KotaSiaga-API.pdf", f"{info['title']} v{info['version']}")
    print("docs/api/KotaSiaga-API.html, docs/api/KotaSiaga-API.pdf")


if __name__ == "__main__":
    build_readme()
    build_api()
