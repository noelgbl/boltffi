import {
  initialized,
  echoBool, negateBool,
  echoI8, echoU8,
  echoI16, echoU16,
  echoI32, addI32, echoU32,
  echoI64, echoU64,
  echoF32, addF32,
  echoF64, addF64,
  echoString, concatStrings, stringLength,
  echoPoint, makePoint, addPoints, pointDistance,
  echoLine, makeLine, lineLength,
  echoPerson, makePerson, greetPerson,
  echoPolygon, makePolygon, polygonVertexCount, polygonCentroid,
  echoTeam, makeTeam, teamSize,
  echoClassroom, makeClassroom,
  Status, echoStatus, statusToString,
  echoShape, shapeArea, makeCircle, makeRectangle,
  echoOptionalI32, echoOptionalString, echoOptionalPoint,
  unwrapOrDefaultI32, isSomeString, makeSomePoint, makeNonePoint,
  MathError, MathErrorException, safeDivide, safeSqrt, parsePoint, alwaysOk, alwaysErr,
  echoBytes, bytesLength, bytesSum, makeBytes, reverseBytes,
  echoDuration, makeDuration, durationAsMillis,
  echoSystemTime, systemTimeToMillis, millisToSystemTime,
  echoUuid, uuidToString,
  echoUrl, urlToString,
  invokeCallback, invokeCallbackTwice,
  transformPoint,
  mapStatus,
  processVecI32,
  processVecPoint,
  asyncAdd,
  asyncEcho,
  asyncDoubleAll,
  fetchWithCallback,
  fetchStringWithCallback,
  Counter,
  applyClosure,
  applyBinaryClosure,
  applyVoidClosure,
  applyNullaryClosure,
  applyPointClosure,
  applyStringClosure,
} from './dist/wasm/pkg/node.js';

await initialized;
console.log('Module initialized via node.js loader\n');

function assert(condition, msg) {
  if (!condition) throw new Error(msg);
}

console.log('Testing bool...');
assert(echoBool(true) === true, 'echoBool(true)');
assert(echoBool(false) === false, 'echoBool(false)');
assert(negateBool(true) === false, 'negateBool(true)');
assert(negateBool(false) === true, 'negateBool(false)');

console.log('Testing i8/u8...');
assert(echoI8(127) === 127, 'echoI8(127)');
assert(echoI8(-128) === -128, 'echoI8(-128)');
assert(echoU8(255) === 255, 'echoU8(255)');
assert(echoU8(0) === 0, 'echoU8(0)');

console.log('Testing i16/u16...');
assert(echoI16(32767) === 32767, 'echoI16(32767)');
assert(echoI16(-32768) === -32768, 'echoI16(-32768)');
assert(echoU16(65535) === 65535, 'echoU16(65535)');

console.log('Testing i32/u32...');
assert(echoI32(2147483647) === 2147483647, 'echoI32(max)');
assert(echoI32(-2147483648) === -2147483648, 'echoI32(min)');
assert(addI32(2, 3) === 5, 'addI32(2, 3)');
assert(echoU32(2147483647) === 2147483647, 'echoU32(below signed max)');

console.log('Testing i64/u64...');
assert(echoI64(9007199254740991n) === 9007199254740991n, 'echoI64(safe max)');
assert(echoI64(-9007199254740991n) === -9007199254740991n, 'echoI64(safe min)');
assert(echoU64(9007199254740991n) === 9007199254740991n, 'echoU64(safe max)');

console.log('Testing f32...');
assert(Math.abs(echoF32(3.14) - 3.14) < 0.001, 'echoF32(3.14)');
assert(Math.abs(addF32(1.5, 2.5) - 4.0) < 0.001, 'addF32(1.5, 2.5)');

console.log('Testing f64...');
assert(echoF64(3.141592653589793) === 3.141592653589793, 'echoF64(pi)');
assert(addF64(1.1, 2.2) === 3.3000000000000003, 'addF64(1.1, 2.2)');

console.log('Testing string...');
assert(echoString('hello') === 'hello', 'echoString(hello)');
assert(concatStrings('foo', 'bar') === 'foobar', 'concatStrings');
assert(stringLength('test') === 4, 'stringLength(test)');

console.log('Testing records (Point)...');
const p1 = makePoint(3.0, 4.0);
assert(p1.x === 3.0, 'makePoint x');
assert(p1.y === 4.0, 'makePoint y');

const p2 = echoPoint({ x: 1.5, y: 2.5 });
assert(p2.x === 1.5, 'echoPoint x');
assert(p2.y === 2.5, 'echoPoint y');

const p3 = addPoints({ x: 1.0, y: 2.0 }, { x: 3.0, y: 4.0 });
assert(p3.x === 4.0, 'addPoints x');
assert(p3.y === 6.0, 'addPoints y');

const dist = pointDistance({ x: 3.0, y: 4.0 });
assert(Math.abs(dist - 5.0) < 0.0001, 'pointDistance');

console.log('Testing records (Line - nested)...');
const line1 = makeLine(0, 0, 3, 4);
assert(line1.start.x === 0, 'makeLine start.x');
assert(line1.start.y === 0, 'makeLine start.y');
assert(line1.end.x === 3, 'makeLine end.x');
assert(line1.end.y === 4, 'makeLine end.y');

const line2 = echoLine({ start: { x: 1, y: 2 }, end: { x: 5, y: 6 } });
assert(line2.start.x === 1, 'echoLine start.x');
assert(line2.end.y === 6, 'echoLine end.y');

const len = lineLength({ start: { x: 0, y: 0 }, end: { x: 3, y: 4 } });
assert(Math.abs(len - 5.0) < 0.0001, 'lineLength');

console.log('Testing records (Person - with string)...');
const person1 = makePerson('Alice', 30);
assert(person1.name === 'Alice', 'makePerson name');
assert(person1.age === 30, 'makePerson age');

const person2 = echoPerson({ name: 'Bob', age: 25 });
assert(person2.name === 'Bob', 'echoPerson name');
assert(person2.age === 25, 'echoPerson age');

const greeting = greetPerson({ name: 'Charlie', age: 40 });
assert(greeting === 'Hello, Charlie! You are 40 years old.', 'greetPerson');

console.log('Testing records (Polygon - Vec<Point>)...');
const poly1 = makePolygon([{ x: 0, y: 0 }, { x: 1, y: 0 }, { x: 0.5, y: 1 }]);
assert(poly1.points.length === 3, 'makePolygon length');
assert(poly1.points[0].x === 0, 'makePolygon points[0].x');
assert(poly1.points[2].y === 1, 'makePolygon points[2].y');

const count = polygonVertexCount({ points: [{ x: 0, y: 0 }, { x: 1, y: 1 }] });
assert(count === 2, 'polygonVertexCount');

const centroid = polygonCentroid({ points: [{ x: 0, y: 0 }, { x: 2, y: 0 }, { x: 1, y: 3 }] });
assert(Math.abs(centroid.x - 1.0) < 0.0001, 'polygonCentroid x');
assert(Math.abs(centroid.y - 1.0) < 0.0001, 'polygonCentroid y');

console.log('Testing records (Team - Vec<String>)...');
const team1 = makeTeam('Dev Team', ['Alice', 'Bob', 'Charlie']);
assert(team1.name === 'Dev Team', 'makeTeam name');
assert(team1.members.length === 3, 'makeTeam members length');
assert(team1.members[1] === 'Bob', 'makeTeam members[1]');

const team2 = echoTeam({ name: 'QA', members: ['Dave', 'Eve'] });
assert(team2.name === 'QA', 'echoTeam name');
assert(team2.members.length === 2, 'echoTeam members length');

const size = teamSize({ name: 'Ops', members: ['Frank', 'Grace', 'Heidi', 'Ivan'] });
assert(size === 4, 'teamSize');

console.log('Testing records (Classroom - Vec<Person>)...');
const classroom1 = makeClassroom([{ name: 'Alice', age: 20 }, { name: 'Bob', age: 22 }]);
assert(classroom1.students.length === 2, 'makeClassroom length');
assert(classroom1.students[0].name === 'Alice', 'makeClassroom students[0].name');
assert(classroom1.students[1].age === 22, 'makeClassroom students[1].age');

const classroom2 = echoClassroom({ students: [{ name: 'Charlie', age: 25 }] });
assert(classroom2.students.length === 1, 'echoClassroom length');
assert(classroom2.students[0].name === 'Charlie', 'echoClassroom students[0].name');

console.log('Testing enums (C-style - Status)...');
assert(echoStatus(Status.Active) === Status.Active, 'echoStatus Active');
assert(echoStatus(Status.Inactive) === Status.Inactive, 'echoStatus Inactive');
assert(echoStatus(Status.Pending) === Status.Pending, 'echoStatus Pending');
assert(statusToString(Status.Active) === 'active', 'statusToString Active');
assert(statusToString(Status.Inactive) === 'inactive', 'statusToString Inactive');

console.log('Testing enums (Data - Shape)...');
const circle = makeCircle(5.0);
assert(circle.tag === 'Circle', 'makeCircle tag');
assert(circle.radius === 5.0, 'makeCircle radius');
assert(Math.abs(shapeArea(circle) - Math.PI * 25) < 0.0001, 'shapeArea circle');

const rect = makeRectangle(3.0, 4.0);
assert(rect.tag === 'Rectangle', 'makeRectangle tag');
assert(rect.width === 3.0, 'makeRectangle width');
assert(rect.height === 4.0, 'makeRectangle height');
assert(shapeArea(rect) === 12.0, 'shapeArea rectangle');

const echoedCircle = echoShape({ tag: 'Circle', radius: 2.5 });
assert(echoedCircle.tag === 'Circle', 'echoShape circle tag');
assert(echoedCircle.radius === 2.5, 'echoShape circle radius');

const echoedPoint = echoShape({ tag: 'Point' });
assert(echoedPoint.tag === 'Point', 'echoShape point tag');
assert(shapeArea({ tag: 'Point' }) === 0.0, 'shapeArea point');

console.log('Testing Option<T>...');
assert(echoOptionalI32(42) === 42, 'echoOptionalI32 some');
assert(echoOptionalI32(null) === null, 'echoOptionalI32 none');
assert(echoOptionalI32(0) === 0, 'echoOptionalI32 zero');

assert(echoOptionalString('hello') === 'hello', 'echoOptionalString some');
assert(echoOptionalString(null) === null, 'echoOptionalString none');
assert(echoOptionalString('') === '', 'echoOptionalString empty');

const optPoint = echoOptionalPoint({ x: 1, y: 2 });
assert(optPoint !== null && optPoint.x === 1 && optPoint.y === 2, 'echoOptionalPoint some');
assert(echoOptionalPoint(null) === null, 'echoOptionalPoint none');

assert(unwrapOrDefaultI32(10, 5) === 10, 'unwrapOrDefaultI32 some');
assert(unwrapOrDefaultI32(null, 5) === 5, 'unwrapOrDefaultI32 none');

assert(isSomeString('test') === true, 'isSomeString true');
assert(isSomeString(null) === false, 'isSomeString false');

const somePoint = makeSomePoint(3, 4);
assert(somePoint !== null && somePoint.x === 3 && somePoint.y === 4, 'makeSomePoint');
assert(makeNonePoint() === null, 'makeNonePoint');

console.log('Testing Result<T, E>...');
assert(safeDivide(10, 2) === 5, 'safeDivide ok');
try {
  safeDivide(10, 0);
  assert(false, 'safeDivide should throw on division by zero');
} catch (e) {
  assert(e instanceof MathErrorException, 'safeDivide err type');
  assert(e.code === MathError.DivisionByZero, 'safeDivide err code');
}

assert(Math.abs(safeSqrt(16) - 4) < 0.0001, 'safeSqrt ok');
try {
  safeSqrt(-1);
  assert(false, 'safeSqrt should throw on negative input');
} catch (e) {
  assert(e instanceof MathErrorException, 'safeSqrt err type');
  assert(e.code === MathError.NegativeInput, 'safeSqrt err code');
}

const point = parsePoint('3.5, 4.5');
assert(Math.abs(point.x - 3.5) < 0.0001 && Math.abs(point.y - 4.5) < 0.0001, 'parsePoint ok');
try {
  parsePoint('invalid');
  assert(false, 'parsePoint should throw on invalid input');
} catch (e) {
  assert(e instanceof Error, 'parsePoint err is Error');
  assert(typeof e.message === 'string', 'parsePoint err has message');
}

assert(alwaysOk(5) === 10, 'alwaysOk');
try {
  alwaysErr('test error');
  assert(false, 'alwaysErr should throw');
} catch (e) {
  assert(e instanceof Error, 'alwaysErr is Error');
  assert(e.message === 'test error', 'alwaysErr message');
}

console.log('Testing Vec<u8>/Bytes...');
const testBytes = new Uint8Array([1, 2, 3, 4, 5]);
const echoed = echoBytes(testBytes);
assert(echoed.length === 5, 'echoBytes length');
assert(echoed[0] === 1 && echoed[4] === 5, 'echoBytes content');

assert(bytesLength(testBytes) === 5, 'bytesLength');
assert(bytesSum(testBytes) === 15, 'bytesSum');

const generated = makeBytes(10);
assert(generated.length === 10, 'makeBytes length');
assert(generated[0] === 0 && generated[9] === 9, 'makeBytes content');

const reversed = reverseBytes(new Uint8Array([1, 2, 3]));
assert(reversed[0] === 3 && reversed[1] === 2 && reversed[2] === 1, 'reverseBytes');

const empty = echoBytes(new Uint8Array(0));
assert(empty.length === 0, 'echoBytes empty');

console.log('Testing Duration...');
const duration = makeDuration(5n, 500000000);
assert(duration.secs === 5n, 'makeDuration secs');
assert(duration.nanos === 500000000, 'makeDuration nanos');

const echoDur = echoDuration({ secs: 10n, nanos: 123456789 });
assert(echoDur.secs === 10n && echoDur.nanos === 123456789, 'echoDuration');

assert(durationAsMillis({ secs: 1n, nanos: 500000000 }) === 1500n, 'durationAsMillis');

console.log('Testing SystemTime...');
const now = new Date();
const echoedTime = echoSystemTime(now);
assert(Math.abs(echoedTime.getTime() - now.getTime()) < 1000, 'echoSystemTime');

const msTime = systemTimeToMillis(new Date(1700000000000));
assert(msTime === 1700000000000n, 'systemTimeToMillis');

const fromMs = millisToSystemTime(1700000000000n);
assert(fromMs.getTime() === 1700000000000, 'millisToSystemTime');

console.log('Testing Uuid...');
const testUuid = '550e8400-e29b-41d4-a716-446655440000';
assert(uuidToString(testUuid).toLowerCase() === testUuid.toLowerCase(), 'uuidToString');
assert(echoUuid(testUuid).toLowerCase() === testUuid.toLowerCase(), 'echoUuid');

console.log('Testing Url...');
const testUrl = 'https://example.com/path?query=1';
const echoedUrl = echoUrl(testUrl);
assert(echoedUrl === testUrl, 'echoUrl');
assert(urlToString(testUrl) === testUrl, 'urlToString');

console.log('Testing Closure Callbacks...');
const triple = (x) => x * 3;
assert(applyClosure(triple, 7) === 21, 'applyClosure triple(7)');
assert(applyClosure((x) => x + 100, 5) === 105, 'applyClosure inline');
assert(applyClosure((x) => -x, 42) === -42, 'applyClosure negate');

assert(applyBinaryClosure((a, b) => a + b, 10, 20) === 30, 'applyBinaryClosure add');
assert(applyBinaryClosure((a, b) => a * b, 6, 7) === 42, 'applyBinaryClosure mul');

let sideEffect = 0;
applyVoidClosure((x) => { sideEffect = x * 2; }, 25);
assert(sideEffect === 50, 'applyVoidClosure side effect');

assert(applyNullaryClosure(() => 42) === 42, 'applyNullaryClosure constant');
let counter = 0;
assert(applyNullaryClosure(() => ++counter) === 1, 'applyNullaryClosure stateful');

console.log('Testing Wire-encoded Closure Callbacks...');
const doublePointClosure = (p) => ({ x: p.x * 2, y: p.y * 2 });
const doubledPoint = applyPointClosure(doublePointClosure, { x: 5, y: 10 });
assert(doubledPoint.x === 10 && doubledPoint.y === 20, 'applyPointClosure double');

const shiftPointClosure = (p) => ({ x: p.x + 1, y: p.y + 1 });
const shiftedPoint = applyPointClosure(shiftPointClosure, { x: 0, y: 0 });
assert(shiftedPoint.x === 1 && shiftedPoint.y === 1, 'applyPointClosure shift');

const upperStr = (s) => s.toUpperCase();
assert(applyStringClosure(upperStr, 'hello') === 'HELLO', 'applyStringClosure upper');

const repeatStr = (s) => s + s;
assert(applyStringClosure(repeatStr, 'ab') === 'abab', 'applyStringClosure repeat');

console.log('Testing Callbacks...');
const doubler = { onValue: (v) => v * 2 };
assert(invokeCallback(doubler, 5) === 10, 'invokeCallback doubler');
assert(invokeCallback(doubler, -3) === -6, 'invokeCallback negative');

const adder = { onValue: (v) => v + 100 };
assert(invokeCallbackTwice(adder, 1, 2) === 203, 'invokeCallbackTwice');

const accumulator = { sum: 0, onValue(v) { this.sum += v; return this.sum; } };
invokeCallback(accumulator, 10);
invokeCallback(accumulator, 20);
assert(accumulator.sum === 30, 'callback with state');

console.log('Testing Wire-type Callbacks (Record - Point)...');
const pointDoubler = { transform: (p) => ({ x: p.x * 2, y: p.y * 2 }) };
const doubled = transformPoint(pointDoubler, { x: 5, y: 10 });
assert(doubled.x === 10 && doubled.y === 20, 'transformPoint doubler');

const pointNegator = { transform: (p) => ({ x: -p.x, y: -p.y }) };
const negated = transformPoint(pointNegator, { x: 3, y: -7 });
assert(negated.x === -3 && negated.y === 7, 'transformPoint negator');

const pointOffset = { transform: (p) => ({ x: p.x + 100, y: p.y + 200 }) };
const offset1 = transformPoint(pointOffset, { x: 1, y: 2 });
assert(offset1.x === 101 && offset1.y === 202, 'transformPoint offset 1');

console.log('Testing Wire-type Callbacks (Enum - Status)...');
const statusToggle = { mapStatus: (s) => s === Status.Active ? Status.Inactive : Status.Active };
const toggled1 = mapStatus(statusToggle, Status.Active);
assert(toggled1 === Status.Inactive, 'mapStatus Active->Inactive');
const toggled2 = mapStatus(statusToggle, Status.Inactive);
assert(toggled2 === Status.Active, 'mapStatus Inactive->Active');
const toggled3 = mapStatus(statusToggle, Status.Pending);
assert(toggled3 === Status.Active, 'mapStatus Pending->Active');

console.log('Testing Wire-type Callbacks (Vec<Point>)...');
const pointsDoubler = { process: (pts) => pts.map(p => ({ x: p.x * 2, y: p.y * 2 })) };
const doubledPoints = processVecPoint(pointsDoubler, [{ x: 1, y: 2 }, { x: 3, y: 4 }]);
assert(doubledPoints.length === 2, 'processVecPoint length');
assert(doubledPoints[0].x === 2 && doubledPoints[0].y === 4, 'processVecPoint[0]');
assert(doubledPoints[1].x === 6 && doubledPoints[1].y === 8, 'processVecPoint[1]');

const pointsFilter = { process: (pts) => pts.filter(p => p.x > 0) };
const filtered = processVecPoint(pointsFilter, [{ x: -1, y: 0 }, { x: 5, y: 10 }, { x: -2, y: 3 }]);
assert(filtered.length === 1, 'processVecPoint filter length');
assert(filtered[0].x === 5 && filtered[0].y === 10, 'processVecPoint filter result');

console.log('Testing Wire-type Callbacks (Vec<i32>)...');
const vecDoubler = { process: (values) => values.map(v => v * 2) };
const vecResult = processVecI32(vecDoubler, [1, 2, 3, 4, 5]);
assert(vecResult.length === 5, 'processVecI32 length');
assert(vecResult[0] === 2 && vecResult[4] === 10, 'processVecI32 values');

console.log('Testing Async Functions...');
const asyncAddResult = await asyncAdd(3, 4);
console.log(`  asyncAdd(3, 4) = ${asyncAddResult}`);
assert(asyncAddResult === 7, 'asyncAdd');

const asyncEchoResult = await asyncEcho('Hello, Async!');
console.log(`  asyncEcho('Hello, Async!') = "${asyncEchoResult}"`);
assert(asyncEchoResult === 'Echo: Hello, Async!', 'asyncEcho');

const asyncDoubleResult = await asyncDoubleAll([1, 2, 3, 4, 5]);
console.log(`  asyncDoubleAll([1,2,3,4,5]) = [${asyncDoubleResult.join(', ')}]`);
assert(asyncDoubleResult.length === 5, 'asyncDoubleAll length');
assert(asyncDoubleResult[0] === 2 && asyncDoubleResult[4] === 10, 'asyncDoubleAll values');

console.log('Testing Async Callbacks...');
const asyncFetcher = {
  fetchValue: async (key) => {
    await new Promise(r => setTimeout(r, 10));
    return key * 10;
  },
  fetchString: async (input) => {
    await new Promise(r => setTimeout(r, 10));
    return `async: ${input}`;
  }
};
const fetchedValue = await fetchWithCallback(asyncFetcher, 5);
console.log(`  fetchWithCallback(5) = ${fetchedValue}`);
assert(fetchedValue === 50, 'fetchWithCallback');

const fetchedString = await fetchStringWithCallback(asyncFetcher, 'hello');
console.log(`  fetchStringWithCallback('hello') = "${fetchedString}"`);
assert(fetchedString === 'async: hello', 'fetchStringWithCallback');

console.log('\nTesting Counter class...');
{
  const counter = Counter.new(10);
  assert(counter.get() === 10, 'Counter.new(10).get()');

  counter.increment();
  assert(counter.get() === 11, 'increment()');

  counter.add(5);
  assert(counter.get() === 16, 'add(5)');

  counter.reset();
  assert(counter.get() === 0, 'reset()');

  counter.dispose();
  console.log('  sync methods ok');
}

{
  const counter = Counter.new(10);
  const result = counter.transform((x) => x * 2);
  assert(result === 20, 'transform(x => x * 2)');
  assert(counter.get() === 20, 'get() after transform');

  const sum = counter.applyBinary((a, b) => a + b, 5);
  assert(sum === 25, 'applyBinary add');

  const product = counter.applyBinary((a, b) => a * b, 3);
  assert(product === 60, 'applyBinary mul');

  counter.dispose();
  console.log('  closure methods ok');
}

{
  const counter = Counter.createWithDefault();
  assert(counter.get() === 0, 'createWithDefault().get()');
  counter.dispose();
  console.log('  createWithDefault ok');
}

{
  const counter = Counter.new(100);
  const result = await counter.asyncAdd(50);
  assert(result === 150, 'asyncAdd(50)');
  assert(counter.get() === 150, 'get() after asyncAdd');
  counter.dispose();
  console.log('  async methods ok');
}

{
  const c1 = Counter.new(10);
  const c2 = Counter.new(20);
  c1.add(5);
  assert(c1.get() === 15, 'c1 independent');
  assert(c2.get() === 20, 'c2 independent');
  c1.dispose();
  c2.dispose();
  console.log('  independent instances ok');
}

{
  const counter = Counter.new(0);
  counter.dispose();
  let threw = false;
  try {
    counter.get();
  } catch (e) {
    threw = true;
    assert(e.message.includes('disposed'), 'disposed error message');
  }
  assert(threw, 'disposed counter throws');
  console.log('  dispose protection ok');
}

console.log('\nTesting Callback Registry (T01-T11)...');
import {
  registerValueCallback,
  unregisterValueCallback,
} from './dist/wasm/pkg/node.js';

{
  // T01: register once - handle non-null
  const handle = registerValueCallback({ onValue: (v) => v });
  assert(handle !== null && handle !== undefined && handle > 0, 'T01: register returns non-null handle');
  unregisterValueCallback(handle);
  console.log('  T01: register returns non-null handle - ok');
}

{
  // T07: deterministic return - exact value
  const callback = { onValue: (v) => v * 3 + 7 };
  const result1 = invokeCallback(callback, 10);
  const result2 = invokeCallback(callback, 10);
  assert(result1 === 37, 'T07: deterministic return value');
  assert(result1 === result2, 'T07: same input produces same output');
  console.log('  T07: deterministic return - ok');
}

{
  // T09: side effect counter - +1 per call
  let callCount = 0;
  const callback = { onValue: (v) => { callCount++; return v; } };
  
  invokeCallback(callback, 1);
  assert(callCount === 1, 'T09: side effect count after 1 call');
  
  invokeCallback(callback, 2);
  assert(callCount === 2, 'T09: side effect count after 2 calls');
  
  invokeCallbackTwice(callback, 3, 4);
  assert(callCount === 4, 'T09: side effect count after invokeCallbackTwice');
  console.log('  T09: side effect counter - ok');
}

{
  // T10: reentrant callback - correct call order
  const callOrder = [];
  const outerCallback = {
    onValue: (v) => {
      callOrder.push(`outer-start-${v}`);
      if (v > 0) {
        invokeCallback(innerCallback, v - 1);
      }
      callOrder.push(`outer-end-${v}`);
      return v;
    }
  };
  const innerCallback = {
    onValue: (v) => {
      callOrder.push(`inner-${v}`);
      return v;
    }
  };
  
  invokeCallback(outerCallback, 2);
  assert(callOrder.length === 3, 'T10: reentrant callback call count');
  assert(callOrder[0] === 'outer-start-2', 'T10: reentrant order[0]');
  assert(callOrder[1] === 'inner-1', 'T10: reentrant order[1]');
  assert(callOrder[2] === 'outer-end-2', 'T10: reentrant order[2]');
  console.log('  T10: reentrant callback - ok');
}

{
  // T15: large payload args/returns - identical decoded value
  const largeVec = Array.from({ length: 1000 }, (_, i) => i);
  const processor = { process: (values) => values.map(v => v * 2) };
  const result = processVecI32(processor, largeVec);
  assert(result.length === 1000, 'T15: large payload length');
  assert(result[0] === 0 && result[999] === 1998, 'T15: large payload values');
  console.log('  T15: large payload - ok');
}

console.log('\nAll tests passed!');
