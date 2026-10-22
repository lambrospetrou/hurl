// Runs in Node, before the compat spec runs in workerd: starts the HTTP server, runs every Hurl
// file with the native Hurl CLI, and at the end writes the comparison table.
import { execFile } from "node:child_process";
import { existsSync, mkdtempSync, mkdirSync, readFileSync, readdirSync, rmSync, statSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, relative, resolve } from "node:path";
import { performance } from "node:perf_hooks";
import type { TestProject } from "vitest/node";
import { SECRETS, SENSITIVE, VARIABLES, renderReport, type CliRun, type CompatRow } from "./compat";
import { startServer, takeRows } from "./server";

declare module "vitest" {
	export interface ProvidedContext {
		compatOrigin: string;
		compatCli: CliRun[];
		/** Files available to the Hurl files, relative to `tests/`, base64 encoded. */
		compatFiles: Record<string, string>;
	}
}

const COMPAT_DIR = resolve(import.meta.dirname);
const TESTS_DIR = join(COMPAT_DIR, "tests");
const OUT_DIR = join(COMPAT_DIR, "out");

function hurlBin(): string {
	if (process.env.HURL_BIN) return process.env.HURL_BIN;
	const local = resolve(COMPAT_DIR, "../../../../target/release/hurl");
	return existsSync(local) ? local : "hurl";
}

function listFiles(dir: string): string[] {
	return readdirSync(dir).flatMap((name) => {
		const path = join(dir, name);
		return statSync(path).isDirectory() ? listFiles(path) : [path];
	});
}

function runCli(bin: string, args: string[]): Promise<{ code: number; stderr: string }> {
	return new Promise((resolve) => {
		execFile(bin, args, { cwd: TESTS_DIR, maxBuffer: 64 * 1024 * 1024 }, (error, _stdout, stderr) => {
			const code = error ? (typeof error.code === "number" ? error.code : -1) : 0;
			resolve({ code, stderr: error && typeof error.code !== "number" ? String(error) : stderr });
		});
	});
}

export default async function setup(project: TestProject) {
	const { server, origin } = await startServer();
	const bin = hurlBin();
	const filter = process.env.COMPAT_FILTER;
	const hurlFiles = readdirSync(TESTS_DIR)
		.filter((name) => name.endsWith(".hurl") && (!filter || name.includes(filter)))
		.sort();

	const variables = { ...VARIABLES, origin };
	const variableArgs = [
		...Object.entries(variables).flatMap(([name, value]) => ["--variable", `${name}=${value}`]),
		...Object.entries(SECRETS).flatMap(([name, value]) => ["--secret", `${name}=${value}`]),
	];
	const reportsDir = mkdtempSync(join(tmpdir(), "hurl-compat-"));
	const cli: CliRun[] = [];
	for (const file of hurlFiles) {
		await fetch(`${origin}/__reset`, { method: "POST" });
		const reportDir = join(reportsDir, file);
		const start = performance.now();
		const { code, stderr } = await runCli(bin, ["--no-color", "--report-json", reportDir, ...variableArgs, file]);
		const ms = performance.now() - start;
		const reportPath = join(reportDir, "report.json");
		const report = existsSync(reportPath) ? JSON.parse(readFileSync(reportPath, "utf8"))[0] : null;
		cli.push({ file, content: readFileSync(join(TESTS_DIR, file), "utf8"), exitCode: code, stderr, report, ms });
	}
	rmSync(reportsDir, { recursive: true, force: true });

	const files: Record<string, string> = {};
	for (const path of listFiles(TESTS_DIR).filter((p) => !p.endsWith(".hurl"))) {
		files[relative(TESTS_DIR, path)] = readFileSync(path).toString("base64");
	}

	project.provide("compatOrigin", origin);
	project.provide("compatCli", cli);
	project.provide("compatFiles", files);

	return async () => {
		server.close();
		const rows = new Map(takeRows().map((row: CompatRow) => [row.file, row]));
		const sorted = cli.map((run) => rows.get(run.file) ?? missingRow(run));
		mkdirSync(OUT_DIR, { recursive: true });
		for (const row of sorted) {
			if (row.cli) writeFileSync(join(OUT_DIR, `${row.file}.cli.json`), `${JSON.stringify(row.cli, null, 2)}\n`);
			const run = cli.find((r) => r.file === row.file);
			if (run?.stderr) writeFileSync(join(OUT_DIR, `${row.file}.cli.stderr`), run.stderr);
			if (row.wasm) writeFileSync(join(OUT_DIR, `${row.file}.wasm.json`), `${JSON.stringify(row.wasm, null, 2)}\n`);
		}
		const report = renderReport(sorted, { hurl: bin, sensitive: SENSITIVE.length });
		writeFileSync(join(OUT_DIR, "report.md"), report);
		console.log(`\n${report}\nWritten to ${relative(process.cwd(), join(OUT_DIR, "report.md"))}`);
	};
}

function missingRow(run: CliRun): CompatRow {
	return {
		file: run.file, entries: run.report?.entries?.length ?? 0, cliSuccess: run.exitCode === 0, wasmSuccess: null,
		compatible: false, known: null, differences: ["not run in hurl-wasm"], leaks: [], cliMs: run.ms, wasmMs: null, cli: null, wasm: null,
	};
}
