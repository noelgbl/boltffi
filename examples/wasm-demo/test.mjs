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

console.log('\nAll tests passed!');
