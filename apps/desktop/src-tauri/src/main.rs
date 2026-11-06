// Release builds link against the Windows GUI subsystem so no console is allocated; debug
// builds keep it for `diagnostic_event` stderr output and panic messages.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    courage_desktop_lib::run();
}
