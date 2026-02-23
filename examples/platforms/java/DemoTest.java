package com.boltffi.demo;

public final class DemoTest {
    public static void main(String[] args) {
        System.out.println("Testing Java bindings...\n");
        testBool();
        testI32();
        testI64();
        testF32();
        testF64();
        testStrings();
        testPointRecords();
        testLineRecords();
        testPersonRecords();
        System.out.println("All tests passed!");
    }

    private static void testBool() {
        System.out.println("Testing bool...");
        assert Demo.echoBool(true);
        assert !Demo.echoBool(false);
        assert !Demo.negateBool(true);
        assert Demo.negateBool(false);
        System.out.println("  PASS\n");
    }

    private static void testI32() {
        System.out.println("Testing i32...");
        assert Demo.echoI32(42) == 42 : "echoI32(42)";
        assert Demo.echoI32(-100) == -100 : "echoI32(-100)";
        assert Demo.addI32(10, 20) == 30 : "addI32(10, 20)";
        System.out.println("  PASS\n");
    }

    private static void testI64() {
        System.out.println("Testing i64...");
        assert Demo.echoI64(9999999999L) == 9999999999L : "echoI64(large)";
        assert Demo.echoI64(-9999999999L) == -9999999999L : "echoI64(negative large)";
        System.out.println("  PASS\n");
    }

    private static void testF32() {
        System.out.println("Testing f32...");
        assert Math.abs(Demo.echoF32(3.14f) - 3.14f) < 0.001f : "echoF32(3.14)";
        assert Math.abs(Demo.addF32(1.5f, 2.5f) - 4.0f) < 0.001f : "addF32(1.5, 2.5)";
        System.out.println("  PASS\n");
    }

    private static void testF64() {
        System.out.println("Testing f64...");
        assert Math.abs(Demo.echoF64(3.14159265359) - 3.14159265359) < 0.0000001 : "echoF64(pi)";
        assert Math.abs(Demo.addF64(1.5, 2.5) - 4.0) < 0.0000001 : "addF64(1.5, 2.5)";
        System.out.println("  PASS\n");
    }

    private static void testStrings() {
        System.out.println("Testing strings...");
        assert Demo.echoString("hello").equals("hello") : "echoString(hello)";
        assert Demo.echoString("").equals("") : "echoString(empty)";
        assert Demo.echoString("café").equals("café") : "echoString(unicode)";
        assert Demo.echoString("日本語").equals("日本語") : "echoString(cjk)";
        assert Demo.echoString("hello 🌍 world").equals("hello 🌍 world") : "echoString(emoji)";
        assert Demo.concatStrings("foo", "bar").equals("foobar") : "concatStrings(foo, bar)";
        assert Demo.concatStrings("", "bar").equals("bar") : "concatStrings(empty, bar)";
        assert Demo.concatStrings("foo", "").equals("foo") : "concatStrings(foo, empty)";
        assert Demo.concatStrings("🎉", "🎊").equals("🎉🎊") : "concatStrings(emoji)";
        assert Demo.stringLength("hello") == 5 : "stringLength(hello)";
        assert Demo.stringLength("") == 0 : "stringLength(empty)";
        assert Demo.stringLength("café") == 5 : "stringLength(utf8 bytes)";
        assert Demo.stringLength("🌍") == 4 : "stringLength(emoji 4 bytes)";
        System.out.println("  PASS\n");
    }

    private static void testPointRecords() {
        System.out.println("Testing records (Point)...");
        Point point = Demo.makePoint(1.0, 2.0);
        assert point.x() == 1.0 : "makePoint.x";
        assert point.y() == 2.0 : "makePoint.y";
        Point echoedPoint = Demo.echoPoint(point);
        assert echoedPoint.x() == 1.0 : "echoPoint.x";
        assert echoedPoint.y() == 2.0 : "echoPoint.y";
        Point sumPoint = Demo.addPoints(new Point(3.0, 4.0), new Point(5.0, 6.0));
        assert sumPoint.x() == 8.0 : "addPoints.x";
        assert sumPoint.y() == 10.0 : "addPoints.y";
        assert Math.abs(Demo.pointDistance(new Point(3.0, 4.0)) - 5.0) < 0.0001 : "pointDistance";
        System.out.println("  PASS\n");
    }

    private static void testLineRecords() {
        System.out.println("Testing records (Line)...");
        Line line = Demo.makeLine(0.0, 0.0, 3.0, 4.0);
        assert line.start().x() == 0.0 : "makeLine.start.x";
        assert line.end().y() == 4.0 : "makeLine.end.y";
        Line echoedLine = Demo.echoLine(line);
        assert echoedLine.start().x() == 0.0 : "echoLine.start.x";
        assert echoedLine.end().x() == 3.0 : "echoLine.end.x";
        assert Math.abs(Demo.lineLength(line) - 5.0) < 0.0001 : "lineLength";
        System.out.println("  PASS\n");
    }

    private static void testPersonRecords() {
        System.out.println("Testing records (Person)...");
        Person person = Demo.makePerson("Alice", 30);
        assert person.name().equals("Alice") : "makePerson.name";
        assert person.age() == 30 : "makePerson.age";
        Person echoedPerson = Demo.echoPerson(person);
        assert echoedPerson.name().equals("Alice") : "echoPerson.name";
        assert echoedPerson.age() == 30 : "echoPerson.age";
        assert Demo.greetPerson(person).equals("Hello, Alice! You are 30 years old.") : "greetPerson";
        Person emojiPerson = Demo.makePerson("🎉 Party", 25);
        assert emojiPerson.name().equals("🎉 Party") : "makePerson(emoji)";
        Person echoedEmojiPerson = Demo.echoPerson(emojiPerson);
        assert echoedEmojiPerson.name().equals("🎉 Party") : "echoPerson(emoji)";
        System.out.println("  PASS\n");
    }
}
