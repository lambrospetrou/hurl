// HTTP server used by the compatibility suite: the native Hurl CLI and hurl-wasm both run against
// it. `POST /__reset` clears its state, so each run of a file sees the same responses.
import { createServer, type IncomingMessage, type Server, type ServerResponse } from "node:http";
import { performance } from "node:perf_hooks";
import { brotliCompressSync, deflateRawSync, deflateSync, gzipSync, zstdCompressSync } from "node:zlib";
import { SECRETS, type CompatRow } from "./compat";

let rows: CompatRow[] = [];

/** Returns the rows posted by the compat spec, and clears them. */
export function takeRows(): CompatRow[] {
	const result = rows;
	rows = [];
	return result;
}

const USERS = [
	{ id: 1, name: "Alice", email: "alice@example.com", age: 30, active: true, score: 4.5, roles: ["admin", "dev"], address: { city: "Paris", zip: "75001" } },
	{ id: 2, name: "Bob", email: "bob@example.com", age: 25, active: false, score: 3.25, roles: ["dev"], address: { city: "Berlin", zip: "10115" } },
	{ id: 3, name: "Chloé", email: "chloe@example.com", age: 41, active: true, score: 5, roles: [], address: { city: "Αθήνα", zip: null } },
];

const HTML_PAGE = `<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="description" content="Compatibility page">
<title>Compat &amp; Hurl</title>
<link rel="stylesheet" href="/static/site.css">
<script src="/static/app.js" defer></script>
</head>
<body class="home">
<nav><a href="/">Home</a> <a href="/about" class="active">About</a> <a href="https://example.com/x?a=1&amp;b=2">External</a></nav>
<h1 id="title">Welcome, Hurl</h1>
<p>First paragraph
<p class="note">Second <b>bold</b> paragraph
<ul><li>One<li>Two<li>Three</ul>
<table id="prices">
<tr><th>Item</th><th>Price</th></tr>
<tr><td>Apple<td>1.50</tr>
<tr><td>Pear<td>2.25</tr>
</table>
<form action="/html/login" method="post">
<input type="hidden" name="csrf" value="csrf-31415926">
<input type="text" name="user">
<input type="password" name="password">
<button type="submit">Log in</button>
</form>
<!-- a comment -->
<script>
  window.APP = { version: "1.4.2", features: ["a", "b"] };
  if (1 < 2 && 3 > 2) { console.log("</div>"); }
</script>
<p>Caf&eacute; &lt;tag&gt; &#x1F680;</p>
</body>
</html>
`;

const APP_JS = `"use strict";
const API_BASE = "/api/v1";
export function greet(name) {
  return \`Hello, \${name}!\`;
}
window.APP_CONFIG = { version: "1.4.2", debug: false, retries: 3 };
`;

const XML_FEED = `<?xml version="1.0" encoding="UTF-8"?>
<feed xmlns="http://www.w3.org/2005/Atom" xmlns:media="http://search.yahoo.com/mrss/">
  <title>Compat feed</title>
  <entry><title>First</title><media:thumbnail url="https://example.com/1.png"/></entry>
  <entry><title>Second &amp; last</title></entry>
</feed>
`;

let state: { flaky: Map<string, number>; counter: number } = { flaky: new Map(), counter: 0 };

const CT_TEXT = "Hello encoded world! café 日本\n";
const CONTENT: Record<string, { body: () => string | Buffer; headers: Record<string, string> }> = {
	deflate: { body: () => deflateSync(CT_TEXT), headers: { "Content-Encoding": "deflate" } },
	"deflate-raw": { body: () => deflateRawSync(CT_TEXT), headers: { "Content-Encoding": "deflate" } },
	zstd: { body: () => zstdCompressSync(CT_TEXT), headers: { "Content-Encoding": "zstd" } },
	"gzip-br": { body: () => brotliCompressSync(gzipSync(CT_TEXT)), headers: { "Content-Encoding": "gzip, br" } },
	br: { body: () => brotliCompressSync(CT_TEXT), headers: { "Content-Encoding": "br" } },
	"gzip-upper": { body: () => gzipSync(CT_TEXT), headers: { "Content-Encoding": "GZIP" } },
	"x-gzip": { body: () => gzipSync(CT_TEXT), headers: { "Content-Encoding": "x-gzip" } },
	identity: { body: () => CT_TEXT, headers: { "Content-Encoding": "identity" } },
	"gzip-json": { body: () => gzipSync(JSON.stringify({ ok: true, n: 1 })), headers: { "Content-Encoding": "gzip", "Content-Type": "application/json" } },
	"gzip-html": { body: () => gzipSync("<html><body><p>one<p>two</body></html>"), headers: { "Content-Encoding": "gzip", "Content-Type": "text/html" } },
	windows1252: { body: () => Buffer.from([0x63, 0x61, 0x66, 0xe9, 0x20, 0x80]), headers: { "Content-Type": "text/plain; charset=windows-1252" } },
	shiftjis: { body: () => Buffer.from([0x93, 0xfa, 0x96, 0x7b]), headers: { "Content-Type": "text/plain; charset=Shift_JIS" } },
	utf16: { body: () => Buffer.from("\ufeffhello", "utf16le"), headers: { "Content-Type": "text/plain; charset=UTF-16" } },
	"unknown-charset": { body: () => "hello", headers: { "Content-Type": "text/plain; charset=xxx" } },
	"no-content-type": { body: () => "plain without type", headers: {} },
	"html-meta-latin1": { body: () => Buffer.from('<html><head><meta charset="iso-8859-1"></head><body><p>caf\xe9</p></body></html>', "latin1"), headers: { "Content-Type": "text/html" } },
	"xml-latin1": { body: () => Buffer.from('<?xml version="1.0" encoding="ISO-8859-1"?><a><b>caf\xe9</b></a>', "latin1"), headers: { "Content-Type": "application/xml" } },
	"json-bom": { body: () => "\ufeff" + JSON.stringify({ ok: true }), headers: { "Content-Type": "application/json" } },
	"json-vendor": { body: () => JSON.stringify({ data: [{ id: 1 }] }), headers: { "Content-Type": "application/vnd.api+json" } },
	sse: { body: () => "event: tick\ndata: 1\n\nevent: tick\ndata: 2\n\n", headers: { "Content-Type": "text/event-stream" } },
	png: { body: () => Buffer.from("89504e470d0a1a0a0000000d49484452", "hex"), headers: { "Content-Type": "image/png" } },
	"multipart-mixed": { body: () => "--b\r\nContent-Type: text/plain\r\n\r\npart1\r\n--b--\r\n", headers: { "Content-Type": "multipart/mixed; boundary=b" } },
	"html-octet": { body: () => "<html><body><p>x</p></body></html>", headers: { "Content-Type": "application/octet-stream" } },
	"ndjson": { body: () => '{"a":1}\n{"a":2}\n', headers: { "Content-Type": "application/x-ndjson" } },
};

function send(res: ServerResponse, status: number, body: string | Buffer, headers: Record<string, string | string[]> = {}) {
	res.writeHead(status, { "Content-Type": "text/plain; charset=utf-8", ...headers });
	res.end(body);
}

function json(res: ServerResponse, status: number, value: unknown, headers: Record<string, string | string[]> = {}) {
	send(res, status, JSON.stringify(value), { "Content-Type": "application/json", ...headers });
}

function cookies(req: IncomingMessage): Record<string, string> {
	const result: Record<string, string> = {};
	for (const part of (req.headers.cookie ?? "").split(";")) {
		const [name, ...value] = part.trim().split("=");
		if (name) result[name] = value.join("=");
	}
	return result;
}

/** Request headers that don't depend on the HTTP client. */
function stableHeaders(req: IncomingMessage): Record<string, string> {
	const result: Record<string, string> = {};
	for (const [name, value] of Object.entries(req.headers)) {
		if (name.startsWith("x-") || ["authorization", "content-type", "cookie", "user-agent", "accept"].includes(name)) {
			result[name] = String(value);
		}
	}
	return result;
}

function handle(req: IncomingMessage, res: ServerResponse, body: Buffer) {
	const url = new URL(req.url ?? "/", "http://localhost");
	const path = url.pathname;
	const query = Object.fromEntries(url.searchParams);
	const method = req.method ?? "GET";

	if (path === "/__clock") return json(res, 200, { now: performance.now() });
	if (path === "/__compat/row" && method === "POST") {
		rows.push(JSON.parse(body.toString()));
		return send(res, 204, "");
	}
	if (path === "/__reset") {
		state = { flaky: new Map(), counter: 0 };
		return send(res, 204, "");
	}

	// JSON API
	if (path === "/api/users" && method === "GET") {
		const active = query.active;
		const users = active === undefined ? USERS : USERS.filter((u) => String(u.active) === active);
		return json(res, 200, { total: users.length, users }, { "X-Total-Count": String(users.length) });
	}
	if (path === "/api/users" && method === "POST") {
		const user = JSON.parse(body.toString() || "{}");
		return json(res, 201, { id: 4, ...user }, { Location: "/api/users/4" });
	}
	const userMatch = path.match(/^\/api\/users\/(\d+)$/);
	if (userMatch) {
		const id = Number(userMatch[1]);
		const user = USERS.find((u) => u.id === id) ?? (id === 4 ? { id: 4, name: "Dan" } : undefined);
		if (!user) return json(res, 404, { error: "not_found", message: `user ${id} not found` });
		if (method === "PUT" || method === "PATCH") return json(res, 200, { ...user, ...JSON.parse(body.toString() || "{}") });
		if (method === "DELETE") return send(res, 204, "");
		return json(res, 200, user, { ETag: `"user-${id}-v1"` });
	}
	if (path === "/api/types") {
		return json(res, 200, {
			int: 42, negative: -7, float: 3.14159, zero: 0, big: 9007199254740991, exp: 1.5e-7,
			bool: true, falsy: false, nil: null, str: "héllo wörld ✓", emoji: "🚀 launch", empty: "",
			escaped: 'line1\nline2\t"quoted" \\ back', list: [1, "two", 3.0, true, null], emptyList: [], emptyObj: {},
			nested: { a: { b: { c: [10, 20, 30] } } }, date: "2026-01-15T10:20:30Z", uuid: "4b2f7f4e-3c3a-4f7e-9b1d-1a2b3c4d5e6f",
			ipv4: "192.168.1.10", ipv6: "2001:db8::1", base64: Buffer.from("Hello Hurl").toString("base64"),
			"key with space": "spaced", "dash-key": "dashed", "dot.key": "dotted",
		});
	}
	if (path === "/api/paged") {
		const page = Number(query.page ?? "1");
		const items = [1, 2, 3].map((i) => ({ id: (page - 1) * 3 + i }));
		return json(res, 200, { page, items, next: page < 3 ? `/api/paged?page=${page + 1}` : null });
	}
	if (path === "/api/large") {
		const n = Number(query.n ?? "1000");
		const items = Array.from({ length: n }, (_, i) => ({ id: i, name: `item-${i}`, price: (i % 100) + 0.5, tags: ["a", i % 2 ? "odd" : "even"], in_stock: i % 3 !== 0 }));
		return json(res, 200, { count: n, items });
	}
	if (path === "/api/error") {
		return json(res, Number(query.status ?? "500"), { error: "boom", code: Number(query.status ?? "500") });
	}

	// Echo
	if (path === "/echo") {
		const text = body.toString("utf8");
		let parsed: unknown;
		try { parsed = JSON.parse(text); } catch { parsed = undefined; }
		return json(res, 200, { method, path, query, headers: stableHeaders(req), cookies: cookies(req), body: text, json: parsed, length: body.length });
	}

	// Auth
	if (path === "/auth/login" && method === "POST") {
		const credentials = JSON.parse(body.toString() || "{}");
		if (credentials.user !== "alice" || credentials.password !== "s3cret") return json(res, 401, { error: "invalid_credentials" });
		return json(res, 200, { token: "tok-9f8e7d6c5b4a", expires_in: 3600 }, { "Set-Cookie": "session=sess-77aa; Path=/; HttpOnly" });
	}
	if (path === "/auth/me") {
		if (req.headers.authorization !== "Bearer tok-9f8e7d6c5b4a") return json(res, 401, { error: "unauthorized" });
		return json(res, 200, { user: "alice", session: cookies(req).session ?? null });
	}
	if (path === "/auth/key") {
		const key = req.headers["x-api-key"] ?? query.key;
		if (key !== SECRETS.api_key) return json(res, 403, { error: "forbidden" });
		return json(res, 200, { ok: true }, { "X-Key-Echo": String(key) });
	}
	if (path === "/basic-auth") {
		const expected = `Basic ${Buffer.from("bob:secret").toString("base64")}`;
		if (req.headers.authorization !== expected) return send(res, 401, "Unauthorized", { "WWW-Authenticate": 'Basic realm="compat"' });
		return send(res, 200, "Authenticated as bob");
	}

	// Text
	if (path === "/text/plain") return send(res, 200, "Hello World!\nSecond line\n");
	if (path === "/text/unicode") return send(res, 200, "Ελληνικά, 日本語, emoji 🚀, accents àéîõü\n");
	if (path === "/text/latin1") return send(res, 200, Buffer.from("café crème", "latin1"), { "Content-Type": "text/plain; charset=ISO-8859-1" });
	if (path === "/text/csv") return send(res, 200, "id,name,price\n1,apple,1.50\n2,pear,2.25\n", { "Content-Type": "text/csv" });
	if (path === "/text/log") {
		return send(res, 200, "2026-01-15T10:20:30Z INFO  started pid=1234\n2026-01-15T10:20:31Z WARN  slow request ms=812\n2026-01-15T10:20:32Z ERROR failed code=E42\n");
	}
	if (path === "/text/empty") return send(res, 200, "");
	if (path === "/text/bytes") return send(res, 200, Buffer.from([0x00, 0x01, 0x02, 0xfe, 0xff]), { "Content-Type": "application/octet-stream" });
	if (path === "/text/gzip") return send(res, 200, gzipSync("Hello compressed world!\n"), { "Content-Encoding": "gzip" });
	if (path === "/text/headers") {
		return send(res, 200, "headers", {
			"X-Single": "one", "Cache-Control": "no-cache, max-age=0",
			"Set-Cookie": ["c1=v1; Path=/", "c2=v2; Path=/text; HttpOnly; Max-Age=3600", "c3=v3; Path=/; Expires=Wed, 13 Jan 2077 22:23:01 GMT; Secure"],
		});
	}

	// HTML / JS / CSS / XML
	if (path === "/html/page") return send(res, 200, HTML_PAGE, { "Content-Type": "text/html; charset=utf-8" });
	if (path === "/html/large") {
		const rows = Number(query.rows ?? "1000");
		const tr = Array.from({ length: rows }, (_, i) => `<tr class="${i % 2 ? "odd" : "even"}"><td>${i}</td><td>name-${i}</td></tr>`).join("\n");
		return send(res, 200, `<html><head><title>Large</title></head><body><table>${tr}</table></body></html>`, { "Content-Type": "text/html" });
	}
	if (path === "/html/login" && method === "POST") {
		const form = new URLSearchParams(body.toString());
		if (form.get("csrf") !== "csrf-31415926" || form.get("user") !== "alice") return send(res, 403, "<p>Forbidden</p>", { "Content-Type": "text/html" });
		return send(res, 302, "", { Location: "/html/dashboard", "Set-Cookie": "web_session=web-1234; Path=/; HttpOnly" });
	}
	if (path === "/html/dashboard") {
		if (cookies(req).web_session !== "web-1234") return send(res, 302, "", { Location: "/html/page" });
		return send(res, 200, `<html><body><h1>Dashboard</h1><p class="user">alice</p></body></html>`, { "Content-Type": "text/html" });
	}
	if (path === "/static/app.js") return send(res, 200, APP_JS, { "Content-Type": "application/javascript; charset=utf-8" });
	if (path === "/static/site.css") return send(res, 200, "body { color: #333; }\n.note { font-weight: bold; }\n", { "Content-Type": "text/css" });
	if (path === "/xml/feed") return send(res, 200, XML_FEED, { "Content-Type": "application/atom+xml" });

	// Redirects, retries, cookies, misc
	const redirect = path.match(/^\/redirect\/(\d+)$/);
	if (redirect) {
		const n = Number(redirect[1]);
		if (n === 0) return send(res, 200, "redirect done");
		return send(res, query.status ? Number(query.status) : 302, "", { Location: `/redirect/${n - 1}` });
	}
	if (path === "/redirect-post") return send(res, 307, "", { Location: "/echo" });
	if (path === "/flaky") {
		const id = query.id ?? "default";
		const count = (state.flaky.get(id) ?? 0) + 1;
		state.flaky.set(id, count);
		const failures = Number(query.failures ?? "2");
		return count > failures ? json(res, 200, { attempts: count }) : json(res, 503, { attempts: count });
	}
	if (path === "/counter") {
		state.counter += 1;
		return json(res, 200, { count: state.counter });
	}
	if (path === "/slow") {
		setTimeout(() => json(res, 200, { slow: true }), Number(query.ms ?? "200"));
		return;
	}
	if (path === "/cookies/set") {
		return send(res, 200, "cookies set", { "Set-Cookie": ["theme=dark; Path=/", "lang=en; Path=/cookies"] });
	}
	if (path.startsWith("/content/")) {
		const variant = CONTENT[path.slice("/content/".length)];
		if (variant) return send(res, 200, variant.body(), variant.headers);
	}
	if (path === "/known/multi-header") return send(res, 200, "multi", { "X-Multi": ["a", "b"] });
	if (path === "/known/cookie/default-path") return send(res, 200, "cookie", { "Set-Cookie": "nopath=1" });
	if (path === "/cookies/echo") return json(res, 200, cookies(req));

	send(res, 404, `not found: ${path}`);
}

export async function startServer(): Promise<{ server: Server; origin: string }> {
	const server = createServer((req, res) => {
		const chunks: Buffer[] = [];
		req.on("data", (chunk: Buffer) => chunks.push(chunk));
		req.on("end", () => handle(req, res, Buffer.concat(chunks)));
	});
	await new Promise<void>((resolve) => server.listen(0, "127.0.0.1", resolve));
	const address = server.address();
	if (address === null || typeof address === "string") throw new Error("no server address");
	return { server, origin: `http://127.0.0.1:${address.port}` };
}
