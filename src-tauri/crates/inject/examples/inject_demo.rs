//! Manual verification for fude-inject. Not run in CI — requires a human
//! at the keyboard to focus a text field before injection fires.
//!
//! Usage: `cargo run -p fude-inject --example inject_demo`
//! Then click into any text field (TextEdit, a browser address bar, a
//! terminal) within the 3-second countdown.

use std::time::Duration;
use fude_inject::inject_auto;

fn main() {
    env_logger::init();

    println!("fude-inject demo");
    println!("Click into a text field now. Injecting in 3 seconds...");
    std::thread::sleep(Duration::from_secs(3));

    match inject_auto("fude injection test — paperback") {
        Ok(method) => println!("Injected successfully via: {method}"),
        Err(e) => eprintln!("Injection failed: {e}"),
    }
}
