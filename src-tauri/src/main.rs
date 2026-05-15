// Prevent extra console window on Windows in release; keep it in debug.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    opensplit_lib::run();
}
