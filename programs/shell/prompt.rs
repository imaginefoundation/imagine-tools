//! Montagem do prompt (`user@host:cwd#`).
//!
//! Fica separado do editor de linha porque o prompt é redesenhado a cada tecla
//! (para o realce acompanhar a digitação) e também pelo próprio `Editor` ao
//! mover o cursor — centralizar aqui evita que as duas rotas imprimam formatos
//! diferentes.

use crate::write;

/// Escreve o prompt com as cores do tema atual.
pub fn write_prompt(cwd: &[u8]) {
    let mut username = [0u8; 64];
    let mut hostname = [0u8; 64];
    let mut username_length = 4usize;
    let mut hostname_length = 9usize;
    username[..4].copy_from_slice(b"root");
    hostname[..9].copy_from_slice(b"imagineos");
    let mut is_root = true;
    if let Ok(identity) = imagineos::users::identity() {
        let user = (identity.username_length as usize).min(username.len());
        let host = (identity.hostname_length as usize).min(hostname.len());
        username_length = user;
        hostname_length = host;
        username[..user].copy_from_slice(&identity.username[..user]);
        hostname[..host].copy_from_slice(&identity.hostname[..host]);
        is_root = identity.uid == 0;
    }
    let username = core::str::from_utf8(&username[..username_length]).unwrap_or("root");
    let hostname = core::str::from_utf8(&hostname[..hostname_length]).unwrap_or("imagineos");
    let directory: &[u8] = if cwd.is_empty() { b"/" } else { cwd };

    write(b"\x1b[1;32m");
    write(username.as_bytes());
    write(b"\x1b[0m@\x1b[1;36m");
    write(hostname.as_bytes());
    write(b"\x1b[0m:\x1b[1;34m");
    write(directory);
    write(if is_root { b"\x1b[0m# " } else { b"\x1b[0m$ " });
}
