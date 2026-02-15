# BoltFFI Justfile

set shell := ["bash", "-cu"]

default:
    @just --list

# ─────────────────────────────────────────────────────────────────────────────
# Setup
# ─────────────────────────────────────────────────────────────────────────────

# Install Rust targets for cross-compilation
setup-targets:
    rustup target add aarch64-apple-darwin x86_64-apple-darwin
    rustup target add aarch64-apple-ios aarch64-apple-ios-sim x86_64-apple-ios
    rustup target add aarch64-linux-android armv7-linux-androideabi x86_64-linux-android i686-linux-android

# Install development tools (cargo-insta, cargo-nextest)
setup-tools:
    cargo install cargo-insta cargo-nextest

# Full development setup
setup: setup-targets setup-tools

# Install boltffi CLI to ~/.cargo/bin
install:
    cargo install --path boltffi_cli --force

# ─────────────────────────────────────────────────────────────────────────────
# Build
# ─────────────────────────────────────────────────────────────────────────────

# Build boltffi CLI (debug)
build:
    cargo build -p boltffi_cli

# Build boltffi CLI (release)
build-release:
    cargo build -p boltffi_cli --release

# Build entire workspace (debug)
build-all:
    cargo build --workspace

# Build entire workspace (release)
build-all-release:
    cargo build --workspace --release

# ─────────────────────────────────────────────────────────────────────────────
# Test
# ─────────────────────────────────────────────────────────────────────────────

# Run all workspace tests
test:
    cargo test --workspace

# Run tests with cargo-nextest (parallel, faster)
test-nextest:
    cargo nextest run --workspace

# Run tests for a single crate
test-crate crate:
    cargo test -p {{crate}}

# Run bindgen snapshot tests only
test-snapshots:
    cargo test -p boltffi_bindgen

# Accept snapshot changes
snapshots-accept:
    cargo insta test --accept

# Run Miri for undefined behavior detection (requires nightly)
test-miri:
    cargo +nightly miri test -p boltffi -p boltffi_tests

# ─────────────────────────────────────────────────────────────────────────────
# Lint & Format
# ─────────────────────────────────────────────────────────────────────────────

# Check code formatting
fmt-check:
    cargo fmt --all -- --check

# Format all code
fmt:
    cargo fmt --all

# Run clippy lints
lint:
    cargo clippy --workspace --all-targets -- -D warnings

# Run format check + clippy + tests
check: fmt-check lint test

# ─────────────────────────────────────────────────────────────────────────────
# Benchmarks
# ─────────────────────────────────────────────────────────────────────────────

# Swift benchmark (macOS CLI) - builds xcframework and runs benchmark
bench-swift:
    #!/usr/bin/env bash
    set -e
    tmpfile=$(mktemp /tmp/boltffi_bench_swift_XXXXXX.txt)
    trap "rm -f $tmpfile" EXIT
    cd benchmarks/rust-boltffi
    ./build.sh --platform apple --release 2>&1 | tee "$tmpfile"
    echo ""
    echo "=== Summary ==="
    python3 ../swift-macos-bench/format_bench.py < "$tmpfile"

# Kotlin benchmark (JVM via JMH) - builds JNI libs, runs JMH, generates report
bench-kotlin:
    #!/usr/bin/env bash
    set -e
    echo "=== Building BoltFFI for Android (Kotlin bindings + JNI glue) ==="
    cd benchmarks/rust-boltffi && ./build.sh --platform android --release --skip-bench
    
    echo "=== Building UniFFI Kotlin baseline ==="
    cd ../rust-uniffi && ./build-kotlin.sh
    
    echo "=== Building desktop JNI library ==="
    cd ../kotlin-jvm-bench && ./build-jni.sh
    
    echo "=== Running JMH benchmarks ==="
    ./gradlew jmh
    
    echo "=== Generating report ==="
    python3 jmh_report.py --format both
    echo ""
    echo "Report: $(pwd)/build/results/jmh/report.txt"

# Build xcframework only (for iOS development in Xcode)
bench-build-ios:
    cd benchmarks/rust-boltffi && ./build.sh --platform apple --release --skip-bench
    @echo ""
    @echo "xcframework ready. Open benchmarks/ios-app/ in Xcode."

# Build jniLibs only (for Android development in Android Studio)
bench-build-android:
    cd benchmarks/rust-boltffi && ./build.sh --platform android --release --skip-bench
    @echo ""
    @echo "jniLibs ready. Open benchmarks/android-app/ in Android Studio."

# WASM benchmark (Node.js) - builds wasm, runs benchmark
bench-wasm:
    #!/usr/bin/env bash
    set -e
    echo "=== Building BoltFFI WASM ==="
    cd benchmarks/rust-boltffi && cargo run -p boltffi_cli --manifest-path ../../Cargo.toml -- pack wasm --release --regenerate
    
    echo "=== Building wasm-bindgen baseline ==="
    cd ../rust-wasm-bindgen && cargo build --target wasm32-unknown-unknown --release
    wasm-bindgen --target experimental-nodejs-module --out-dir dist target/wasm32-unknown-unknown/release/bench_wasm_bindgen.wasm
    
    echo "=== Copying to benchmark runner ==="
    cp -r ../rust-boltffi/dist/wasm/pkg/* ../wasm-bench/boltffi/
    cp -r dist/* ../wasm-bench/wasmbindgen/
    
    echo "=== Running benchmarks ==="
    cd ../wasm-bench && node bench.mjs

# ─────────────────────────────────────────────────────────────────────────────
# Clean
# ─────────────────────────────────────────────────────────────────────────────

# Clean workspace target/
clean:
    cargo clean

# Clean benchmark build artifacts
clean-benchmarks:
    rm -rf benchmarks/rust-boltffi/target
    rm -rf benchmarks/rust-boltffi/dist
    rm -rf benchmarks/rust-uniffi/target
    rm -rf benchmarks/rust-uniffi/dist
    rm -rf benchmarks/rust-wasm-bindgen/target
    rm -rf benchmarks/rust-wasm-bindgen/dist
    rm -rf benchmarks/swift-macos-bench/.build
    rm -rf benchmarks/kotlin-jvm-bench/build
    rm -rf benchmarks/wasm-bench/boltffi
    rm -rf benchmarks/wasm-bench/wasmbindgen
    rm -rf benchmarks/wasm-bench/node_modules

# Clean everything
clean-all: clean clean-benchmarks

# ─────────────────────────────────────────────────────────────────────────────
# CI
# ─────────────────────────────────────────────────────────────────────────────

# Run CI checks locally (format + lint + test)
ci: fmt-check lint test

# Run full CI including Miri (slow)
ci-full: ci test-miri
