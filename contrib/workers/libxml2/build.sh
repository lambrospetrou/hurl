#!/bin/bash
# Builds libxml2 for WebAssembly, so that hurl_wasm has the XPath of the native CLI (same libxml2
# code). Writes, in work/ (or the directory given as argument):
#   - prefix/: libxml2, a static library built with wasi-sdk (wasi-libc);
#   - crates/libxml: the `libxml` crate patched with libxml-0.3.22-wasm.patch;
#   - wasm32_bindings.rs: the bindings of the crate for wasm32;
#   - cargo.toml: the Cargo config for hurl_wasm (`cargo build --config <work>/cargo.toml`).
# The steps already done are skipped. Runs on Linux (x86_64 or arm64), and needs curl, make, bindgen (`cargo install bindgen-cli`) and
# libclang (set LIBCLANG_PATH if bindgen does not find it).
set -euo pipefail
HERE=$(dirname "$(realpath "$0")")
WORK=$(realpath -m "${1:-$HERE/work}")
if [ "$(uname -s)" != Linux ]; then
	echo "build.sh runs on Linux only: use ../docker.sh on other systems" >&2
	exit 1
fi
case "$(uname -m)" in
x86_64) WASI_ARCH=x86_64 WASI_SHA256=52640dde13599bf127a95499e61d6d640256119456d1af8897ab6725bcf3d89c ;;
aarch64 | arm64) WASI_ARCH=arm64 WASI_SHA256=47fccad8b2498f2239e05e1115c3ffc652bf37e7de2f88fb64b2d663c976ce2d ;;
*)
	echo "No wasi-sdk for $(uname -m)" >&2
	exit 1
	;;
esac
WASI_SDK=$WORK/wasi-sdk-25.0-$WASI_ARCH-linux
SYSROOT=$WASI_SDK/share/wasi-sysroot
LIBXML2=$WORK/libxml2-2.13.8
PREFIX=$WORK/prefix
CRATE=$WORK/crates/libxml
mkdir -p "$WORK"
cd "$WORK"

download() { # url sha256
	local file
	file=$(basename "$1")
	curl -sSfL -o "$file.part" "$1"
	echo "$2  $file.part" | sha256sum -c --quiet -
	mv "$file.part" "$file"
}

if [ ! -d "$WASI_SDK" ]; then
	download "https://github.com/WebAssembly/wasi-sdk/releases/download/wasi-sdk-25/wasi-sdk-25.0-$WASI_ARCH-linux.tar.gz" \
		"$WASI_SHA256"
	tar xzf "wasi-sdk-25.0-$WASI_ARCH-linux.tar.gz" && rm "wasi-sdk-25.0-$WASI_ARCH-linux.tar.gz"
fi

if [ ! -f "$PREFIX/lib/libxml2.a" ]; then
	rm -rf "$LIBXML2"
	download https://download.gnome.org/sources/libxml2/2.13/libxml2-2.13.8.tar.xz \
		277294cb33119ab71b2bc81f2f445e9bc9435b893ad15bb2cd2b0e859a0ee84a
	tar xJf libxml2-2.13.8.tar.xz && rm libxml2-2.13.8.tar.xz
	# wasi-libc has no dup(). libxml2 only uses it on file descriptors, and Hurl gives it memory.
	printf '#pragma once\n#define dup(fd) (-1)\n' > wasi_compat.h
	(
		cd "$LIBXML2"
		./configure -q --host=wasm32-wasi --prefix="$PREFIX" \
			CC="$WASI_SDK/bin/clang --target=wasm32-wasip1 --sysroot=$SYSROOT" \
			AR="$WASI_SDK/bin/llvm-ar" RANLIB="$WASI_SDK/bin/llvm-ranlib" \
			CFLAGS="-O2 -include $WORK/wasi_compat.h" \
			--disable-shared --enable-static --without-threads --without-zlib --without-lzma \
			--without-iconv --without-icu --without-http --without-ftp --without-python \
			--without-readline --without-history --without-modules --without-catalog --without-debug
		make -s -j"$(nproc)" install >/dev/null
	)
fi

if [ ! -f "$CRATE/.patched" ]; then
	rm -rf "$CRATE" && mkdir -p "$(dirname "$CRATE")"
	download https://static.crates.io/crates/libxml/libxml-0.3.22.crate \
		1157d8bc7a14d07445541827e7a9c80fc5939718b43a973cf7e666d3ab5035fb
	tar xzf libxml-0.3.22.crate -C "$(dirname "$CRATE")" && rm libxml-0.3.22.crate
	mv "$(dirname "$CRATE")/libxml-0.3.22" "$CRATE"
	(cd "$CRATE" && patch -s -p1 < "$HERE/libxml-0.3.22-wasm.patch")
	touch "$CRATE/.patched"
fi

if [ ! -f "$WORK/wasm32_bindings.rs" ]; then
	# bindgen has no wasm32 target: a freestanding 32-bit target with the wasm32 type sizes and the
	# wasi-libc headers gives the same layout.
	(
		cd "$CRATE"
		bindgen src/wrapper.h --no-include-path-detection --opaque-type max_align_t \
			-o "$WORK/wasm32_bindings.rs.part" -- \
			--target=armv7-none-eabi -nostdinc -isystem "$WASI_SDK/lib/clang/19/include" \
			-isystem "$SYSROOT/include/wasm32-wasip1" -D__wasi__ -D__wasm__ -D__wasm32__ -D__wasm32 \
			-fsigned-char -U__SIZE_TYPE__ "-D__SIZE_TYPE__=unsigned long" \
			-U__PTRDIFF_TYPE__ "-D__PTRDIFF_TYPE__=long" -U__INTPTR_TYPE__ "-D__INTPTR_TYPE__=long" \
			-U__UINTPTR_TYPE__ "-D__UINTPTR_TYPE__=unsigned long" -U__WCHAR_TYPE__ "-D__WCHAR_TYPE__=int" \
			-I"$PREFIX/include/libxml2" 2> "$WORK/bindgen.log"
	)
	mv "$WORK/wasm32_bindings.rs.part" "$WORK/wasm32_bindings.rs"
fi

# wasi-libc is linked as a reactor (crt1-reactor.o, `_initialize`), so the exports do not run the
# C constructors and destructors on each call.
cat > "$WORK/cargo.toml" <<TOML
[env]
LIBXML2_BINDINGS = "$WORK/wasm32_bindings.rs"
LIBXML2_LIBDIR = "$PREFIX/lib"
# hurl's build.rs compiles native/libxml.c for the target with cc-rs.
CC_wasm32_unknown_unknown = "$WASI_SDK/bin/clang"
AR_wasm32_unknown_unknown = "$WASI_SDK/bin/llvm-ar"

[target.wasm32-unknown-unknown]
rustflags = [
    "-C", "link-arg=--no-entry",
    "-C", "link-arg=$SYSROOT/lib/wasm32-wasip1/crt1-reactor.o",
    "-C", "link-arg=-L$SYSROOT/lib/wasm32-wasip1",
    "-C", "link-arg=-lc",
    "-C", "link-arg=$WASI_SDK/lib/clang/19/lib/wasip1/libclang_rt.builtins-wasm32.a",
]
TOML
