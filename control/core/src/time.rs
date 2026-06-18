pub use time::OffsetDateTime;

pub fn utc_now() -> OffsetDateTime {
    OffsetDateTime::now_utc()
}
