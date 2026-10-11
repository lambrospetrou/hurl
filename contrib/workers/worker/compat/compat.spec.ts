// Runs every Hurl file of `tests/` with hurl-wasm in workerd, and compares the JSON report with
// the one of the native Hurl CLI (run by `global-setup.ts`).
import { expect, inject, test } from "vitest";
import { MemoryFileSystem, runHurl } from "../src/hurl";
import { SECRETS, VARIABLES, compare, typedVariables } from "./compat";

const origin = inject("compatOrigin");
const files = Object.fromEntries(
	Object.entries(inject("compatFiles")).map(([path, data]) => [path, Uint8Array.from(atob(data), (c) => c.charCodeAt(0))]),
);

/** Returns the time of the Node server: time doesn't advance in workerd while the CPU runs. */
async function clock(): Promise<number> {
	const response = await fetch(`${origin}/__clock`);
	return ((await response.json()) as { now: number }).now;
}

for (const run of inject("compatCli")) {
	test(run.file, async () => {
		await fetch(`${origin}/__reset`, { method: "POST" });
		const start = await clock();
		const result = await runHurl(run.content, {
			filename: run.file,
			variables: typedVariables({ ...VARIABLES, origin }),
			secrets: SECRETS,
			files: new MemoryFileSystem(files),
		});
		const row = compare(run, result, (await clock()) - start);
		await fetch(`${origin}/__compat/row`, { method: "POST", body: JSON.stringify(row) });
		expect(row.leaks).toEqual([]);
		if (row.known) {
			// Remove the `# compat: known-difference` line once the builds agree.
			expect(row.compatible, `${run.file} is now compatible`).toBe(false);
		} else {
			expect(row.differences).toEqual([]);
			expect(result.success).toBe(row.cliSuccess);
		}
	});
}
