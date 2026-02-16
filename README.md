# BoltFFI

A high-performance multi-language bindings generator for Rust. Up to 1,000x faster than UniFFI. Up to 450x faster than wasm-bindgen.

<p align="center">
  <img src="docs/assets/demo.gif" width="700" />
</p>

Quick links: [User Guide](https://boltffi.dev/docs/overview) | [Tutorial](https://boltffi.dev/docs/tutorial) | [Getting Started](https://boltffi.dev/docs/getting-started)

## Performance

### vs UniFFI (Swift/Kotlin)

| Benchmark | BoltFFI | UniFFI | Speedup |
|-----------|--------:|-------:|--------:|
| noop | <1 ns | 1,416 ns | >1000x |
| echo_i32 | <1 ns | 1,416 ns | >1000x |
| counter_increment (1k calls) | 1,083 ns | 1,388,895 ns | 1,282x |
| generate_locations (1k structs) | 4,167 ns | 1,276,333 ns | 306x |
| generate_locations (10k structs) | 62,542 ns | 12,817,000 ns | 205x |

### vs wasm-bindgen (WASM)

| Benchmark | BoltFFI | wasm-bindgen | Speedup |
|-----------|--------:|-------------:|--------:|
| 1k particles | 29,886 ns | 13,532,530 ns | 453x |
| 100 particles | 3,117 ns | 748,287 ns | 240x |
| 1k locations | 21,931 ns | 4,037,879 ns | 184x |
| 1k trades | 42,015 ns | 5,781,767 ns | 138x |
| 100 locations | 2,199 ns | 283,753 ns | 129x |

Full benchmark code: [benchmarks](./benchmarks)


## Why BoltFFI?

Serialization-based FFI is slow. UniFFI serializes every value to a byte buffer. wasm-bindgen materializes every struct as a JavaScript object. That overhead shows up even when you're making tens or hundreds of FFI calls per second.

BoltFFI uses zero-copy where possible. Primitives pass as raw values. Structs with primitive fields pass as pointers to memory both sides can read directly. WASM uses a wire buffer format that avoids per-field allocation. Only strings and collections go through encoding.

## What it does

Mark your Rust types with `#[data]` and functions with `#[export]`:

```rust
use boltffi::*;

#[data]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

#[export]
pub fn distance(a: Point, b: Point) -> f64 {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    (dx * dx + dy * dy).sqrt()
}
```

Run `boltffi pack`:

```bash
boltffi pack apple
# Produces: ./dist/apple/YourCrate.xcframework + Package.swift

boltffi pack android
# Produces: ./dist/android/jniLibs/*.so + Kotlin bindings

boltffi pack wasm
# Produces: ./dist/wasm/pkg/*.wasm + TypeScript bindings + npm package
```

Use it from Swift, Kotlin, or TypeScript:

```swift
let d = distance(a: Point(x: 0, y: 0), b: Point(x: 3, y: 4)) // 5.0
```

```kotlin
val d = distance(a = Point(x = 0.0, y = 0.0), b = Point(x = 3.0, y = 4.0)) // 5.0
```

```typescript
import { distance } from 'your-crate';
const d = distance({ x: 0, y: 0 }, { x: 3, y: 4 }); // 5.0
```

The generated bindings use each language's idioms. Swift gets async/await. Kotlin gets coroutines. TypeScript gets Promises. Errors become native exceptions.

## Supported languages

| Language | Status       |
|----------|--------------|
| Swift    | Full support |
| Kotlin   | Full support |
| WASM/TypeScript | Full support |
| Python   | Planned      |
| C#       | Planned      |
| Ruby     | Planned      |

Want another language? [Open an issue](https://github.com/boltffi/boltffi/issues).

## Installation

```bash
cargo install boltffi_cli
```

Add to your `Cargo.toml`:

```toml
[dependencies]
boltffi = "0.1"

[lib]
crate-type = ["staticlib", "cdylib"]
```

## Documentation

- [Overview](https://boltffi.dev/docs/overview)
- [Getting Started](https://boltffi.dev/docs/getting-started)
- [Tutorial](https://boltffi.dev/docs/tutorial)
- [Types](https://boltffi.dev/docs/types)
- [Async](https://boltffi.dev/docs/async)
- [Streaming](https://boltffi.dev/docs/streaming)

## Alternative tools

Other tools that solve similar problems:

- [UniFFI](https://github.com/mozilla/uniffi-rs) - Mozilla's binding generator, uses serialization-based approach
- [Diplomat](https://github.com/rust-diplomat/diplomat) - Focused on C/C++ interop
- [cxx](https://github.com/dtolnay/cxx) - Safe C++/Rust interop

## Contributing
Contributions are warmly welcomed 🙌

- [File an issue](https://github.com/boltffi/boltffi/issues)
- [Submit a PR](https://github.com/boltffi/boltffi/pulls)

## License
BOLTFFI is released under the MIT license. See [LICENSE](./LICENSE) for more information.
