use std::time::{Duration, SystemTime, UNIX_EPOCH};

use boltffi::*;
use url::Url;
use uuid::Uuid;

#[export]
pub fn echo_duration(d: Duration) -> Duration {
    d
}

#[export]
pub fn make_duration(secs: u64, nanos: u32) -> Duration {
    Duration::new(secs, nanos)
}

#[export]
pub fn duration_as_millis(d: Duration) -> u64 {
    d.as_millis() as u64
}

#[export]
pub fn echo_system_time(t: SystemTime) -> SystemTime {
    t
}

#[export]
pub fn system_time_to_millis(t: SystemTime) -> u64 {
    t.duration_since(UNIX_EPOCH).unwrap().as_millis() as u64
}

#[export]
pub fn millis_to_system_time(millis: u64) -> SystemTime {
    UNIX_EPOCH + Duration::from_millis(millis)
}

#[export]
pub fn echo_uuid(id: Uuid) -> Uuid {
    id
}

#[export]
pub fn uuid_to_string(id: Uuid) -> String {
    id.to_string()
}

#[export]
pub fn echo_url(url: Url) -> Url {
    url
}

#[export]
pub fn url_to_string(url: Url) -> String {
    url.to_string()
}
