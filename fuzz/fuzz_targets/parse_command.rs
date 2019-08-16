#![no_main]

#[macro_use]
extern crate libfuzzer_sys;
extern crate libunftp;

fuzz_target!(|data: &[u8]| {
    let _ = libunftp::commands::Command::parse(data);
});
