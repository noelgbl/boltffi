import { add, initialized } from './dist/wasm/pkg/node.js';

await initialized;

console.log('Module initialized via node.js loader');

const result = add(2, 3);
console.log(`add(2, 3) = ${result}`);
if (result !== 5) throw new Error(`Expected 5, got ${result}`);

console.log('pack wasm e2e test passed!');
