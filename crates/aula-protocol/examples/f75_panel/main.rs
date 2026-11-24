//! Original Win32 panel for the Keylux fork. MIT OR Apache-2.0.
#![cfg_attr(all(windows, not(test)), windows_subsystem = "windows")]
#[cfg(any(windows, test))]
mod profiles;
#[cfg(windows)]
mod windows;
#[cfg(windows)]
fn main() {
    windows::run();
}
#[cfg(not(windows))]
fn main() {
    eprintln!("O painel gráfico é para Windows.");
}
