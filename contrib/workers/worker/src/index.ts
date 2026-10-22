import { parseHurl, runHurl } from "./hurl";

// POST /parse: checks the syntax of the Hurl file in the request body.
// POST /run?name=value: runs the Hurl file in the request body, with the query parameters as
// variables, and returns the Hurl JSON report.
export default {
	async fetch(request): Promise<Response> {
		const url = new URL(request.url);
		if (request.method !== "POST") {
			return new Response("POST a Hurl file to /parse or /run\n", { status: 405 });
		}
		const content = await request.text();
		switch (url.pathname) {
			case "/parse":
				return Response.json(await parseHurl(content));
			case "/run":
				return Response.json(await runHurl(content, { variables: Object.fromEntries(url.searchParams) }));
			default:
				return new Response("Not found\n", { status: 404 });
		}
	},
} satisfies ExportedHandler;
