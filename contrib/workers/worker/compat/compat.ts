// Shared by the Node global setup and the workerd spec: inputs of the runs, normalization of the
// JSON reports, comparison and the Markdown table.

/** Variables of every run, like `--variable name=value` (the `origin` of the server is added). */
export const VARIABLES: Record<string, string> = {
	int_var: "3",
	float_var: "2.5",
	bool_var: "true",
	null_var: "null",
	str_var: "hello world",
};

/**
 * Returns the variables typed like the CLI types `--variable` values: `true`, `false`, `null`,
 * numbers and `"quoted"` strings. hurl-wasm takes JS values, so it keeps strings as strings.
 */
export function typedVariables(variables: Record<string, string>): Record<string, string | number | boolean | null> {
	const typed = (s: string) => {
		if (s === "true" || s === "false") return s === "true";
		if (s === "null") return null;
		if (/^-?\d+(\.\d+)?([eE][-+]?\d+)?$/.test(s)) return Number(s);
		return s.length >= 2 && s.startsWith('"') && s.endsWith('"') ? s.slice(1, -1) : s;
	};
	return Object.fromEntries(Object.entries(variables).map(([name, value]) => [name, typed(value)]));
}

/** Secrets of every run, like `--secret name=value`. */
export const SECRETS: Record<string, string> = { api_key: "sk-live-4f3c2b1a9e8d" };

/** Values that must never appear in a report: the secrets and the values captured with `redact`. */
export const SENSITIVE = [SECRETS.api_key, "tok-9f8e7d6c5b4a", "csrf-31415926"];

export interface CliRun {
	file: string;
	content: string;
	exitCode: number;
	stderr: string;
	/** The first (and only) file of `report.json`, or null if the CLI wrote no report. */
	report: any;
	ms: number;
}

export interface CompatRow {
	file: string;
	entries: number;
	cliSuccess: boolean;
	wasmSuccess: boolean | null;
	compatible: boolean;
	/** Set by a `# compat: known-difference <reason>` line: the file is expected to differ. */
	known: string | null;
	differences: string[];
	leaks: string[];
	cliMs: number;
	wasmMs: number | null;
	cli: unknown;
	wasm: unknown;
}

/** Fields that change from run to run, or that only one build can know. */
const IGNORED_FIELDS = new Set(["time", "timings", "body", "certificate", "ip", "filename"]);
/** Request headers added by the HTTP client itself (libcurl or fetch), not by the Hurl file. */
const IGNORED_REQUEST_HEADERS = new Set(["host", "accept", "user-agent", "accept-encoding", "content-length"]);
/** Response headers added by the Node server that depend on the connection or the time. */
const IGNORED_RESPONSE_HEADERS = new Set(["date", "connection", "keep-alive", "transfer-encoding"]);

/** Multipart boundaries are random, and libcurl and hurl-wasm make them with different lengths. */
const MULTIPART_BOUNDARY = /-{20,}[0-9a-f]{16,32}/g;

/** Returns the parts of a Hurl JSON report that both builds must produce identically. */
export function normalize(report: any): unknown {
	const strip = (value: any, ignoredHeaders?: Set<string>): any => {
		if (Array.isArray(value)) return value.map((v) => strip(v));
		if (typeof value === "string") return value.replace(MULTIPART_BOUNDARY, "<boundary>");
		if (value === null || typeof value !== "object") return value;
		const result: Record<string, unknown> = {};
		for (const [key, v] of Object.entries(value)) {
			if (IGNORED_FIELDS.has(key)) continue;
			if (key === "request") result[key] = stripHeaders(strip(v), IGNORED_REQUEST_HEADERS);
			else if (key === "response") result[key] = stripHeaders(strip(v), IGNORED_RESPONSE_HEADERS);
			else result[key] = strip(v, ignoredHeaders);
		}
		return result;
	};
	return strip(report);
}

/**
 * Removes the ignored headers, and sorts the others by lowercase name: fetch lowercases and sorts
 * the response headers, and libcurl and fetch add the request headers in different orders.
 */
function stripHeaders(message: any, ignored: Set<string>): any {
	if (Array.isArray(message?.headers)) {
		message.headers = message.headers
			.map((h: { name: string; value: string }) => ({ ...h, name: h.name.toLowerCase() }))
			.filter((h: { name: string }) => !ignored.has(h.name))
			.sort((a: { name: string }, b: { name: string }) => (a.name < b.name ? -1 : a.name > b.name ? 1 : 0));
	}
	return message;
}

/** Returns the reason of a `# compat: known-difference <reason>` line of a Hurl file. */
export function knownDifference(content: string): string | null {
	const match = content.match(/^# compat: known-difference\b(.*)$/m);
	return match ? match[1].trim() || "known difference" : null;
}

/** Returns the differences between two JSON values, as `path: cli=... wasm=...` lines. */
export function diff(cli: unknown, wasm: unknown, path = "$", out: string[] = []): string[] {
	if (JSON.stringify(cli) === JSON.stringify(wasm)) return out;
	// Cookie expiry dates from `Max-Age` depend on the time of the run.
	if (path.endsWith(".expires") && typeof cli === "number" && typeof wasm === "number" && Math.abs(cli - wasm) <= 60) {
		return out;
	}
	const isObject = (v: unknown) => v !== null && typeof v === "object";
	if (isObject(cli) && isObject(wasm) && Array.isArray(cli) === Array.isArray(wasm)) {
		const keys = new Set([...Object.keys(cli as object), ...Object.keys(wasm as object)]);
		for (const key of keys) {
			const child = Array.isArray(cli) ? `${path}[${key}]` : `${path}.${key}`;
			diff((cli as any)[key], (wasm as any)[key], child, out);
		}
		return out;
	}
	out.push(`${path}: cli=${JSON.stringify(cli)} wasm=${JSON.stringify(wasm)}`);
	return out;
}

/** Returns the paths of `value` where a sensitive value appears. */
export function findLeaks(value: unknown, path = "$", out: string[] = []): string[] {
	if (typeof value === "string") {
		if (SENSITIVE.some((s) => value.includes(s))) out.push(path);
	} else if (Array.isArray(value)) {
		value.forEach((v, i) => findLeaks(v, `${path}[${i}]`, out));
	} else if (value !== null && typeof value === "object") {
		for (const [key, v] of Object.entries(value)) findLeaks(v, `${path}.${key}`, out);
	}
	return out;
}

export function compare(run: CliRun, wasm: { success?: boolean; errors?: unknown; result?: unknown }, wasmMs: number): CompatRow {
	const cli = run.report === null ? null : normalize(run.report);
	const wasmReport = wasm.result === undefined ? null : normalize(wasm.result);
	const differences =
		cli === null
			? [`the CLI wrote no report (exit code ${run.exitCode}): ${run.stderr.trim().split("\n")[0]}`]
			: wasmReport === null
				? [`hurl-wasm returned no report: ${JSON.stringify(wasm.errors)}`]
				: diff(cli, wasmReport);
	const leaks = [
		...findLeaks(run.report, "cli.report"),
		...findLeaks(run.stderr, "cli.stderr"),
		...findLeaks(wasm.result, "wasm.report"),
		...findLeaks(wasm.errors, "wasm.errors"),
	];
	return {
		file: run.file,
		entries: run.report?.entries?.length ?? 0,
		cliSuccess: run.exitCode === 0,
		wasmSuccess: wasm.success ?? false,
		compatible: differences.length === 0 && (run.exitCode === 0) === (wasm.success ?? false),
		known: knownDifference(run.content),
		differences,
		leaks,
		cliMs: run.ms,
		wasmMs,
		cli,
		wasm: wasmReport,
	};
}

const MAX_DIFFERENCES = 10;

/** Renders the rows, already sorted by file name, as a Markdown report. */
export function renderReport(rows: CompatRow[], info: { hurl: string; sensitive: number }): string {
	const status = (success: boolean | null) => (success === null ? "–" : success ? "pass" : "fail");
	const ms = (value: number | null) => (value === null ? "–" : value.toFixed(0));
	const compatible = rows.filter((r) => r.compatible).length;
	const known = rows.filter((r) => !r.compatible && r.known).length;
	const redacted = rows.filter((r) => r.leaks.length === 0).length;
	const lines = [
		"# Hurl CLI vs hurl-wasm compatibility",
		"",
		`Hurl CLI: \`${info.hurl}\`. Compatible: ${compatible}/${rows.length} files (${known} known differences). Redacted: ${redacted}/${rows.length} files (${info.sensitive} sensitive values checked).`,
		"",
		"| File | Entries | CLI | hurl-wasm | Compatible | Redacted | Differences | CLI ms | hurl-wasm ms |",
		"|---|---:|---|---|---|---|---:|---:|---:|",
		...rows.map(
			(r) =>
				`| ${r.file} | ${r.entries} | ${status(r.cliSuccess)} | ${status(r.wasmSuccess)} | ${r.compatible ? "yes" : r.known ? "known" : "**NO**"} | ${r.leaks.length === 0 ? "yes" : "**NO**"} | ${r.differences.length} | ${ms(r.cliMs)} | ${ms(r.wasmMs)} |`,
		),
		"",
	];
	for (const r of rows.filter((r) => r.differences.length > 0 || r.leaks.length > 0)) {
		lines.push(`## ${r.file}`, "");
		if (r.known) lines.push(`Known difference: ${r.known}`, "");
		for (const d of r.differences.slice(0, MAX_DIFFERENCES)) lines.push(`- ${d.replaceAll("|", "\\|")}`);
		if (r.differences.length > MAX_DIFFERENCES) lines.push(`- … ${r.differences.length - MAX_DIFFERENCES} more`);
		for (const l of r.leaks) lines.push(`- leak: sensitive value in \`${l}\``);
		lines.push("");
	}
	return `${lines.join("\n")}\n`;
}
