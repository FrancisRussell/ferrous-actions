use chrono::{DateTime, FixedOffset, Local, NaiveDateTime, TimeZone as _};

pub fn now() -> DateTime<Local> {
    let date_js = js_sys::Date::new_0();
    let naive_utc =
        NaiveDateTime::from_timestamp_millis(date_js.value_of() as i64).expect("Out of range date");
    let offset = FixedOffset::west_opt(60 * date_js.get_timezone_offset() as i32)
        .expect("seconds out of bounds");
    let timezone = Local::from_offset(&offset);
    timezone.from_utc_datetime(&naive_utc)
}
