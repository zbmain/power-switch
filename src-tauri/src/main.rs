#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

/// Launch the native shell; model configuration logic stays in the testable library.
fn main() {
    power_switch::run();
}
