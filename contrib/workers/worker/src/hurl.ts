// Runs Hurl files with the Hurl WebAssembly module (see ../../hurl_wasm).
//
// The module is synchronous Rust code. Its async imports (HTTP requests, sleep, files) suspend the
// WebAssembly stack with JSPI: `WebAssembly.Suspending` wraps the imports and
// `WebAssembly.promising` wraps the `hurl_run` export.
import { Buffer } from "node:buffer";
import hurlWasm from "./hurl_wasm.wasm";

const encoder = new TextEncoder();
const decoder = new TextDecoder();

// JSPI is not in the TypeScript WebAssembly types yet.
const jspi = WebAssembly as unknown as {
	Suspending: new (fn: (...args: never[]) => Promise<unknown>) => object;
	promising: <A extends unknown[], R>(fn: (...args: A) => R) => (...args: A) => Promise<R>;
};

interface HurlExports {
	memory: WebAssembly.Memory;
	alloc(len: number): number;
	dealloc(ptr: number, len: number): void;
	hurl_parse(ptr: number, len: number): bigint;
	hurl_fmt(ptr: number, len: number): bigint;
	hurl_run(ptr: number, len: number): bigint;
}

interface HostRequest {
	method: string;
	url: string;
	headers: [string, string][];
	body: string;
	timeout_ms: number;
}

type HostResponse = { status: number; headers: [string, string][]; body: string } | { error: string };

interface FileRequest {
	path: string;
	body?: string;
	append?: boolean;
}

type FileResponse = { body?: string } | { error: string } | { not_found: true };

export type FileContent = Uint8Array | ReadableStream<Uint8Array>;

/** Default of `RunOptions.maxFileSize`. */
export const DEFAULT_MAX_FILE_SIZE = 16 * 1024 * 1024;

/**
 * Files used by Hurl: request bodies (`file,data.bin;`), `[Multipart]` files, `file,...;`
 * predicates (read), and the `output:` option (write). Paths are relative to the file root;
 * Hurl refuses paths outside it (`../x`, `/x`) before calling these methods.
 */
export interface HurlFileSystem {
	/**
	 * Returns the file content, or `undefined` if the file does not exist. A stream is read until
	 * `maxSize` bytes and then canceled; for example, return the `body` of an R2 object.
	 */
	read(path: string, options: { maxSize: number }): Promise<FileContent | undefined> | FileContent | undefined;
	/** Writes the file. If `append` is false, the file is replaced. */
	write?(path: string, data: Uint8Array, append: boolean): Promise<void> | void;
}

/** A file system in memory, for example filled from R2 or KV before a run. */
export class MemoryFileSystem implements HurlFileSystem {
	readonly files: Map<string, Uint8Array>;

	constructor(files: Record<string, string | Uint8Array> = {}) {
		this.files = new Map(
			Object.entries(files).map(([path, data]) => [path, typeof data === "string" ? encoder.encode(data) : data]),
		);
	}

	read(path: string): Uint8Array | undefined {
		return this.files.get(path);
	}

	write(path: string, data: Uint8Array, append: boolean): void {
		const previous = append ? this.files.get(path) : undefined;
		this.files.set(path, previous ? Buffer.concat([previous, data]) : data);
	}

	/** Returns the content of a file as UTF-8 text. */
	text(path: string): string | undefined {
		const data = this.files.get(path);
		return data && decoder.decode(data);
	}
}

export interface ParseResult {
	ok: boolean;
	entries?: number;
	error?: { line: number; column: number; description: string; message: string };
}

export interface FormatOptions {
	/** `hurlfmt --in`: a Hurl file, or a curl command line (one per line). Default: `hurl`. */
	inputFormat?: "hurl" | "curl";
	/** `hurlfmt --out`: formatted Hurl, the JSON of the file, or HTML. Default: `hurl`. */
	outputFormat?: "hurl" | "json" | "html";
	/** `hurlfmt --standalone`: a full HTML page, with the CSS. */
	standalone?: boolean;
}

export interface FormatResult {
	ok: boolean;
	output?: string;
	/** Like `hurlfmt --check`: the Hurl input is already formatted (null for curl input). */
	formatted?: boolean | null;
	/** A parse error has the same fields as in `ParseResult`; a curl error only has `message`. */
	error?: { message: string; line?: number; column?: number; description?: string };
}

export interface RunResult {
	ok: boolean;
	success?: boolean;
	/** The Hurl JSON report of the file (see https://hurl.dev/docs/running-tests.html#json-report). */
	result?: HurlJsonReport;
	/** The errors of the entries (failed asserts, file access, HTTP), rendered like the Hurl CLI. */
	errors?: RunError[];
	/** The failed calls to `RunOptions.files`, with their reason (Hurl only reports the path). */
	fileErrors?: FileError[];
	/** Why the response bodies could not be saved to `responseDir`; `result` has no bodies then. */
	reportError?: string;
	/** The features of the file that `fetch` can not do (see `RunOptions.unsupported`). */
	unsupported?: UnsupportedFeature[];
	error?: string;
}

export interface UnsupportedFeature {
	entry: number;
	line: number;
	column: number;
	/** For example "option `http2`", "`HTTP/2`" or "query `ip`". */
	feature: string;
	message: string;
}

export interface FileError {
	path: string;
	operation: "read" | "write";
	message: string;
}

export interface RunError {
	entry: number;
	line: number;
	column: number;
	assert: boolean;
	description: string;
	message: string;
}

export interface HurlJsonReport {
	filename: string;
	success: boolean;
	time: number;
	entries: {
		index: number;
		line: number;
		calls: { request: { method: string; url: string }; response: { status: number } }[];
		captures?: { name: string; value: unknown }[];
		asserts: { line: number; success: boolean; message?: string }[];
		time: number;
	}[];
	[key: string]: unknown;
}

/**
 * Options of a run, like the Hurl CLI options with the same names (`maxRedirs` is `--max-redirs`).
 * The `[Options]` section of an entry overrides them. Use -1 for no limit in counts.
 */
export interface HurlOptions {
	location?: boolean;
	locationTrusted?: boolean;
	maxRedirs?: number;
	retry?: number;
	retryIntervalMs?: number;
	delayMs?: number;
	maxTimeMs?: number;
	/** `user:password` for basic authentication. */
	user?: string;
	userAgent?: string;
	/** Headers added to each request, as `Name: value`. */
	headers?: string[];
	compressed?: boolean;
	continueOnError?: boolean;
	ignoreAsserts?: boolean;
	fromEntry?: number;
	toEntry?: number;
	repeat?: number;
	maxFilesize?: number;
	/**
	 * Size in bytes of the request and response bodies kept for the report (not a CLI option).
	 * The bodies of an entry are used by its asserts, captures and output, then kept only if they
	 * fit in the remaining size. Default: all the bodies are kept.
	 */
	maxRetainedBodySize?: number;
}

export interface RunOptions {
	variables?: Record<string, string | number | boolean | null>;
	/**
	 * Secret variables, like `--secret name=value`: their values, and those of the `redact`
	 * captures, are replaced by `***` in `result` and `errors`.
	 */
	secrets?: Record<string, string>;
	filename?: string;
	/** Hurl options of the run. */
	options?: HurlOptions;
	/** Sends the HTTP requests of the run (default: the global `fetch`). */
	fetcher?: typeof fetch;
	/**
	 * What to do with the features of the file that `fetch` can not do: libcurl `[Options]`
	 * (`http2`, `ipv6`, `insecure`, `proxy`, ...), `HTTP/1.0`, `HTTP/2` or `HTTP/3` in a status
	 * line, and the `ip` and `certificate` queries. They are listed in `RunResult.unsupported`.
	 * - `"error"` (default): the run fails before sending any request.
	 * - `"ignore"`: the options are ignored (`fetch` uses its defaults), `HTTP/x` in a status
	 *   line matches any version, and the queries are evaluated (`ip` is an empty string).
	 */
	unsupported?: "error" | "ignore";
	/** Files read and written by the run (default: none, file access fails). */
	files?: HurlFileSystem;
	/**
	 * Maximum size in bytes of a file read or written by the run (default:
	 * `DEFAULT_MAX_FILE_SIZE`). Hurl keeps files in WebAssembly memory, thus a larger file fails
	 * the entry instead of using the memory of the Worker.
	 */
	maxFileSize?: number;
	/**
	 * Saves the response bodies of the report to `files`, like the `store` directory of the CLI
	 * `--report-json`: `<responseDir>/<uuid>_response.<ext>`, referenced in the report as
	 * `<last directory of responseDir>/<uuid>_response.<ext>`. Default: bodies are not saved.
	 */
	responseDir?: string;
}

function tooLarge(path: string, maxSize: number): Error {
	return new Error(`file ${path} is larger than the limit of ${maxSize} bytes`);
}

async function readAll(path: string, content: FileContent, maxSize: number): Promise<Uint8Array> {
	if (content instanceof Uint8Array) {
		if (content.byteLength > maxSize) {
			throw tooLarge(path, maxSize);
		}
		return content;
	}
	const reader = content.getReader();
	const chunks: Uint8Array[] = [];
	let size = 0;
	for (;;) {
		const { done, value } = await reader.read();
		if (done) {
			return Buffer.concat(chunks, size);
		}
		size += value.byteLength;
		if (size > maxSize) {
			await reader.cancel(tooLarge(path, maxSize).message);
			throw tooLarge(path, maxSize);
		}
		chunks.push(value);
	}
}

async function send(fetcher: typeof fetch, request: HostRequest): Promise<HostResponse> {
	try {
		const headers = new Headers();
		for (const [name, value] of request.headers) {
			headers.append(name, value);
		}
		// `fetch` refuses a body with GET and HEAD.
		const hasBody = request.body !== "" && request.method !== "GET" && request.method !== "HEAD";
		const response = await fetcher(request.url, {
			method: request.method,
			headers,
			body: hasBody ? Buffer.from(request.body, "base64") : undefined,
			redirect: "manual",
			signal: AbortSignal.timeout(request.timeout_ms),
		});
		const responseHeaders: [string, string][] = [];
		response.headers.forEach((value, name) => {
			// workerd already decodes gzip and br bodies but keeps the Content-Encoding header.
			if (name === "content-encoding" && (value === "gzip" || value === "br")) {
				return;
			}
			if (name !== "set-cookie") {
				responseHeaders.push([name, value]);
			}
		});
		for (const cookie of response.headers.getSetCookie()) {
			responseHeaders.push(["set-cookie", cookie]);
		}
		const body = Buffer.from(await response.arrayBuffer()).toString("base64");
		return { status: response.status, headers: responseHeaders, body };
	} catch (e) {
		return { error: e instanceof Error ? e.message : String(e) };
	}
}

async function readFile(
	files: HurlFileSystem | undefined,
	request: FileRequest,
	maxSize: number,
): Promise<FileResponse> {
	try {
		const content = await files?.read(request.path, { maxSize });
		if (content === undefined) {
			return { not_found: true };
		}
		const data = await readAll(request.path, content, maxSize);
		return { body: Buffer.from(data.buffer, data.byteOffset, data.byteLength).toString("base64") };
	} catch (e) {
		return { error: e instanceof Error ? e.message : String(e) };
	}
}

async function writeFile(
	files: HurlFileSystem | undefined,
	request: FileRequest,
	maxSize: number,
): Promise<FileResponse> {
	if (!files?.write) {
		return { error: "file writes are not supported by this host" };
	}
	try {
		const data = new Uint8Array(Buffer.from(request.body ?? "", "base64"));
		if (data.byteLength > maxSize) {
			throw tooLarge(request.path, maxSize);
		}
		await files.write(request.path, data, request.append ?? false);
		return {};
	} catch (e) {
		return { error: e instanceof Error ? e.message : String(e) };
	}
}

async function instantiate(
	fetcher: typeof fetch,
	files?: HurlFileSystem,
	maxFileSize = DEFAULT_MAX_FILE_SIZE,
	fileErrors: FileError[] = [],
): Promise<HurlExports> {
	let exports: HurlExports | undefined;
	let pending: Uint8Array = new Uint8Array();
	const memory = () => new Uint8Array(exports!.memory.buffer);
	const imports = {
		hurl_host: {
			http_send: new jspi.Suspending(async (ptr: number, len: number) => {
				const request = JSON.parse(decoder.decode(memory().slice(ptr, ptr + len))) as HostRequest;
				pending = encoder.encode(JSON.stringify(await send(fetcher, request)));
				return pending.length;
			}),
			fs_read: new jspi.Suspending(async (ptr: number, len: number) => {
				const request = JSON.parse(decoder.decode(memory().slice(ptr, ptr + len))) as FileRequest;
				const response = await readFile(files, request, maxFileSize);
				if ("error" in response) {
					fileErrors.push({ path: request.path, operation: "read", message: response.error });
				}
				pending = encoder.encode(JSON.stringify(response));
				return pending.length;
			}),
			fs_write: new jspi.Suspending(async (ptr: number, len: number) => {
				const request = JSON.parse(decoder.decode(memory().slice(ptr, ptr + len))) as FileRequest;
				const response = await writeFile(files, request, maxFileSize);
				if ("error" in response) {
					fileErrors.push({ path: request.path, operation: "write", message: response.error });
				}
				pending = encoder.encode(JSON.stringify(response));
				return pending.length;
			}),
			take_response: (ptr: number) => {
				memory().set(pending, ptr);
				pending = new Uint8Array();
			},
			now_ms: () => Date.now(),
			sleep_ms: new jspi.Suspending((ms: number) => new Promise((resolve) => setTimeout(resolve, ms))),
			random_fill: (ptr: number, len: number) => {
				// getRandomValues fills at most 65536 bytes per call.
				for (let offset = 0; offset < len; offset += 65536) {
					crypto.getRandomValues(memory().subarray(ptr + offset, ptr + Math.min(len, offset + 65536)));
				}
			},
		},
	};
	// libxml2 is C code linked with wasi-libc: it only needs stderr and the clock, with no preopened
	// directories, so every other WASI call fails with EBADF.
	const EBADF = 8;
	const view = () => new DataView(exports!.memory.buffer);
	const wasi = {
		fd_write: (_fd: number, iovs: number, iovsLen: number, nwritten: number) => {
			let total = 0;
			for (let i = 0; i < iovsLen; i++) {
				total += view().getUint32(iovs + i * 8 + 4, true);
			}
			view().setUint32(nwritten, total, true);
			return 0;
		},
		clock_time_get: (_id: number, _precision: bigint, time: number) => {
			view().setBigUint64(time, BigInt(Date.now()) * 1_000_000n, true);
			return 0;
		},
		random_get: (ptr: number, len: number) => {
			imports.hurl_host.random_fill(ptr, len);
			return 0;
		},
		fd_close: () => EBADF,
		fd_fdstat_get: () => EBADF,
		fd_prestat_get: () => EBADF,
		fd_prestat_dir_name: () => EBADF,
		fd_read: () => EBADF,
		fd_seek: () => EBADF,
		path_open: () => EBADF,
		proc_exit: (code: number) => {
			throw new Error(`libxml2 called exit(${code})`);
		},
	};
	const instance = await WebAssembly.instantiate(hurlWasm, {
		...imports,
		wasi_snapshot_preview1: wasi,
	} as unknown as WebAssembly.Imports);
	exports = instance.exports as unknown as HurlExports;
	// Runs the C constructors of wasi-libc and libxml2 once (WASI reactor modules export it).
	(instance.exports._initialize as (() => void) | undefined)?.();
	return exports;
}

async function invoke<T>(
	exports: HurlExports,
	fn: (ptr: number, len: number) => bigint | Promise<bigint>,
	input: string,
): Promise<T> {
	const bytes = encoder.encode(input);
	const ptr = exports.alloc(bytes.length);
	new Uint8Array(exports.memory.buffer).set(bytes, ptr);
	const packed = await fn(ptr, bytes.length);
	const outPtr = Number(packed >> 32n);
	const outLen = Number(packed & 0xffffffffn);
	const output = decoder.decode(new Uint8Array(exports.memory.buffer, outPtr, outLen));
	exports.dealloc(outPtr, outLen);
	return JSON.parse(output) as T;
}

/** Parses a Hurl file and reports the syntax error, if any. */
export async function parseHurl(content: string): Promise<ParseResult> {
	const exports = await instantiate(fetch);
	return invoke<ParseResult>(exports, exports.hurl_parse, content);
}

/** Formats a Hurl file, or exports it to JSON or HTML, like `hurlfmt`. */
export async function formatHurl(content: string, options: FormatOptions = {}): Promise<FormatResult> {
	const exports = await instantiate(fetch);
	return invoke<FormatResult>(exports, exports.hurl_fmt, JSON.stringify({ content, ...options }));
}

/** Runs a Hurl file. Each run has its own WebAssembly instance, thus its own cookie store. */
export async function runHurl(content: string, options: RunOptions = {}): Promise<RunResult> {
	const fileErrors: FileError[] = [];
	const exports = await instantiate(options.fetcher ?? fetch, options.files, options.maxFileSize, fileErrors);
	const input = JSON.stringify({
		content,
		filename: options.filename,
		variables: options.variables ?? {},
		secrets: options.secrets ?? {},
		options: options.options ?? {},
		response_dir: options.responseDir,
		unsupported: options.unsupported,
	});
	const result = await invoke<RunResult>(exports, jspi.promising(exports.hurl_run), input);
	if (fileErrors.length > 0) {
		result.fileErrors = fileErrors;
	}
	return result;
}
