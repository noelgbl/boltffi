# `boltffi.toml` guide

This document is non-normative. It explains practical configuration patterns and output layouts. The schema, defaults, and validation rules live in [BOLTFFI_TOML_SPEC.md](BOLTFFI_TOML_SPEC.md).

## How to read this guide

- Use the spec for exact field behavior.
- Use this guide to choose target layouts and command flow.
- Keep one `boltffi.toml` that describes all targets you plan to ship.

## Cross-target workflow

Typical multi-target project flow:

1. Configure `[package]` and enabled `[targets.*]` sections.
2. Run platform builds with `boltffi build ...`.
3. Run binding generation with `boltffi generate ...`.
4. Run packaging with `boltffi pack ...`.

Common command sequence:

```bash
boltffi build apple
boltffi generate swift
boltffi pack apple

boltffi build android
boltffi generate kotlin
boltffi pack android

boltffi build wasm
boltffi generate typescript
boltffi pack wasm
```

## Apple: Swift Package layouts

`[targets.apple.spm].layout` controls where generated Swift goes and what `Package.swift` exposes.

### `layout = "ffi-only"`

Use this when you want one Swift Package that contains both:
- the binary target (`{XcframeworkName}FFI`)
- the Swift wrapper target (`{module_name}`)

Output shape:
- `{spm.output}/Package.swift`
- `{spm.output}/Sources/BoltFFIGenerated/{module_name}.swift`
- xcframework artifacts under `targets.apple.xcframework.output`

### `layout = "bundled"`

Use this when you already have wrapper sources and want generated code dropped into that package tree.

Key input:
- `spm.wrapper_sources` points to the wrapper source root
- if relative, it resolves from `spm.output`

Output shape:
- `{spm.output}/Package.swift`
- `{spm.output}/{spm.wrapper_sources}/BoltFFIGenerated/{module_name}.swift`
- xcframework artifacts under `targets.apple.xcframework.output`

### `layout = "split"`

Use this when binary distribution and wrapper distribution are intentionally separate.

Behavior:
- generated `Package.swift` exposes only binary target `{XcframeworkName}FFI`
- generated wrapper source is written outside the binary package tree

Output shape:
- `{spm.output}/Package.swift` (binary package)
- `{swift.output}/BoltFFIGenerated/{module_name}.swift` (wrapper source)
- xcframework artifacts under `targets.apple.xcframework.output`

### Apple path example

```toml
[targets.apple]
output = "dist/apple"

[targets.apple.swift]
module_name = "MyLib"
output = "swift-wrapper/Sources"

[targets.apple.xcframework]
output = "dist/apple"
name = "MyLib"

[targets.apple.spm]
output = "dist/apple/spm"
layout = "split"
distribution = "local"
```

With this config:
- binary package lives in `dist/apple/spm`
- generated Swift wrapper file lands in `swift-wrapper/Sources/BoltFFIGenerated/MyLib.swift`

## Android packaging pattern

Android output is split between generated Kotlin/JNI sources and packed native libraries.

Typical config:

```toml
[targets.android]
output = "dist/android"
min_sdk = 24

[targets.android.kotlin]
package = "com.acme.mylib"
output = "dist/android/kotlin"
api_style = "top_level"

[targets.android.header]
output = "dist/android/include"

[targets.android.pack]
output = "dist/android/jniLibs"
```

Typical flow:
1. `boltffi build android` builds native artifacts.
2. `boltffi generate kotlin` writes Kotlin/JNI glue.
3. `boltffi pack android` assembles `jniLibs/` for Android integration.

## WASM npm package shape

`boltffi pack wasm` assembles an npm package with environment-specific entrypoints.

### Generated files

- `{module_name}_bg.wasm` compiled WASM binary
- `{module_name}.js` core bindings module with manual initialization
- `{module_name}.d.ts` TypeScript declarations
- `bundler.js` bundler entrypoint (when enabled)
- `web.js` browser entrypoint (when enabled)
- `node.js` Node.js entrypoint (when enabled)
- `package.json` npm manifest (when enabled)
- `README.md` package readme scaffold (when enabled)

### Core module behavior

`{module_name}.js` exports:
- `init(source: BufferSource | Response): Promise<void>`
- all generated API functions

Generated API functions throw `Error` if called before `init()` resolves.

### Loader entrypoint behavior

Each loader entrypoint (`bundler.js`, `web.js`, `node.js`) exports:
- `initialized: Promise<void>`
- all generated API functions

Generated API functions throw `Error` if called before `initialized` resolves.

Loading strategy by entrypoint:
- `bundler.js` relies on bundler WASM asset handling
- `web.js` loads `.wasm` via `fetch()` from package-relative location
- `node.js` loads `.wasm` via `fs.readFile()` from disk

### Package exports examples

All targets enabled (`targets = ["bundler", "web", "nodejs"]`):

```json
{
  "exports": {
    ".": {
      "types": "./{module_name}.d.ts",
      "browser": "./web.js",
      "node": "./node.js",
      "default": "./bundler.js"
    }
  }
}
```

Partial targets include only enabled conditions. `default` resolves to:
- `bundler.js` if enabled
- else `web.js` if enabled
- else `node.js`

Example with `targets = ["nodejs"]`:

```json
{
  "exports": {
    ".": {
      "types": "./{module_name}.d.ts",
      "node": "./node.js",
      "default": "./node.js"
    }
  }
}
```

## Full multi-target example

```toml
[package]
name = "mylib"
crate = "my_lib"
version = "0.1.0"
license = "MIT"
repository = "https://github.com/acme/mylib"

[targets.apple]
enabled = true
output = "dist/apple"
deployment_target = "16.0"
include_macos = false

[targets.apple.swift]
module_name = "MyLib"
tools_version = "5.9"

[targets.apple.header]
output = "dist/apple/include"

[targets.apple.xcframework]
name = "MyLib"

[targets.apple.spm]
layout = "ffi-only"
distribution = "local"

[targets.android]
enabled = true
output = "dist/android"
min_sdk = 24

[targets.android.kotlin]
package = "com.acme.mylib"
api_style = "top_level"
factory_style = "constructors"

[targets.android.header]
output = "dist/android/include"

[targets.android.pack]
output = "dist/android/jniLibs"

[targets.wasm]
enabled = true
triple = "wasm32-unknown-unknown"
profile = "release"
output = "dist/wasm"

[targets.wasm.optimize]
enabled = true
level = "s"
strip_debug = true

[targets.wasm.typescript]
output = "dist/wasm/pkg"
runtime_package = "@boltffi/runtime"
module_name = "mylib"

[targets.wasm.npm]
package_name = "@acme/mylib"
targets = ["bundler", "web", "nodejs"]
generate_package_json = true
generate_readme = true
```
