use chrono::{DateTime, FixedOffset, Local, NaiveDateTime, TimeZone as _, Utc};

pub fn now_utc() -> DateTime<Utc> {
    let date_js = js_sys::Date::new_0();
    #[allow(clippy::cast_possible_truncation)]
    let millis = date_js.value_of() as i64;
    let naive_utc = NaiveDateTime::from_timestamp_millis(millis).expect("Out of range date");
    DateTime::<Utc>::from_utc(naive_utc, Utc)
}

pub fn now_local() -> DateTime<Local> {
    let date_js = js_sys::Date::new_0();
    #[allow(clippy::cast_possible_truncation)]
    let millis = date_js.value_of() as i64;
    let naive_utc = NaiveDateTime::from_timestamp_millis(millis).expect("Out of range date");
    #[allow(clippy::cast_possible_truncation)]
    let offset_minutes = 60 * date_js.get_timezone_offset() as i32;
    let offset = FixedOffset::west_opt(offset_minutes).expect("seconds out of bounds");
    let timezone = Local::from_offset(&offset);
    timezone.from_utc_datetime(&naive_utc)
}
