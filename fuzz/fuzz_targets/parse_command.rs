#![no_main]

#[macro_use]
extern crate libfuzzer_sys;
extern crate firetrap;

fuzz_target!(|data: &[u8]| {
    let _ = firetrap::commands::Command::parse(data);
});
