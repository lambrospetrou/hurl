import { cloudflareTest } from "@cloudflare/vitest-plugin";
import { defineConfig } from "vitest/config";

export default defineConfig({
	plugins: [
		cloudflareTest({
			wrangler: { configPath: "./wrangler.jsonc" },
		}),
	],
	test: {
		include: ["compat/**/*.spec.ts"],
		globalSetup: ["./compat/global-setup.ts"],
		testTimeout: 120_000,
		hookTimeout: 600_000,
	},
});
