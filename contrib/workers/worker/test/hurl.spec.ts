import { SELF } from "cloudflare:test";
import { describe, expect, inject, it } from "vitest";
import { formatHurl, type HurlOptions, MemoryFileSystem, parseHurl, runHurl, type RunResult } from "../src/hurl";

const origin = inject("mockOrigin");

async function run(content: string, variables: Record<string, string> = {}): Promise<RunResult> {
	return runHurl(content, { variables: { origin, ...variables } });
}

function failedAsserts(result: RunResult) {
	return result.result!.entries.flatMap((e) => e.asserts.filter((a) => !a.success));
}

describe("parse (hurl_core)", () => {
	it("parses a valid file", async () => {
		const result = await parseHurl("GET https://example.org\nHTTP 200\n\nGET https://example.org/2\n");
		expect(result).toEqual({ ok: true, entries: 2 });
	});

	it("reports a syntax error with its position", async () => {
		const result = await parseHurl("GET https://example.org\nHTTP 200\n[Asserts]\nfoo == 1\n");
		expect(result.ok).toBe(false);
		expect(result.error).toMatchObject({ line: 4, column: 1 });
	});

	it("parses an XML body", async () => {
		const ok = await parseHurl('POST https://example.org\n<?xml version="1.0"?>\n<a><b x="1">t</b><!-- c --></a>\nHTTP 200\n');
		expect(ok).toEqual({ ok: true, entries: 1 });
		const ko = await parseHurl("POST https://example.org\n<a><b></a>\nHTTP 200\n");
		expect(ko.ok).toBe(false);
	});
});

describe("run (hurl runner)", () => {
	it("runs requests with asserts, captures and templates", async () => {
		const result = await run(`
GET {{origin}}/json
HTTP 200
[Captures]
token: jsonpath "$.token"
[Asserts]
header "Content-Type" == "application/json"
jsonpath "$.name" == "Bob"
jsonpath "$.age" > 18
jsonpath "$.tags" count == 2
body contains "Bob"
sha256 exists
duration < 5000

GET {{origin}}/echo?q={{token}}
Authorization: Bearer {{token}}
HTTP 200
[Asserts]
jsonpath "$.query" == "?q=t0k3n"
jsonpath "$.headers.authorization" == "Bearer t0k3n"
jsonpath "$.headers['user-agent']" startsWith "hurl/"
`);
		expect(failedAsserts(result)).toEqual([]);
		expect(result).toMatchObject({ ok: true, success: true });
		expect(result.result!.entries[0].captures).toEqual([{ name: "token", value: "t0k3n" }]);
	});

	it("reports failed asserts", async () => {
		const result = await run(`
GET {{origin}}/json
HTTP 201
`);
		expect(result).toMatchObject({ ok: true, success: false });
		expect(JSON.stringify(result.result)).toContain("201");
	});

	it("sends JSON, form and multipart bodies", async () => {
		const result = await run(`
POST {{origin}}/echo
{"a": 1}
HTTP 200
[Asserts]
jsonpath "$.headers['content-type']" == "application/json"
jsonpath "$.body" == "{\\"a\\": 1}"

POST {{origin}}/echo
[Form]
name: Bob Smith
lang: fr&en
HTTP 200
[Asserts]
jsonpath "$.headers['content-type']" == "application/x-www-form-urlencoded"
jsonpath "$.body" == "name=Bob%20Smith&lang=fr%26en"

POST {{origin}}/echo
[Multipart]
field1: value1
HTTP 200
[Asserts]
jsonpath "$.headers['content-type']" startsWith "multipart/form-data; boundary="
jsonpath "$.body" contains "name=\\"field1\\""
jsonpath "$.body" contains "value1"
`);
		expect(failedAsserts(result)).toEqual([]);
		expect(result.success).toBe(true);
	});

	it("follows redirects only when asked", async () => {
		const result = await run(`
GET {{origin}}/redirect
HTTP 302
[Asserts]
header "Location" == "/hello"

GET {{origin}}/redirect
[Options]
location: true
HTTP 200
[Asserts]
url endsWith "/hello"
redirects count == 1
body == "Hello World!"
`);
		expect(failedAsserts(result)).toEqual([]);
		expect(result.success).toBe(true);
	});

	it("keeps cookies between requests", async () => {
		const result = await run(`
GET {{origin}}/set-cookie
HTTP 200
[Asserts]
cookie "session" == "abc123"
cookie "session[HttpOnly]" exists

GET {{origin}}/check-cookie
HTTP 200
[Asserts]
body contains "theme=dark"
`);
		expect(failedAsserts(result)).toEqual([]);
		expect(result.success).toBe(true);
	});

	it("retries and delays with the host sleep", async () => {
		const result = await run(`
GET {{origin}}/flaky?id={{id}}
[Options]
delay: 10ms
retry: 5
retry-interval: 10ms
HTTP 200
[Asserts]
jsonpath "$.count" == 3
`, { id: crypto.randomUUID() });
		// Each retry adds an entry result: two failures on HTTP 500, then the success.
		const entries = result.result!.entries;
		expect(entries.map((e) => e.calls[0].response.status)).toEqual([500, 500, 200]);
		expect(result.success).toBe(true);
	});

	it("supports basic auth, filters and functions", async () => {
		const result = await run(`
GET {{origin}}/basic-auth
[BasicAuth]
bob: secret
HTTP 200

GET {{origin}}/echo?id={{newUuid}}
HTTP 200
[Asserts]
jsonpath "$.query" matches /^\\?id=[0-9a-f-]{36}$/
jsonpath "$.query" regex /id=(.+)$/ split "-" count == 5
jsonpath "$.method" replace "GET" "get" == "get"
`);
		expect(failedAsserts(result)).toEqual([]);
		expect(result.success).toBe(true);
	});

	it("decodes gzip response bodies", async () => {
		const result = await run(`
GET {{origin}}/gzip
HTTP 200
[Asserts]
body == "Hello compressed World!"
`);
		expect(failedAsserts(result)).toEqual([]);
		expect(result.success).toBe(true);
	});

	it("evaluates XPath on HTML with libxml2, like the CLI", async () => {
		const result = await run(`
GET {{origin}}/html
HTTP 200
[Captures]
title: xpath "string(//title)"
[Asserts]
xpath "string(//h1)" == "Hi"
xpath "//h1" count == 1
xpath "boolean(//nothing)" == false
body xpath "normalize-space(//body)" == "Hi"
`);
		expect(failedAsserts(result)).toEqual([]);
		expect(result.success).toBe(true);
		expect(result.result!.entries[0].captures).toEqual([{ name: "title", value: "Hurl" }]);
	});

	it("parses HTML and XML like the CLI's libxml2", async () => {
		const bodies: Record<string, [string, string]> = {
			"/table": ["text/html", "<table><tr><td>a<td>b</table><p>one<p>two"],
			"/feed": [
				"application/xml",
				'<feed xmlns="http://www.w3.org/2005/Atom" xmlns:m="urn:m"><entry><title>A</title><m:x>1</m:x></entry><entry/></feed>',
			],
			"/broken": ["application/xml", "<a><b></a>"],
		};
		const result = await runHurl(
			`
GET http://test/table
HTTP 200
[Asserts]
xpath "count(//table/tr)" == 1
xpath "count(//tbody)" == 0
xpath "count(//td)" == 2
xpath "string((//p)[1])" == "one"

GET http://test/feed
HTTP 200
[Asserts]
xpath "count(//_:entry)" == 2
xpath "string(//_:entry/_:title)" == "A"
xpath "string(//m:x)" == "1"
xpath "local-name(/*)" == "feed"

GET http://test/broken
HTTP 200
[Asserts]
xpath "count(//b)" == 1
`,
			{
				fetcher: async (input) => {
					const [type, body] = bodies[new URL(input instanceof Request ? input.url : input.toString()).pathname];
					return new Response(body, { headers: { "Content-Type": type } });
				},
			},
		);
		expect(failedAsserts(result)).toEqual([]);
		expect(result.success).toBe(true);
	});

	it("reports connection errors", async () => {
		const result = await run(`
GET http://127.0.0.1:1/nothing
HTTP 200
`);
		expect(result.success).toBe(false);
	});
});

describe("files (host file system)", () => {
	it("reads request bodies, multipart files and predicate files from the host", async () => {
		const files = new MemoryFileSystem({
			"payload.json": '{"id":42}',
			"docs/readme.txt": "multipart file content",
			"hello.txt": "Hello World!",
		});
		const result = await runHurl(
			`
POST {{origin}}/echo
Content-Type: application/json
file,payload.json;
HTTP 200
[Asserts]
jsonpath "$.body" == "{\\"id\\":42}"

POST {{origin}}/echo
[Multipart]
upload: file,docs/readme.txt; text/plain
HTTP 200
[Asserts]
jsonpath "$.body" contains "multipart file content"
jsonpath "$.body" contains "filename=\\"docs/readme.txt\\""

GET {{origin}}/hello
HTTP 200
[Asserts]
bytes == file,hello.txt;
`,
			{ variables: { origin }, files },
		);
		expect(failedAsserts(result)).toEqual([]);
		expect(result).toMatchObject({ ok: true, success: true });
	});

	it("writes the output option to the host", async () => {
		const files = new MemoryFileSystem();
		const result = await runHurl(
			`
GET {{origin}}/json
[Options]
output: out/response.json
HTTP 200
`,
			{ variables: { origin }, files },
		);
		expect(result).toMatchObject({ ok: true, success: true });
		expect(JSON.parse(files.text("out/response.json")!)).toMatchObject({ name: "Bob" });
	});

	it("reports missing files, and refuses paths outside the file root", async () => {
		const reads: string[] = [];
		const files = {
			read(path: string) {
				reads.push(path);
				return path === "secret" ? new TextEncoder().encode("s3cr3t") : undefined;
			},
		};
		const missing = await runHurl(`POST {{origin}}/echo\nfile,missing.bin;\nHTTP 200\n`, { variables: { origin }, files });
		expect(missing.success).toBe(false);
		expect(missing.errors).toMatchObject([{ entry: 1, line: 2, description: "File read access" }]);
		expect(missing.errors![0].message).toMatch(/missing\.bin/);

		for (const path of ["../secret", "/secret"]) {
			const outside = await runHurl(`POST {{origin}}/echo\nfile,${path};\nHTTP 200\n`, { variables: { origin }, files });
			expect(outside.success).toBe(false);
			expect(outside.errors).toMatchObject([{ description: "Unauthorized file access" }]);
		}
		expect(reads).toEqual(["missing.bin"]);
	});

	it("caps the size of files, and cancels streams over the cap", async () => {
		let pulled = 0;
		let canceled = false;
		const endless = () =>
			new ReadableStream<Uint8Array>({
				pull(controller) {
					pulled++;
					controller.enqueue(new Uint8Array(1024).fill(65));
				},
				cancel() {
					canceled = true;
				},
			});
		const stream = (text: string) =>
			new ReadableStream<Uint8Array>({
				start(controller) {
					for (const part of text.match(/.{1,4}/g)!) {
						controller.enqueue(new TextEncoder().encode(part));
					}
					controller.close();
				},
			});
		const sizes: number[] = [];
		const files = {
			read(path: string, { maxSize }: { maxSize: number }) {
				sizes.push(maxSize);
				if (path === "huge.bin") return endless();
				if (path === "big.bin") return new Uint8Array(5000);
				return stream("Hello World!");
			},
			write() {},
		};
		const options = { variables: { origin }, files, maxFileSize: 4096 };

		const small = await runHurl(`GET {{origin}}/hello\nHTTP 200\n[Asserts]\nbytes == file,hello.txt;\n`, options);
		expect(failedAsserts(small)).toEqual([]);
		expect(small).toMatchObject({ ok: true, success: true });
		expect(sizes).toEqual([4096]);

		const huge = await runHurl(`POST {{origin}}/echo\nfile,huge.bin;\nHTTP 200\n`, options);
		expect(huge.success).toBe(false);
		expect(huge.errors![0].message).toMatch(/file huge\.bin can not be read/);
		expect(huge.fileErrors).toEqual([
			{ path: "huge.bin", operation: "read", message: "file huge.bin is larger than the limit of 4096 bytes" },
		]);
		expect(canceled).toBe(true);
		expect(pulled).toBeLessThan(10);

		const big = await runHurl(`POST {{origin}}/echo\nfile,big.bin;\nHTTP 200\n`, options);
		expect(big.fileErrors![0].message).toMatch(/big\.bin is larger than the limit/);

		const write = await runHurl(`POST {{origin}}/echo\n[Options]\noutput: out.bin\nfile,big.bin;\nHTTP 200\n`, {
			...options,
			maxFileSize: 6000,
		});
		expect(write.success).toBe(false);
		expect(write.errors![0].message).toMatch(/out\.bin is larger than the limit of 6000 bytes/);
	});

	it("fails file access when the Worker gives no file system", async () => {
		const read = await run(`POST {{origin}}/echo\nfile,data.bin;\nHTTP 200\n`);
		expect(read.success).toBe(false);
		expect(read.errors![0].description).toBe("File read access");
		const write = await run(`GET {{origin}}/hello\n[Options]\noutput: out.txt\nHTTP 200\n`);
		expect(write.success).toBe(false);
		expect(write.errors![0].message).toMatch(/not supported/);
	});
});

describe("options (like the CLI options)", () => {
	const withOptions = (content: string, options: HurlOptions) => runHurl(content, { variables: { origin }, options });

	it("follows redirects and retries, without [Options] in the file", async () => {
		const redirect = await withOptions(`GET {{origin}}/redirect\nHTTP 200\n`, { location: true, maxRedirs: 1 });
		expect(redirect).toMatchObject({ ok: true, success: true });
		const noRedirect = await withOptions(`GET {{origin}}/redirect\nHTTP 200\n`, {});
		expect(noRedirect.success).toBe(false);

		const retry = await withOptions(`GET {{origin}}/flaky?id=options\nHTTP 200\n`, { retry: 5, retryIntervalMs: 10 });
		expect(retry).toMatchObject({ ok: true, success: true });
	});

	it("adds headers, user, and user agent to each request", async () => {
		const result = await withOptions(
			`
GET {{origin}}/echo
HTTP 200
[Asserts]
jsonpath "$.headers['x-tenant']" == "acme"
jsonpath "$.headers['user-agent']" == "my-saas/1.0"

GET {{origin}}/basic-auth
HTTP 200
`,
			{ headers: ["X-Tenant: acme"], userAgent: "my-saas/1.0", user: "bob:secret" },
		);
		expect(failedAsserts(result)).toEqual([]);
		expect(result).toMatchObject({ ok: true, success: true });
	});

	it("selects entries, and ignores asserts", async () => {
		const content = `GET {{origin}}/hello\nHTTP 200\n\nGET {{origin}}/missing\nHTTP 200\n`;
		expect((await withOptions(content, {})).success).toBe(false);
		const first = await withOptions(content, { toEntry: 1 });
		expect(first).toMatchObject({ ok: true, success: true });
		expect(first.result!.entries).toHaveLength(1);
		expect((await withOptions(content, { ignoreAsserts: true })).success).toBe(true);
	});

	it("rejects unknown options", async () => {
		const result = await withOptions(`GET {{origin}}/hello\n`, { insecure: true } as HurlOptions);
		expect(result).toMatchObject({ ok: false });
		expect(result.error).toMatch(/insecure/);
	});
});

describe("unsupported features (not available with fetch)", () => {
	const content = `GET {{origin}}/hello
[Options]
http2: true
ipv6: true
insecure: false
HTTP/2 200
[Asserts]
ip isIpv6
`;

	it("fails the run before any request by default", async () => {
		let calls = 0;
		const fetcher: typeof fetch = (input, init) => {
			calls++;
			return fetch(input, init);
		};
		const result = await runHurl(content, { variables: { origin }, fetcher });
		expect(result.ok).toBe(false);
		expect(calls).toBe(0);
		expect(result.unsupported?.map((u) => [u.line, u.column, u.feature])).toEqual([
			[3, 1, "option `http2`"],
			[4, 1, "option `ipv6`"],
			[6, 1, "`HTTP/2`"],
			[8, 1, "query `ip`"],
		]);
		expect(result.error).toContain("input.hurl:3:1: option `http2` is not supported in Workers");
	});

	it("runs files with HTTP, HTTP/1.1 and disabled options", async () => {
		const result = await runHurl(
			`GET {{origin}}/hello\n[Options]\nhttp2: false\ninsecure: false\nHTTP 200\nGET {{origin}}/hello\nHTTP/1.1 200\n`,
			{ variables: { origin } },
		);
		expect(result).toMatchObject({ ok: true, success: true });
		expect(result.unsupported).toBeUndefined();
	});

	it("ignores them when asked, and HTTP/x matches any version", async () => {
		const ok = await runHurl(content.replace("ip isIpv6", "ip == \"\""), {
			variables: { origin },
			unsupported: "ignore",
		});
		expect(ok).toMatchObject({ ok: true, success: true });
		expect(ok.unsupported).toHaveLength(4);

		const failed = await runHurl(`GET {{origin}}/hello\nHTTP/2 404\n`, {
			variables: { origin },
			unsupported: "ignore",
		});
		expect(failed.success).toBe(false);
		expect(failed.errors?.[0].message).toContain("HTTP/2 404");
	});
});

describe("JSON report (like --report-json)", () => {
	const bodies = (result: RunResult) =>
		result.result!.entries.map((e: any) => e.calls[0].response.body as string | undefined);

	it("saves response bodies through the host file system", async () => {
		const files = new MemoryFileSystem();
		const result = await runHurl(`GET {{origin}}/json\nHTTP 200\n\nGET {{origin}}/hello\nHTTP 200\n`, {
			variables: { origin },
			files,
			responseDir: "report/store",
		});
		expect(result).toMatchObject({ ok: true, success: true });
		const [json, text] = bodies(result);
		expect(json).toMatch(/^store\/[0-9a-f-]{36}_response\.json$/);
		expect(text).toMatch(/^store\/[0-9a-f-]{36}_response$/);
		expect(JSON.parse(files.text(`report/${json}`)!)).toMatchObject({ name: "Bob" });
		expect(files.text(`report/${text}`)).toBe("Hello World!");
	});

	it("drops bodies over maxRetainedBodySize, after their asserts and captures", async () => {
		const files = new MemoryFileSystem();
		const entry = `GET {{origin}}/hello\nHTTP 200\n[Captures]\ngreeting: body\n[Asserts]\nbody == "Hello World!"\n`;
		const result = await runHurl(`${entry}\n${entry}\n${entry}\nGET {{origin}}/echo?q={{greeting}}\nHTTP 200\n`, {
			variables: { origin },
			files,
			responseDir: "store",
			options: { maxRetainedBodySize: 30 },
		});
		expect(failedAsserts(result)).toEqual([]);
		expect(result).toMatchObject({ ok: true, success: true });
		const saved = bodies(result);
		expect(saved.map((b) => b !== undefined)).toEqual([true, true, false, false]);
		expect(files.files.size).toBe(2);
		expect(result.result!.entries[2].captures).toMatchObject([{ name: "greeting", value: "Hello World!" }]);
	});

	it("refuses response directories outside the file root", async () => {
		for (const responseDir of ["../x", "/x", "a/../../x"]) {
			const result = await runHurl(`GET {{origin}}/hello\n`, { variables: { origin }, responseDir });
			expect(result).toMatchObject({ ok: false });
			expect(result.error).toMatch(/invalid response directory/);
		}
	});

	it("returns the report without bodies when they cannot be saved", async () => {
		const result = await runHurl(`GET {{origin}}/hello\nHTTP 200\n`, { variables: { origin }, responseDir: "store" });
		expect(result).toMatchObject({ ok: true, success: true });
		expect(result.reportError).toMatch(/not supported/);
		expect(bodies(result)).toEqual([undefined]);
	});
});

describe("formatHurl (hurlfmt)", () => {
	it("formats a Hurl file and checks if it is formatted", async () => {
		const result = await formatHurl("GET   https://example.org\nHTTP 200\n");
		expect(result).toMatchObject({ ok: true, output: "GET https://example.org\nHTTP 200\n", formatted: false });
		expect(await formatHurl(result.output!)).toMatchObject({ ok: true, formatted: true });
	});

	it("exports to JSON and HTML", async () => {
		const json = await formatHurl("GET https://example.org\nHTTP 200\n", { outputFormat: "json" });
		expect(json.ok).toBe(true);
		expect(JSON.parse(json.output!)).toMatchObject({ entries: [{ request: { method: "GET", url: "https://example.org" } }] });
		const html = await formatHurl("GET https://example.org\n", { outputFormat: "html" });
		expect(html.output).toContain("https://example.org");
		expect(html.output).not.toContain("<html");
		const page = await formatHurl("GET https://example.org\n", { outputFormat: "html", standalone: true });
		expect(page.output).toContain("<html");
	});

	it("converts curl command lines", async () => {
		const result = await formatHurl("curl -H 'X-Tenant: acme' https://example.org/api\n", { inputFormat: "curl" });
		expect(result).toMatchObject({ ok: true, formatted: null });
		expect(result.output).toContain("GET https://example.org/api\nX-Tenant: acme");
	});

	it("returns parse errors", async () => {
		const result = await formatHurl("GET https://example.org\nHTTP 200\n[Asserts]\nfoo == 1\n");
		expect(result).toMatchObject({ ok: false, error: { line: 4 } });
		expect(result.error!.message).toContain("foo == 1");
		expect(await formatHurl("x", { outputFormat: "xml" as "json" })).toMatchObject({ ok: false });
	});
});

describe("concurrency", () => {
	it("runs several Hurl files at the same time, one WebAssembly instance each", async () => {
		const file = (id: number) => `
GET {{origin}}/slow?ms=300&id=${id}
HTTP 200
[Asserts]
jsonpath "$.id" == "${id}"

GET {{origin}}/slow?ms=300&id=${id}
HTTP 200
[Asserts]
jsonpath "$.id" == "${id}"
`;
		const start = Date.now();
		const results = await Promise.all([1, 2, 3, 4, 5].map((id) => run(file(id))));
		const elapsed = Date.now() - start;
		for (const result of results) {
			expect(failedAsserts(result)).toEqual([]);
			expect(result).toMatchObject({ ok: true, success: true });
		}
		// Each file waits 2 x 300 ms; 5 files one after another would take 3 s.
		expect(elapsed).toBeLessThan(1500);
	});
});

describe("worker", () => {
	it("runs a Hurl file posted to /run", async () => {
		const response = await SELF.fetch(`https://worker.test/run?origin=${encodeURIComponent(origin)}`, {
			method: "POST",
			body: 'GET {{origin}}/hello\nHTTP 200\n[Asserts]\nbody == "Hello World!"\n',
		});
		expect(response.status).toBe(200);
		expect(await response.json()).toMatchObject({ ok: true, success: true });
	});

	it("checks a Hurl file posted to /parse", async () => {
		const response = await SELF.fetch("https://worker.test/parse", { method: "POST", body: "GET\n" });
		expect(await response.json()).toMatchObject({ ok: false });
	});
});
