# Miscellaneous TODO

## Race condition in analytics testing - does not fail with thread=1 

failures:

---- analytics::api::tests::log_event_records_entry stdout ----

thread 'analytics::api::tests::log_event_records_entry' panicked at src\analytics\api.rs:137:50:
called `Result::unwrap()` on an `Err` value: AnalyticsError { code: Internal, message: "Analytics component not available" }
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace


failures:
    analytics::api::tests::log_event_records_entry

test result: FAILED. 186 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.98s

error: test failed, to rerun pass `--lib`
