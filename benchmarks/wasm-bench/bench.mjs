import Benchmark from 'benchmark';

const boltffi = await import('./boltffi/node.js');
await boltffi.initialized;

const wasmbindgen = await import('./wasmbindgen/bench_wasm_bindgen.js');

console.log('BoltFFI vs wasm-bindgen WASM Benchmark');
console.log('=====================================');
console.log('Note: UniFFI does not have WASM support, defaulting to wasm-bindgen for comparison.\n');

const results = [];

function runSuite(name, boltffiFn, wasmbindgenFn) {
  return new Promise((resolve) => {
    const suite = new Benchmark.Suite(name);
    
    suite
      .add(`boltffi_${name}`, boltffiFn)
      .add(`wasmbindgen_${name}`, wasmbindgenFn)
      .on('cycle', (event) => {
        console.log(String(event.target));
      })
      .on('complete', function() {
        const boltffiResult = this.filter((b) => b.name.startsWith('boltffi'))[0];
        const wbResult = this.filter((b) => b.name.startsWith('wasmbindgen'))[0];
        
        const speedup = wbResult.hz / boltffiResult.hz;
        const boltffiNs = (1 / boltffiResult.hz) * 1e9;
        const wbNs = (1 / wbResult.hz) * 1e9;
        
        results.push({
          name,
          boltffi_ns: Math.round(boltffiNs),
          wasmbindgen_ns: Math.round(wbNs),
          speedup: speedup.toFixed(2) + 'x'
        });
        
        resolve();
      })
      .run({ async: true });
  });
}

await runSuite('noop', 
  () => boltffi.noop(),
  () => wasmbindgen.noop()
);

await runSuite('echo_i32',
  () => boltffi.echoI32(42),
  () => wasmbindgen.echo_i32(42)
);

await runSuite('echo_f64',
  () => boltffi.echoF64(3.14159),
  () => wasmbindgen.echo_f64(3.14159)
);

await runSuite('add',
  () => boltffi.add(100, 200),
  () => wasmbindgen.add(100, 200)
);

await runSuite('multiply',
  () => boltffi.multiply(2.5, 4.0),
  () => wasmbindgen.multiply(2.5, 4.0)
);

await runSuite('echo_string_200',
  () => boltffi.echoString('x'.repeat(200)),
  () => wasmbindgen.echo_string('x'.repeat(200))
);

await runSuite('echo_string_1k',
  () => boltffi.echoString('x'.repeat(1000)),
  () => wasmbindgen.echo_string('x'.repeat(1000))
);

await runSuite('generate_string_1k',
  () => boltffi.generateString(1000),
  () => wasmbindgen.generate_string(1000)
);

await runSuite('generate_locations_100',
  () => boltffi.generateLocations(100),
  () => wasmbindgen.generate_locations(100)
);

await runSuite('generate_locations_1k',
  () => boltffi.generateLocations(1000),
  () => wasmbindgen.generate_locations(1000)
);

await runSuite('generate_trades_100',
  () => boltffi.generateTrades(100),
  () => wasmbindgen.generate_trades(100)
);

await runSuite('generate_trades_1k',
  () => boltffi.generateTrades(1000),
  () => wasmbindgen.generate_trades(1000)
);

await runSuite('generate_particles_100',
  () => boltffi.generateParticles(100),
  () => wasmbindgen.generate_particles(100)
);

await runSuite('generate_particles_1k',
  () => boltffi.generateParticles(1000),
  () => wasmbindgen.generate_particles(1000)
);

await runSuite('generate_i32_vec_1k',
  () => boltffi.generateI32Vec(1000),
  () => wasmbindgen.generate_i32_vec(1000)
);

await runSuite('generate_i32_vec_10k',
  () => boltffi.generateI32Vec(10000),
  () => wasmbindgen.generate_i32_vec(10000)
);

await runSuite('generate_bytes_64k',
  () => boltffi.generateBytes(65536),
  () => wasmbindgen.generate_bytes(65536)
);

await runSuite('roundtrip_locations_100',
  () => {
    const locs = boltffi.generateLocations(100);
    return boltffi.sumRatings(locs);
  },
  () => {
    const locs = wasmbindgen.generate_locations(100);
    return wasmbindgen.sum_location_ratings(locs);
  }
);

await runSuite('roundtrip_i32_vec_1k',
  () => {
    const vec = boltffi.generateI32Vec(1000);
    return boltffi.sumI32Vec(vec);
  },
  () => {
    const vec = wasmbindgen.generate_i32_vec(1000);
    return wasmbindgen.sum_i32_vec(vec);
  }
);

await runSuite('counter_increment_1k',
  () => {
    const counter = boltffi.Counter.new(0);
    for (let i = 0; i < 1000; i++) counter.increment();
    const v = counter.get();
    counter.dispose();
    return v;
  },
  () => {
    const counter = new wasmbindgen.Counter();
    for (let i = 0; i < 1000; i++) counter.increment();
    const v = counter.get();
    counter.free();
    return v;
  }
);

await runSuite('datastore_add_1k',
  () => {
    const store = boltffi.DataStore.new();
    for (let i = 0; i < 1000; i++) {
      store.add({ x: i, y: i * 2, timestamp: BigInt(i) });
    }
    const len = store.len();
    store.dispose();
    return len;
  },
  () => {
    const store = new wasmbindgen.DataStore();
    for (let i = 0; i < 1000; i++) {
      store.add(new wasmbindgen.DataPoint(i, i * 2, BigInt(i)));
    }
    const len = store.len();
    store.free();
    return len;
  }
);

await runSuite('accumulator_1k',
  () => {
    const acc = boltffi.Accumulator.new();
    for (let i = 0n; i < 1000n; i++) acc.add(i);
    const v = acc.get();
    acc.reset();
    acc.dispose();
    return v;
  },
  () => {
    const acc = new wasmbindgen.Accumulator();
    for (let i = 0n; i < 1000n; i++) acc.add(i);
    const v = acc.get();
    acc.reset();
    acc.free();
    return v;
  }
);

await runSuite('find_even_100',
  () => {
    for (let i = 0; i < 100; i++) boltffi.findEven(i);
  },
  () => {
    for (let i = 0; i < 100; i++) wasmbindgen.find_even(i);
  }
);

await runSuite('async_add',
  async () => await boltffi.asyncAdd(100, 200),
  async () => await wasmbindgen.async_add(100, 200)
);

console.log('\n=====================================');
console.log('Results Summary');
console.log('=====================================\n');

console.log('| Benchmark | BoltFFI (ns) | wasm-bindgen (ns) | Speedup |');
console.log('|-----------|--------------|-------------------|---------|');
for (const r of results) {
  const ratio = parseFloat(r.speedup);
  let speedupStr;
  if (ratio >= 0.95 && ratio <= 1.05) {
    speedupStr = 'TIE';
  } else if (ratio < 1) {
    speedupStr = `${(1/ratio).toFixed(2)}x faster`;
  } else {
    speedupStr = `${r.speedup} slower`;
  }
  console.log(`| ${r.name} | ${r.boltffi_ns} | ${r.wasmbindgen_ns} | ${speedupStr} |`);
}
