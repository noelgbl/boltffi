use boltffi::*;

#[data]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Status {
    #[default]
    Active,
    Inactive,
    Pending,
}

#[export]
pub fn echo_status(s: Status) -> Status {
    s
}

#[export]
pub fn status_to_string(s: Status) -> String {
    match s {
        Status::Active => "active".to_string(),
        Status::Inactive => "inactive".to_string(),
        Status::Pending => "pending".to_string(),
    }
}

#[data]
#[derive(Clone, Debug, PartialEq)]
pub enum Shape {
    Circle { radius: f64 },
    Rectangle { width: f64, height: f64 },
    Point,
}

#[export]
pub fn echo_shape(s: Shape) -> Shape {
    s
}

#[export]
pub fn shape_area(s: Shape) -> f64 {
    match s {
        Shape::Circle { radius } => std::f64::consts::PI * radius * radius,
        Shape::Rectangle { width, height } => width * height,
        Shape::Point => 0.0,
    }
}

#[export]
pub fn make_circle(radius: f64) -> Shape {
    Shape::Circle { radius }
}

#[export]
pub fn make_rectangle(width: f64, height: f64) -> Shape {
    Shape::Rectangle { width, height }
}
