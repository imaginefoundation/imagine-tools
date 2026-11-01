use core::panic::PanicInfo;

#[allow(unused_imports)]
pub use imagineos::legacy::{
    argument, clear, environment_value, exit, list_directory, mkdir, read_char, read_fd, read_file,
    read_stdin, remove, touch, write, write_file,
};

/// Resolve `path` contra `PWD` do ambiente do processo.
///
/// # Safety
/// `envp` deve apontar para o vetor de ambiente recebido em `_start` (o mesmo
/// contrato de `imagineos::legacy::resolve_path`).
#[allow(dead_code)]
pub unsafe fn resolve_path(
    envp: *const *const u8,
    envc: usize,
    path: &[u8],
    output: &mut [u8],
) -> Option<usize> {
    // SAFETY: contrato delegado ao chamador (ver doc acima).
    unsafe { imagineos::legacy::resolve_path(envp, envc, path, output) }
}

#[panic_handler]
fn panic(_info: &PanicInfo<'_>) -> ! {
    imagineos::process::exit(127)
}
