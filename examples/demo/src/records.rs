use boltffi::*;

#[data]
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

#[export]
pub fn echo_point(p: Point) -> Point {
    p
}

#[export]
pub fn make_point(x: f64, y: f64) -> Point {
    Point { x, y }
}

#[export]
pub fn add_points(a: Point, b: Point) -> Point {
    Point {
        x: a.x + b.x,
        y: a.y + b.y,
    }
}

#[export]
pub fn point_distance(p: Point) -> f64 {
    (p.x * p.x + p.y * p.y).sqrt()
}

#[data]
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Line {
    pub start: Point,
    pub end: Point,
}

#[export]
pub fn echo_line(l: Line) -> Line {
    l
}

#[export]
pub fn make_line(x1: f64, y1: f64, x2: f64, y2: f64) -> Line {
    Line {
        start: Point { x: x1, y: y1 },
        end: Point { x: x2, y: y2 },
    }
}

#[export]
pub fn line_length(l: Line) -> f64 {
    let dx = l.end.x - l.start.x;
    let dy = l.end.y - l.start.y;
    (dx * dx + dy * dy).sqrt()
}

#[data]
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Person {
    pub name: String,
    pub age: u32,
}

#[export]
pub fn echo_person(p: Person) -> Person {
    p
}

#[export]
pub fn make_person(name: String, age: u32) -> Person {
    Person { name, age }
}

#[export]
pub fn greet_person(p: Person) -> String {
    format!("Hello, {}! You are {} years old.", p.name, p.age)
}

#[data]
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Polygon {
    pub points: Vec<Point>,
}

#[export]
pub fn echo_polygon(p: Polygon) -> Polygon {
    p
}

#[export]
pub fn make_polygon(points: Vec<Point>) -> Polygon {
    Polygon { points }
}

#[export]
pub fn polygon_vertex_count(p: Polygon) -> u32 {
    p.points.len() as u32
}

#[export]
pub fn polygon_centroid(p: Polygon) -> Point {
    if p.points.is_empty() {
        return Point { x: 0.0, y: 0.0 };
    }
    let sum_x: f64 = p.points.iter().map(|pt| pt.x).sum();
    let sum_y: f64 = p.points.iter().map(|pt| pt.y).sum();
    let n = p.points.len() as f64;
    Point {
        x: sum_x / n,
        y: sum_y / n,
    }
}

#[data]
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Team {
    pub name: String,
    pub members: Vec<String>,
}

#[export]
pub fn echo_team(t: Team) -> Team {
    t
}

#[export]
pub fn make_team(name: String, members: Vec<String>) -> Team {
    Team { name, members }
}

#[export]
pub fn team_size(t: Team) -> u32 {
    t.members.len() as u32
}

#[data]
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Classroom {
    pub students: Vec<Person>,
}

#[export]
pub fn echo_classroom(c: Classroom) -> Classroom {
    c
}

#[export]
pub fn make_classroom(students: Vec<Person>) -> Classroom {
    Classroom { students }
}
