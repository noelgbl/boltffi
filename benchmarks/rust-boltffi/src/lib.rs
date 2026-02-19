use std::sync::Arc;

use boltffi::*;

/// A physical location with geographic coordinates and metadata.
///
/// Used to represent points of interest such as restaurants or landmarks.
#[derive(Clone, Copy)]
#[data]
pub struct Location {
    /// Unique identifier for this location.
    pub id: i64,
    /// Latitude in decimal degrees (WGS 84).
    pub lat: f64,
    /// Longitude in decimal degrees (WGS 84).
    pub lng: f64,
    /// Average user rating on a 0.0 to 5.0 scale.
    pub rating: f64,
    /// Total number of user reviews submitted.
    pub review_count: i32,
    /// Whether the location is currently open for business.
    pub is_open: bool,
}

/// A single executed trade on a financial exchange.
#[derive(Clone, Copy)]
#[data]
pub struct Trade {
    /// Unique trade identifier.
    pub id: i64,
    /// Numeric symbol identifier for the traded instrument.
    pub symbol_id: i32,
    /// Execution price of the trade.
    pub price: f64,
    /// Number of units traded.
    pub quantity: i64,
    /// Best bid price at time of execution.
    pub bid: f64,
    /// Best ask price at time of execution.
    pub ask: f64,
    /// Cumulative volume for this instrument today.
    pub volume: i64,
    /// Unix timestamp in milliseconds when the trade occurred.
    pub timestamp: i64,
    /// Whether this was a buy-side initiated trade.
    pub is_buy: bool,
}

/// A particle in a three-dimensional physics simulation.
#[derive(Clone, Copy)]
#[data]
pub struct Particle {
    /// Unique particle identifier.
    pub id: i64,
    /// Position along the X axis.
    pub x: f64,
    /// Position along the Y axis.
    pub y: f64,
    /// Position along the Z axis.
    pub z: f64,
    /// Velocity component along the X axis.
    pub vx: f64,
    /// Velocity component along the Y axis.
    pub vy: f64,
    /// Velocity component along the Z axis.
    pub vz: f64,
    /// Mass of the particle in arbitrary units.
    pub mass: f64,
    /// Electric charge of the particle.
    pub charge: f64,
    /// Whether this particle is active in the simulation.
    pub active: bool,
}

/// A timestamped reading from an environmental sensor.
#[derive(Clone, Copy)]
#[data]
pub struct SensorReading {
    /// Identifier of the sensor that produced this reading.
    pub sensor_id: i64,
    /// Unix timestamp in milliseconds when the reading was taken.
    pub timestamp: i64,
    /// Ambient temperature in degrees Celsius.
    pub temperature: f64,
    /// Relative humidity as a percentage (0 to 100).
    pub humidity: f64,
    /// Atmospheric pressure in hectopascals (hPa).
    pub pressure: f64,
    /// Ambient light level in lux.
    pub light: f64,
    /// Battery level as a percentage (0 to 100).
    pub battery: f64,
    /// Signal strength in dBm (typically negative).
    pub signal_strength: i32,
    /// Whether this reading passed validation checks.
    pub is_valid: bool,
}

/// A user profile containing personal information and engagement metrics.
#[derive(Clone)]
#[data]
pub struct UserProfile {
    /// Unique user identifier.
    pub id: i64,
    /// Display name of the user.
    pub name: String,
    /// Primary email address.
    pub email: String,
    /// Free-text biography written by the user.
    pub bio: String,
    /// Age of the user in years.
    pub age: i32,
    /// Aggregate engagement score.
    pub score: f64,
    /// Freeform tags associated with this profile.
    pub tags: Vec<String>,
    /// Historical score values.
    pub scores: Vec<i32>,
    /// Whether the account is currently active.
    pub is_active: bool,
}

/// Generates a vector of synthetic user profiles for benchmarking.
#[export]
pub fn generate_user_profiles(count: i32) -> Vec<UserProfile> {
    (0..count as i64)
        .map(|i| UserProfile {
            id: i,
            name: format!("User {}", i),
            email: format!("user{}@example.com", i),
            bio: format!(
                "This is a bio for user {}. It contains some text to make it realistic.",
                i
            ),
            age: 20 + (i % 50) as i32,
            score: (i as f64) * 1.5,
            tags: vec![
                format!("tag{}", i % 5),
                format!("category{}", i % 3),
                "common".to_string(),
            ],
            scores: vec![
                (i % 100) as i32,
                ((i + 10) % 100) as i32,
                ((i + 20) % 100) as i32,
            ],
            is_active: i % 2 == 0,
        })
        .collect()
}

/// Sums the engagement scores of all provided user profiles.
#[export]
pub fn sum_user_scores(users: Vec<UserProfile>) -> f64 {
    users.iter().map(|u| u.score).sum()
}

/// Counts the number of active user profiles.
#[export]
pub fn count_active_users(users: Vec<UserProfile>) -> i32 {
    users.iter().filter(|u| u.is_active).count() as i32
}

/// A no-operation function used for measuring FFI call overhead.
#[export]
pub fn noop() {}

/// Returns the given 32-bit integer unchanged (echo benchmark).
#[export]
pub fn echo_i32(value: i32) -> i32 {
    value
}

/// Returns the given 64-bit float unchanged (echo benchmark).
#[export]
pub fn echo_f64(value: f64) -> f64 {
    value
}

/// Returns the given string unchanged (echo benchmark).
#[export]
pub fn echo_string(value: &str) -> String {
    value.to_string()
}

/// Adds two 32-bit integers.
#[export]
pub fn add(a: i32, b: i32) -> i32 {
    a + b
}

/// Asynchronously adds two 32-bit integers.
#[export]
pub async fn async_add(a: i32, b: i32) -> i32 {
    a + b
}

/// Multiplies two 64-bit floats.
#[export]
pub fn multiply(a: f64, b: f64) -> f64 {
    a * b
}

/// Generates a string of the given size by repeating the character 'x'.
#[export]
pub fn generate_string(size: i32) -> String {
    "x".repeat(size as usize)
}

/// Generates a vector of synthetic locations for benchmarking.
#[export]
pub fn generate_locations(count: i32) -> Vec<Location> {
    (0..count)
        .map(|i| Location {
            id: i as i64,
            lat: 37.7749 + (i as f64 * 0.001),
            lng: -122.4194 + (i as f64 * 0.001),
            rating: 3.0 + ((i % 20) as f64 * 0.1),
            review_count: 10 + (i * 5),
            is_open: i % 2 == 0,
        })
        .collect()
}

/// Returns the number of locations in the given vector.
#[export]
pub fn process_locations(locations: Vec<Location>) -> i32 {
    locations.len() as i32
}

/// Sums the ratings of all provided locations.
#[export]
pub fn sum_ratings(locations: Vec<Location>) -> f64 {
    locations.iter().map(|l| l.rating).sum()
}

/// Generates a byte vector filled with the value 42.
#[export]
pub fn generate_bytes(size: i32) -> Vec<u8> {
    vec![42u8; size as usize]
}

/// Generates a vector of sequential 32-bit integers from 0 to count-1.
#[export]
pub fn generate_i32_vec(count: i32) -> Vec<i32> {
    (0..count).collect()
}

/// Sums a vector of 32-bit integers into a 64-bit result.
#[export]
pub fn sum_i32_vec(values: Vec<i32>) -> i64 {
    values.iter().map(|&v| v as i64).sum()
}

/// Divides two integers returning an error on division by zero.
#[export]
pub fn divide(a: i32, b: i32) -> Result<i32, &'static str> {
    if b == 0 {
        Err("division by zero")
    } else {
        Ok(a / b)
    }
}

/// Parses a string as a 32-bit integer.
#[export]
pub fn parse_int(s: &str) -> Result<i32, &'static str> {
    s.parse().map_err(|_| "invalid integer")
}

/// Validates and greets a user by name.
#[export]
pub fn validate_name(name: &str) -> Result<String, &'static str> {
    if name.is_empty() {
        Err("name cannot be empty")
    } else if name.len() > 100 {
        Err("name too long")
    } else {
        Ok(format!("Hello, {}!", name))
    }
}

/// A fallible no-op that succeeds or fails based on the flag.
#[export]
pub fn try_do_nothing(fail: bool) -> Result<(), &'static str> {
    if fail {
        Err("intentional failure")
    } else {
        Ok(())
    }
}

/// Fetches a location by its ID, returning an error for negative IDs.
#[export]
pub fn fetch_location(id: i32) -> Result<Location, &'static str> {
    if id < 0 {
        Err("invalid id")
    } else {
        Ok(Location {
            id: id as i64,
            lat: 37.7749,
            lng: -122.4194,
            rating: 4.5,
            review_count: 100,
            is_open: true,
        })
    }
}

/// Converts compass degrees (0-359) to a cardinal direction.
#[export]
pub fn get_direction(degrees: i32) -> Result<Direction, &'static str> {
    match degrees {
        0..=89 => Ok(Direction::North),
        90..=179 => Ok(Direction::East),
        180..=269 => Ok(Direction::South),
        270..=359 => Ok(Direction::West),
        _ => Err("degrees must be 0-359"),
    }
}

/// Processes a value and returns an API result, or an error if too negative.
#[export]
pub fn try_process_value(value: i32) -> Result<ApiResult, &'static str> {
    if value < -100 {
        Err("value too negative")
    } else if value > 0 {
        Ok(ApiResult::Success)
    } else if value == 0 {
        Ok(ApiResult::ErrorCode(-1))
    } else {
        Ok(ApiResult::ErrorWithData {
            code: value,
            detail: value * 2,
        })
    }
}

// TODO: Result<Vec<_>, _> not yet supported in JNI
// #[export]
// pub fn try_generate_bytes(size: i32) -> Result<Vec<u8>, &'static str> {
//     if size < 0 {
//         Err("size must be non-negative")
//     } else if size > 10000 {
//         Err("size too large")
//     } else {
//         Ok(vec![42u8; size as usize])
//     }
// }

// #[export]
// pub fn try_generate_locations(count: i32) -> Result<Vec<Location>, &'static str> {
//     if count < 0 {
//         Err("count must be non-negative")
//     } else if count > 1000 {
//         Err("count too large")
//     } else {
//         Ok((0..count)
//             .map(|i| Location {
//                 id: i as i64,
//                 lat: 37.7749 + (i as f64 * 0.01),
//                 lng: -122.4194 + (i as f64 * 0.01),
//                 rating: 3.0 + (i as f64 % 2.0),
//                 review_count: 10 + i,
//                 is_open: i % 2 == 0,
//             })
//             .collect())
//     }
// }

/// Generates a vector of synthetic trades for benchmarking.
#[export]
pub fn generate_trades(count: i32) -> Vec<Trade> {
    (0..count)
        .map(|i| Trade {
            id: i as i64,
            symbol_id: i % 500,
            price: 100.0 + (i as f64 * 0.01),
            quantity: (i as i64 % 1000) + 1,
            bid: 99.95 + (i as f64 * 0.01),
            ask: 100.05 + (i as f64 * 0.01),
            volume: (i as i64) * 1000,
            timestamp: 1700000000000 + (i as i64 * 1000),
            is_buy: i % 2 == 0,
        })
        .collect()
}

/// Generates a vector of synthetic particles for benchmarking.
#[export]
pub fn generate_particles(count: i32) -> Vec<Particle> {
    (0..count)
        .map(|i| Particle {
            id: i as i64,
            x: (i as f64) * 0.1,
            y: (i as f64) * 0.2,
            z: (i as f64) * 0.3,
            vx: (i as f64) * 0.01,
            vy: (i as f64) * 0.02,
            vz: (i as f64) * 0.03,
            mass: 1.0 + (i as f64 * 0.001),
            charge: if i % 2 == 0 { 1.0 } else { -1.0 },
            active: i % 10 != 0,
        })
        .collect()
}

/// Generates a vector of synthetic sensor readings for benchmarking.
#[export]
pub fn generate_sensor_readings(count: i32) -> Vec<SensorReading> {
    (0..count)
        .map(|i| SensorReading {
            sensor_id: (i % 100) as i64,
            timestamp: 1700000000000 + (i as i64 * 100),
            temperature: 20.0 + ((i % 30) as f64),
            humidity: 40.0 + ((i % 40) as f64),
            pressure: 1013.25 + ((i % 20) as f64),
            light: (i % 1000) as f64,
            battery: 100.0 - ((i % 100) as f64),
            signal_strength: -50 - (i % 50),
            is_valid: i % 20 != 0,
        })
        .collect()
}

/// Sums the volume of all provided trades.
#[export]
pub fn sum_trade_volumes(trades: Vec<Trade>) -> i64 {
    trades.iter().map(|t| t.volume).sum()
}

/// Sums the mass of all provided particles.
#[export]
pub fn sum_particle_masses(particles: Vec<Particle>) -> f64 {
    particles.iter().map(|p| p.mass).sum()
}

/// Computes the average temperature across all sensor readings.
#[export]
pub fn avg_sensor_temperature(readings: Vec<SensorReading>) -> f64 {
    let sum: f64 = readings.iter().map(|r| r.temperature).sum();
    sum / readings.len() as f64
}

/// Generates a vector of 64-bit floats spaced by 0.1.
#[export]
pub fn generate_f64_vec(count: i32) -> Vec<f64> {
    (0..count).map(|i| i as f64 * 0.1).collect()
}

/// Sums a vector of 64-bit floats.
#[export]
pub fn sum_f64_vec(values: Vec<f64>) -> f64 {
    values.iter().sum()
}

/// Increments the first element of a u64 slice by one.
#[export]
pub fn inc_u64(value: &mut [u64]) {
    if let Some(v) = value.first_mut() {
        *v += 1;
    }
}

/// Thread-safe 64-bit counter using Mutex (comparable to UniFFI).
#[derive(Default)]
pub struct Counter {
    value: std::sync::Mutex<u64>,
}

#[export]
impl Counter {
    /// Creates a new counter starting at zero.
    pub fn new() -> Self {
        Self {
            value: std::sync::Mutex::new(0),
        }
    }

    /// Sets the counter to the given value.
    pub fn set(&self, value: u64) {
        *self.value.lock().unwrap() = value;
    }

    /// Increments the counter by one.
    pub fn increment(&self) {
        *self.value.lock().unwrap() += 1;
    }

    /// Returns the current counter value.
    pub fn get(&self) -> u64 {
        *self.value.lock().unwrap()
    }
}

/// Single-threaded 64-bit counter without Mutex (BoltFFI-only, not comparable to UniFFI).
#[derive(Default)]
pub struct CounterSingleThreaded {
    value: u64,
}

#[export(single_threaded)]
impl CounterSingleThreaded {
    /// Creates a new counter starting at zero.
    pub fn new() -> Self {
        Self { value: 0 }
    }

    /// Sets the counter to the given value.
    pub fn set(&mut self, value: u64) {
        self.value = value;
    }

    /// Increments the counter by one.
    pub fn increment(&mut self) {
        self.value += 1;
    }

    /// Returns the current counter value.
    pub fn get(&self) -> u64 {
        self.value
    }
}

/// A two-dimensional data point with a timestamp.
#[data]
#[derive(Clone, Copy, Debug, Default)]
pub struct DataPoint {
    /// X coordinate value.
    pub x: f64,
    /// Y coordinate value.
    pub y: f64,
    /// Unix timestamp in milliseconds.
    pub timestamp: i64,
}

/// A collection of data points with various query and mutation methods.
#[derive(Default)]
pub struct DataStore {
    items: Vec<DataPoint>,
}

#[export(single_threaded)]
impl DataStore {
    /// Creates an empty data store.
    pub fn new() -> Self {
        Self { items: Vec::new() }
    }

    /// Creates a data store pre-populated with sample data.
    pub fn with_sample_data() -> Self {
        Self {
            items: vec![
                DataPoint {
                    x: 1.0,
                    y: 2.0,
                    timestamp: 100,
                },
                DataPoint {
                    x: 3.0,
                    y: 4.0,
                    timestamp: 200,
                },
                DataPoint {
                    x: 5.0,
                    y: 6.0,
                    timestamp: 300,
                },
            ],
        }
    }

    /// Creates an empty data store with the given pre-allocated capacity.
    pub fn with_capacity(capacity: i32) -> Self {
        Self {
            items: Vec::with_capacity(capacity as usize),
        }
    }

    /// Creates a data store containing a single initial point.
    pub fn with_initial_point(x: f64, y: f64, timestamp: i64) -> Self {
        Self {
            items: vec![DataPoint { x, y, timestamp }],
        }
    }

    /// Appends a data point to the store.
    pub fn add(&mut self, point: DataPoint) {
        self.items.push(point);
    }

    /// Returns the number of data points stored.
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Returns true if the store contains no data points.
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Copies stored data points into the destination buffer, returning the count copied.
    pub fn copy_into(&self, dst: &mut [DataPoint]) -> usize {
        let len = self.items.len().min(dst.len());
        dst[..len].copy_from_slice(&self.items[..len]);
        len
    }

    /// Invokes the callback for each stored data point.
    pub fn foreach(&self, mut callback: impl FnMut(DataPoint)) {
        self.items.iter().for_each(|p| callback(*p));
    }

    /// Returns the sum of (x + y) for all stored data points.
    pub fn sum(&self) -> f64 {
        self.items.iter().map(|p| p.x + p.y).sum()
    }

    /// Asynchronously computes the sum, returning an error if the store is empty.
    pub async fn async_sum(&self) -> Result<f64, &'static str> {
        std::thread::sleep(std::time::Duration::from_millis(10));
        if self.items.is_empty() {
            Err("no items to sum")
        } else {
            Ok(self.items.iter().map(|p| p.x + p.y).sum())
        }
    }

    /// Asynchronously returns the number of stored data points.
    pub async fn async_len(&self) -> Result<usize, &'static str> {
        std::thread::sleep(std::time::Duration::from_millis(5));
        Ok(self.items.len())
    }
}

/// Thread-safe signed 64-bit accumulator using Mutex (comparable to UniFFI).
#[derive(Default)]
pub struct Accumulator {
    value: std::sync::Mutex<i64>,
}

#[export]
impl Accumulator {
    /// Creates a new accumulator starting at zero.
    pub fn new() -> Self {
        Self {
            value: std::sync::Mutex::new(0),
        }
    }

    /// Adds the given amount to the accumulated value.
    pub fn add(&self, amount: i64) {
        *self.value.lock().unwrap() += amount;
    }

    /// Returns the current accumulated value.
    pub fn get(&self) -> i64 {
        *self.value.lock().unwrap()
    }

    /// Resets the accumulated value to zero.
    pub fn reset(&self) {
        *self.value.lock().unwrap() = 0;
    }
}

/// Single-threaded signed 64-bit accumulator without Mutex (BoltFFI-only).
#[derive(Default)]
pub struct AccumulatorSingleThreaded {
    value: i64,
}

#[export(single_threaded)]
impl AccumulatorSingleThreaded {
    /// Creates a new accumulator starting at zero.
    pub fn new() -> Self {
        Self { value: 0 }
    }

    /// Adds the given amount to the accumulated value.
    pub fn add(&mut self, amount: i64) {
        self.value += amount;
    }

    /// Returns the current accumulated value.
    pub fn get(&self) -> i64 {
        self.value
    }

    /// Resets the accumulated value to zero.
    pub fn reset(&mut self) {
        self.value = 0;
    }
}

/// A cardinal compass direction.
#[data]
#[repr(i32)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Direction {
    /// Pointing toward the north pole (0 degrees).
    North = 0,
    /// Pointing east (90 degrees).
    East = 1,
    /// Pointing toward the south pole (180 degrees).
    South = 2,
    /// Pointing west (270 degrees).
    West = 3,
}

/// Returns the opposite cardinal direction.
#[export]
pub fn opposite_direction(dir: Direction) -> Direction {
    match dir {
        Direction::North => Direction::South,
        Direction::East => Direction::West,
        Direction::South => Direction::North,
        Direction::West => Direction::East,
    }
}

/// Converts a cardinal direction to its compass bearing in degrees.
#[export]
pub fn direction_to_degrees(dir: Direction) -> i32 {
    match dir {
        Direction::North => 0,
        Direction::East => 90,
        Direction::South => 180,
        Direction::West => 270,
    }
}

/// Returns the value if it is even, or None if odd.
#[export]
pub fn find_even(value: i32) -> Option<i32> {
    if value % 2 == 0 {
        Some(value)
    } else {
        None
    }
}

/// Returns the value if positive, or None otherwise.
#[export]
pub fn find_positive_i64(value: i64) -> Option<i64> {
    if value > 0 {
        Some(value)
    } else {
        None
    }
}

/// Returns the float value if positive, or None otherwise.
#[export]
pub fn find_positive_f64(value: f64) -> Option<f64> {
    if value > 0.0 {
        Some(value)
    } else {
        None
    }
}

/// Looks up a name by ID, returning None for non-positive IDs.
#[export]
pub fn find_name(id: i32) -> Option<String> {
    if id > 0 {
        Some(format!("Name_{}", id))
    } else {
        None
    }
}

/// Looks up a location by ID, returning None for non-positive IDs.
#[export]
pub fn find_location(id: i32) -> Option<Location> {
    if id > 0 {
        Some(Location {
            id: id as i64,
            lat: 37.7749,
            lng: -122.4194,
            rating: 4.5,
            review_count: 100,
            is_open: true,
        })
    } else {
        None
    }
}

/// Generates a sequential number vector if count is positive, or None otherwise.
#[export]
pub fn find_numbers(count: i32) -> Option<Vec<i32>> {
    if count > 0 {
        Some((0..count).collect())
    } else {
        None
    }
}

/// Generates a location vector if count is positive, or None otherwise.
#[export]
pub fn find_locations(count: i32) -> Option<Vec<Location>> {
    if count > 0 {
        Some(
            (0..count)
                .map(|i| Location {
                    id: i as i64,
                    lat: 37.7749 + (i as f64 * 0.01),
                    lng: -122.4194 + (i as f64 * 0.01),
                    rating: 3.0 + (i as f64 * 0.1),
                    review_count: 10 + i,
                    is_open: i % 2 == 0,
                })
                .collect(),
        )
    } else {
        None
    }
}

/// Maps an integer ID (0-3) to a cardinal direction, or None if out of range.
#[export]
pub fn find_direction(id: i32) -> Option<Direction> {
    match id {
        0 => Some(Direction::North),
        1 => Some(Direction::East),
        2 => Some(Direction::South),
        3 => Some(Direction::West),
        _ => None,
    }
}

/// Maps an integer code to a predefined API result, or None if unknown.
#[export]
pub fn find_api_result(code: i32) -> Option<ApiResult> {
    match code {
        0 => Some(ApiResult::Success),
        1 => Some(ApiResult::ErrorCode(-1)),
        2 => Some(ApiResult::ErrorWithData {
            code: -1,
            detail: -2,
        }),
        _ => None,
    }
}

/// Generates a name vector if count is positive, or None otherwise.
#[export]
pub fn find_names(count: i32) -> Option<Vec<String>> {
    if count > 0 {
        Some((0..count).map(|i| format!("Name_{}", i)).collect())
    } else {
        None
    }
}

/// Generates a direction vector cycling through all four directions, or None if count is non-positive.
#[export]
pub fn find_directions(count: i32) -> Option<Vec<Direction>> {
    if count > 0 {
        Some(
            (0..count)
                .map(|i| match i % 4 {
                    0 => Direction::North,
                    1 => Direction::East,
                    2 => Direction::South,
                    _ => Direction::West,
                })
                .collect(),
        )
    } else {
        None
    }
}

/// The outcome of an API call.
#[data]
#[derive(Clone, Copy, Debug)]
pub enum ApiResult {
    /// The operation completed successfully.
    Success = 0,
    /// The operation failed with an error code.
    ErrorCode(i32) = 1,
    /// The operation failed with a detailed error payload.
    ErrorWithData {
        /// Numeric error code.
        code: i32,
        /// Additional detail value describing the failure.
        detail: i32,
    } = 2,
}

/// Errors that can occur during computation.
#[error]
#[derive(Clone, Copy, Debug)]
pub enum ComputeError {
    /// The input value was not valid for the operation.
    InvalidInput(i32) = 0,
    /// The computation overflowed the allowed range.
    Overflow {
        /// The value that caused the overflow.
        value: i32,
        /// The maximum allowed value.
        limit: i32,
    } = 1,
}

/// Converts an integer value into an API result based on its sign.
#[export]
pub fn process_value(value: i32) -> ApiResult {
    if value > 0 {
        ApiResult::Success
    } else if value == 0 {
        ApiResult::ErrorCode(-1)
    } else {
        ApiResult::ErrorWithData {
            code: value,
            detail: value * 2,
        }
    }
}

/// Returns true if the API result represents success.
#[export]
pub fn api_result_is_success(result: ApiResult) -> bool {
    matches!(result, ApiResult::Success)
}

/// Doubles the value if positive, or returns a typed error.
#[export]
pub fn try_compute(value: i32) -> Result<i32, ComputeError> {
    if value > 0 {
        Ok(value * 2)
    } else if value == 0 {
        Err(ComputeError::InvalidInput(-999))
    } else {
        Err(ComputeError::Overflow { value, limit: 0 })
    }
}

/// Simulates a heavy computation by sleeping then doubling the input.
#[export]
pub async fn compute_heavy(input: i32) -> i32 {
    std::thread::sleep(std::time::Duration::from_millis(100));
    input * 2
}

/// Asynchronous version of try_compute.
#[export]
pub async fn try_compute_async(value: i32) -> Result<i32, ComputeError> {
    std::thread::sleep(std::time::Duration::from_millis(10));
    try_compute(value)
}

/// Asynchronously fetches data by ID, returning an error for non-positive IDs.
#[export]
pub async fn fetch_data(id: i32) -> Result<i32, &'static str> {
    std::thread::sleep(std::time::Duration::from_millis(50));
    if id > 0 {
        Ok(id * 10)
    } else {
        Err("invalid id")
    }
}

/// Asynchronously formats a value into a descriptive string.
#[export]
pub async fn async_make_string(value: i32) -> String {
    std::thread::sleep(std::time::Duration::from_millis(30));
    format!("Value is: {}", value)
}

/// Asynchronously creates a data point from the given coordinates.
#[export]
pub async fn async_fetch_point(x: f64, y: f64) -> DataPoint {
    std::thread::sleep(std::time::Duration::from_millis(20));
    DataPoint {
        x,
        y,
        timestamp: 12345,
    }
}

/// Asynchronously generates a sequential integer vector.
#[export]
pub async fn async_get_numbers(count: i32) -> Vec<i32> {
    std::thread::sleep(std::time::Duration::from_millis(20));
    (0..count).collect()
}

// #[export]
// pub async fn async_find_value(needle: i32) -> Option<i32> {
//     std::thread::sleep(std::time::Duration::from_millis(10));
//     if needle > 0 { Some(needle * 100) } else { None }
// }

/// A single real-time reading emitted by a sensor stream.
#[data]
#[derive(Clone, Copy, Debug, Default)]
pub struct StreamReading {
    /// Identifier of the sensor that produced this reading.
    pub sensor_id: i32,
    /// Timestamp in milliseconds since epoch.
    pub timestamp_ms: i64,
    /// The measured value.
    pub value: f64,
}

/// Monitors a sensor and produces a stream of readings for subscribers.
pub struct SensorMonitor {
    readings_producer: StreamProducer<StreamReading>,
}

impl Default for SensorMonitor {
    fn default() -> Self {
        Self::new()
    }
}

#[export]
impl SensorMonitor {
    /// Creates a new sensor monitor with an internal ring buffer of 512 slots.
    pub fn new() -> Self {
        Self {
            readings_producer: StreamProducer::new(512),
        }
    }

    /// Returns a subscription to the readings stream.
    #[ffi_stream(item = StreamReading)]
    pub fn readings(&self) -> Arc<EventSubscription<StreamReading>> {
        self.readings_producer.subscribe()
    }

    /// Pushes a new reading to all active subscribers.
    pub fn emit_reading(&self, sensor_id: i32, timestamp_ms: i64, value: f64) {
        self.readings_producer.push(StreamReading {
            sensor_id,
            timestamp_ms,
            value,
        });
    }

    /// Returns the number of active subscribers.
    pub fn subscriber_count(&self) -> usize {
        self.readings_producer.subscriber_count()
    }
}

/// Provides indexed access to a collection of data points.
#[export]
pub trait DataProvider: Send + Sync {
    /// Returns the total number of available data points.
    fn get_count(&self) -> u32;
    /// Returns the data point at the given index.
    fn get_item(&self, index: u32) -> DataPoint;
}

/// Asynchronously fetches values by key.
#[export]
#[allow(async_fn_in_trait)]
pub trait AsyncDataFetcher {
    /// Fetches the value associated with the given key.
    async fn fetch_value(&self, key: u32) -> u64;
}

/// Consumes data from an injected provider and computes aggregate results.
#[derive(Default)]
pub struct DataConsumer {
    provider: Option<Box<dyn DataProvider + Send + Sync>>,
}

#[export(single_threaded)]
impl DataConsumer {
    /// Creates a new consumer with no provider set.
    pub fn new() -> Self {
        Self { provider: None }
    }

    /// Sets the data provider to use for subsequent computations.
    pub fn set_provider(&mut self, provider: Box<dyn DataProvider + Send + Sync>) {
        self.provider = Some(provider);
    }

    /// Computes the sum of (x + y) across all provider items, returning 0 if no provider is set.
    pub fn compute_sum(&self) -> u64 {
        let Some(ref provider) = self.provider else {
            return 0;
        };
        let count = provider.get_count();
        (0..count)
            .map(|i| {
                let point = provider.get_item(i);
                (point.x + point.y) as u64
            })
            .sum()
    }
}

// =============================================================================
// NESTED TYPE TESTS
// =============================================================================

/// A postal address.
#[data]
#[derive(Clone, Debug)]
pub struct Address {
    /// Street name and number.
    pub street: String,
    /// City name.
    pub city: String,
    /// Numeric postal code.
    pub zip_code: i32,
}

/// A person with a name, age, and residential address.
#[data]
#[derive(Clone, Debug)]
pub struct Person {
    /// Full name of the person.
    pub name: String,
    /// Age in years.
    pub age: i32,
    /// Residential address.
    pub address: Address,
}

/// A company with executive, employees, and headquarters.
#[data]
#[derive(Clone, Debug)]
pub struct Company {
    /// Legal name of the company.
    pub name: String,
    /// The chief executive officer.
    pub ceo: Person,
    /// List of company employees.
    pub employees: Vec<Person>,
    /// Physical headquarters address.
    pub headquarters: Address,
}

/// A two-dimensional coordinate.
#[data]
#[derive(Clone, Copy, Debug)]
pub struct Coordinate {
    /// Horizontal position.
    pub x: f64,
    /// Vertical position.
    pub y: f64,
}

/// An axis-aligned bounding box defined by its minimum and maximum corners.
#[data]
#[derive(Clone, Copy, Debug)]
pub struct BoundingBox {
    /// Lower-left corner.
    pub min: Coordinate,
    /// Upper-right corner.
    pub max: Coordinate,
}

/// A named geographic region with bounds and a set of interior points.
#[data]
#[derive(Clone, Debug)]
pub struct Region {
    /// Human-readable name of the region.
    pub name: String,
    /// Axis-aligned bounding box enclosing the region.
    pub bounds: BoundingBox,
    /// Interior points of interest within the region.
    pub points: Vec<Coordinate>,
}

/// Task priority level.
#[data]
#[repr(i32)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Priority {
    /// Lowest priority, processed last.
    Low = 0,
    /// Normal priority.
    Medium = 1,
    /// Elevated priority, processed before normal tasks.
    High = 2,
    /// Highest priority, requires immediate attention.
    Critical = 3,
}

/// Current execution status of a task.
#[data]
#[derive(Clone, Copy, Debug)]
pub enum TaskStatus {
    /// The task has not started yet.
    Pending = 0,
    /// The task is currently running.
    InProgress {
        /// Percentage of completion (0 to 100).
        progress: i32,
    } = 1,
    /// The task finished successfully.
    Completed {
        /// The computed result value.
        result: i32,
    } = 2,
    /// The task failed.
    Failed {
        /// Error code describing the failure.
        error_code: i32,
        /// Number of times the task was retried.
        retry_count: i32,
    } = 3,
}

/// A unit of work with priority, status, optional assignee, and nested subtasks.
#[data]
#[derive(Clone, Debug)]
pub struct WorkItem {
    /// Unique work item identifier.
    pub id: i64,
    /// Short title describing the work.
    pub title: String,
    /// Priority level of this work item.
    pub priority: Priority,
    /// Current execution status.
    pub status: TaskStatus,
    /// The person assigned to this work item, if any.
    pub assignee: Option<Person>,
    /// Child work items nested under this one.
    pub subtasks: Vec<WorkItem>,
}

/// A project containing work items, an owner, and collaborators.
#[data]
#[derive(Clone, Debug)]
pub struct Project {
    /// Name of the project.
    pub name: String,
    /// Top-level work items in the project.
    pub tasks: Vec<WorkItem>,
    /// Person who owns the project.
    pub owner: Person,
    /// People collaborating on the project.
    pub collaborators: Vec<Person>,
}

/// Creates an address from its components.
#[export]
pub fn create_address(street: &str, city: &str, zip_code: i32) -> Address {
    Address {
        street: street.to_string(),
        city: city.to_string(),
        zip_code,
    }
}

/// Creates a person with the given name, age, and address.
#[export]
pub fn create_person(name: &str, age: i32, address: Address) -> Person {
    Person {
        name: name.to_string(),
        age,
        address,
    }
}

/// Creates a company with the given CEO, employees, and headquarters.
#[export]
pub fn create_company(
    name: &str,
    ceo: Person,
    employees: Vec<Person>,
    headquarters: Address,
) -> Company {
    Company {
        name: name.to_string(),
        ceo,
        employees,
        headquarters,
    }
}

/// Returns the number of employees in the company.
#[export]
pub fn get_company_employee_count(company: Company) -> i32 {
    company.employees.len() as i32
}

/// Returns the city where the CEO lives.
#[export]
pub fn get_ceo_address_city(company: Company) -> String {
    company.ceo.address.city.clone()
}

/// Creates a bounding box from corner coordinates.
#[export]
pub fn create_bounding_box(min_x: f64, min_y: f64, max_x: f64, max_y: f64) -> BoundingBox {
    BoundingBox {
        min: Coordinate { x: min_x, y: min_y },
        max: Coordinate { x: max_x, y: max_y },
    }
}

/// Creates a named region with bounds and interior points.
#[export]
pub fn create_region(name: &str, bounds: BoundingBox, points: Vec<Coordinate>) -> Region {
    Region {
        name: name.to_string(),
        bounds,
        points,
    }
}

/// Computes the area of a region from its bounding box.
#[export]
pub fn get_region_area(region: Region) -> f64 {
    let width = region.bounds.max.x - region.bounds.min.x;
    let height = region.bounds.max.y - region.bounds.min.y;
    width * height
}

/// Creates a work item with no assignee and no subtasks.
#[export]
pub fn create_task(id: i64, title: &str, priority: Priority, status: TaskStatus) -> WorkItem {
    WorkItem {
        id,
        title: title.to_string(),
        priority,
        status,
        assignee: None,
        subtasks: vec![],
    }
}

/// Creates a work item assigned to the given person.
#[export]
pub fn create_task_with_assignee(
    id: i64,
    title: &str,
    priority: Priority,
    status: TaskStatus,
    assignee: Person,
) -> WorkItem {
    WorkItem {
        id,
        title: title.to_string(),
        priority,
        status,
        assignee: Some(assignee),
        subtasks: vec![],
    }
}

/// Creates a pending work item with the given subtasks.
#[export]
pub fn create_task_with_subtasks(
    id: i64,
    title: &str,
    priority: Priority,
    subtasks: Vec<WorkItem>,
) -> WorkItem {
    WorkItem {
        id,
        title: title.to_string(),
        priority,
        status: TaskStatus::Pending,
        assignee: None,
        subtasks,
    }
}

/// Recursively counts all subtasks (direct and nested).
#[export]
pub fn count_all_subtasks(task: WorkItem) -> i32 {
    fn count_recursive(task: &WorkItem) -> i32 {
        let direct = task.subtasks.len() as i32;
        let nested: i32 = task.subtasks.iter().map(count_recursive).sum();
        direct + nested
    }
    count_recursive(&task)
}

/// Creates a project with the given owner and work items.
#[export]
pub fn create_project(name: &str, owner: Person, tasks: Vec<WorkItem>) -> Project {
    Project {
        name: name.to_string(),
        tasks,
        owner,
        collaborators: vec![],
    }
}

/// Returns the number of top-level tasks in the project.
#[export]
pub fn get_project_task_count(project: Project) -> i32 {
    project.tasks.len() as i32
}

/// Finds the first task matching the given priority, if any.
#[export]
pub fn find_task_by_priority(project: Project, priority: Priority) -> Option<WorkItem> {
    project.tasks.into_iter().find(|t| t.priority == priority)
}

/// Returns all tasks with High or Critical priority.
#[export]
pub fn get_high_priority_tasks(project: Project) -> Vec<WorkItem> {
    project
        .tasks
        .into_iter()
        .filter(|t| t.priority == Priority::High || t.priority == Priority::Critical)
        .collect()
}

/// Creates a 2x2 grid of coordinates as nested vectors.
#[export]
pub fn create_nested_coordinates() -> Vec<Vec<Coordinate>> {
    vec![
        vec![Coordinate { x: 0.0, y: 0.0 }, Coordinate { x: 1.0, y: 0.0 }],
        vec![Coordinate { x: 0.0, y: 1.0 }, Coordinate { x: 1.0, y: 1.0 }],
    ]
}

/// Flattens nested coordinate vectors into a single vector.
#[export]
pub fn flatten_coordinates(nested: Vec<Vec<Coordinate>>) -> Vec<Coordinate> {
    nested.into_iter().flatten().collect()
}

/// Creates a person with a default address if has_address is true, or None.
#[export]
pub fn create_optional_person(name: &str, age: i32, has_address: bool) -> Option<Person> {
    if has_address {
        Some(Person {
            name: name.to_string(),
            age,
            address: Address {
                street: "123 Main St".to_string(),
                city: "Anytown".to_string(),
                zip_code: 12345,
            },
        })
    } else {
        None
    }
}

/// Extracts the task status if the work item is present.
#[export]
pub fn get_optional_task_status(task: Option<WorkItem>) -> Option<TaskStatus> {
    task.map(|t| t.status)
}

/// Extracts the progress or result value from a task status.
#[export]
pub fn get_status_progress(status: TaskStatus) -> i32 {
    match status {
        TaskStatus::Pending => 0,
        TaskStatus::InProgress { progress } => progress,
        TaskStatus::Completed { result } => result,
        TaskStatus::Failed { error_code, .. } => error_code,
    }
}

/// Returns true if the task status is Completed.
#[export]
pub fn is_status_complete(status: TaskStatus) -> bool {
    matches!(status, TaskStatus::Completed { .. })
}

/// A response from an API call containing the result or an error.
#[data]
#[derive(Clone, Debug)]
pub struct ApiResponse {
    /// The original request identifier.
    pub request_id: i64,
    /// The outcome: either a data point or a compute error.
    pub result: Result<DataPoint, ComputeError>,
}

/// Creates a successful API response wrapping the given data point.
#[export]
pub fn create_success_response(request_id: i64, point: DataPoint) -> ApiResponse {
    ApiResponse {
        request_id,
        result: Ok(point),
    }
}

/// Creates a failed API response wrapping the given error.
#[export]
pub fn create_error_response(request_id: i64, error: ComputeError) -> ApiResponse {
    ApiResponse {
        request_id,
        result: Err(error),
    }
}

/// Returns true if the API response contains a successful result.
#[export]
pub fn is_response_success(response: ApiResponse) -> bool {
    response.result.is_ok()
}

/// Extracts the data point from a successful response, or None on error.
#[export]
pub fn get_response_value(response: ApiResponse) -> Option<DataPoint> {
    response.result.ok()
}

#[export]
pub trait TypeExhaustiveCallback: Send + Sync {
    fn on_bool(&self, value: bool) -> bool;
    fn on_i8(&self, value: i8) -> i8;
    fn on_u8(&self, value: u8) -> u8;
    fn on_i16(&self, value: i16) -> i16;
    fn on_u16(&self, value: u16) -> u16;
    fn on_i32(&self, value: i32) -> i32;
    fn on_u32(&self, value: u32) -> u32;
    fn on_i64(&self, value: i64) -> i64;
    fn on_u64(&self, value: u64) -> u64;
    fn on_f32(&self, value: f32) -> f32;
    fn on_f64(&self, value: f64) -> f64;
    fn on_string(&self, value: String) -> String;
    fn on_blittable_record(&self, point: DataPoint) -> DataPoint;
    fn on_void(&self, tag: i32);
    fn on_multi_param(&self, flag: bool, count: i32, name: String, point: DataPoint) -> bool;
}

pub struct TypeExhaustiveRunner {
    callback: Box<dyn TypeExhaustiveCallback + Send + Sync>,
}

#[export]
impl TypeExhaustiveRunner {
    pub fn new(callback: Box<dyn TypeExhaustiveCallback + Send + Sync>) -> Self {
        Self { callback }
    }

    pub fn run_bool(&self, value: bool) -> bool {
        self.callback.on_bool(value)
    }

    pub fn run_i32(&self, value: i32) -> i32 {
        self.callback.on_i32(value)
    }

    pub fn run_string(&self, value: String) -> String {
        self.callback.on_string(value)
    }

    pub fn run_blittable(&self, point: DataPoint) -> DataPoint {
        self.callback.on_blittable_record(point)
    }

    pub fn run_void(&self, tag: i32) {
        self.callback.on_void(tag)
    }

    pub fn run_multi_param(&self, flag: bool, count: i32, name: String, point: DataPoint) -> bool {
        self.callback.on_multi_param(flag, count, name, point)
    }
}
