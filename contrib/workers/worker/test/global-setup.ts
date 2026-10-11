// Starts a local HTTP server that the Hurl files of the tests call from workerd.
import { createServer, type IncomingMessage, type ServerResponse } from "node:http";
import type { AddressInfo } from "node:net";
import { gzipSync } from "node:zlib";
import type { TestProject } from "vitest/node";

declare module "vitest" {
	export interface ProvidedContext {
		mockOrigin: string;
	}
}

const flakyCounts = new Map<string, number>();

async function readBody(req: IncomingMessage): Promise<Buffer> {
	const chunks: Buffer[] = [];
	for await (const chunk of req) {
		chunks.push(chunk as Buffer);
	}
	return Buffer.concat(chunks);
}

async function handle(req: IncomingMessage, res: ServerResponse) {
	const url = new URL(req.url ?? "/", "http://localhost");
	const json = (status: number, value: unknown) => {
		res.writeHead(status, { "Content-Type": "application/json" });
		res.end(JSON.stringify(value));
	};
	switch (url.pathname) {
		case "/hello":
			res.writeHead(200, { "Content-Type": "text/plain" });
			res.end("Hello World!");
			return;
		case "/json":
			json(200, { name: "Bob", age: 30, tags: ["a", "b"], token: "t0k3n" });
			return;
		case "/echo": {
			const body = await readBody(req);
			json(200, { method: req.method, query: url.search, headers: req.headers, body: body.toString("utf8") });
			return;
		}
		case "/redirect":
			res.writeHead(302, { Location: "/hello" });
			res.end();
			return;
		case "/set-cookie":
			res.writeHead(200, { "Set-Cookie": ["session=abc123; Path=/; HttpOnly", "theme=dark"] });
			res.end();
			return;
		case "/check-cookie":
			res.writeHead(req.headers.cookie?.includes("session=abc123") ? 200 : 401);
			res.end(req.headers.cookie ?? "");
			return;
		case "/flaky": {
			const id = url.searchParams.get("id") ?? "";
			const count = (flakyCounts.get(id) ?? 0) + 1;
			flakyCounts.set(id, count);
			json(count < 3 ? 500 : 200, { count });
			return;
		}
		case "/slow": {
			const ms = Number(url.searchParams.get("ms") ?? "0");
			await new Promise((resolve) => setTimeout(resolve, ms));
			json(200, { ms, id: url.searchParams.get("id") });
			return;
		}
		case "/gzip":
			res.writeHead(200, { "Content-Type": "text/plain", "Content-Encoding": "gzip" });
			res.end(gzipSync("Hello compressed World!"));
			return;
		case "/html":
			res.writeHead(200, { "Content-Type": "text/html" });
			res.end("<html><head><title>Hurl</title></head><body><h1>Hi</h1></body></html>");
			return;
		case "/basic-auth": {
			const expected = `Basic ${Buffer.from("bob:secret").toString("base64")}`;
			res.writeHead(req.headers.authorization === expected ? 200 : 401);
			res.end();
			return;
		}
		default:
			res.writeHead(404);
			res.end();
	}
}

export default async function ({ provide }: TestProject) {
	const server = createServer((req, res) => {
		handle(req, res).catch((e) => {
			res.writeHead(500);
			res.end(String(e));
		});
	});
	await new Promise<void>((resolve) => server.listen(0, "127.0.0.1", resolve));
	const { port } = server.address() as AddressInfo;
	provide("mockOrigin", `http://127.0.0.1:${port}`);
	return () => new Promise<void>((resolve) => server.close(() => resolve()));
}
