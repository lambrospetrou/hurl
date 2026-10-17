import { describe, expect, it } from "vitest";

describe("workerd runtime", () => {
	it("supports JSPI, which the Hurl WebAssembly module needs for async HTTP and sleep", () => {
		const jspi = WebAssembly as unknown as Record<string, unknown>;
		expect(typeof jspi.Suspending).toBe("function");
		expect(typeof jspi.promising).toBe("function");
	});
});
