#![no_std]
#![no_main]
use core::panic::PanicInfo;
#[unsafe(no_mangle)]
extern "C" fn _start(_: usize, _: *const *const u8, _: usize, _: *const *const u8) -> ! {
    let mut buffer = [0u8; 4096];
    let length = imagineos::kernel_log::read(&mut buffer).unwrap_or(0);
    let _ = imagineos::console::write_all(&buffer[..length]);
    imagineos::process::exit(0)
}
#[panic_handler]
fn panic(_: &PanicInfo) -> ! { imagineos::process::exit(101) }
