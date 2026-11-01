#![no_std]
#![no_main]

use imagineos_rt::{Arguments, Environment};

fn user_main(mut arguments: Arguments<'_>, _environment: Environment<'_>) -> i32 {
    let username = match arguments.next() {
        None => "root",
        Some(Ok(username)) => username,
        Some(Err(error)) => return error.code(),
    };
    if arguments.next().is_some() {
        let _ = imagineos::console::write_error(b"su: uso: su [usuario]\n");
        return 2;
    }
    if imagineos::console::write_all(b"Password: ").is_err() {
        return 1;
    }
    let mut password = [0u8; imagineos::abi::ACCOUNT_PASSWORD_SIZE];
    let length = match read_password(&mut password) {
        Ok(length) => length,
        Err(error) => return error.code(),
    };
    let result = imagineos::users::authenticate(username, &password[..length], u32::MAX);
    password.fill(0);
    if result.is_err() {
        let _ = imagineos::console::write_error(b"su: autenticacao falhou\n");
        return 1;
    }
    match imagineos::process::exec("/bin/shell", &["/bin/shell"], &[]) {
        Ok(_) => 0,
        Err(error) => error.code(),
    }
}

fn read_password(output: &mut [u8]) -> imagineos::Result<usize> {
    let mut length = 0;
    loop {
        let character = imagineos::console::read_char()?;
        match character {
            '\r' | '\n' => {
                imagineos::console::write_all(b"\n")?;
                return Ok(length);
            }
            '\u{8}' | '\u{7f}' if length > 0 => {
                length -= 1;
                imagineos::console::write_all(b"\x08 \x08")?;
            }
            value if value.is_ascii() && !value.is_ascii_control() && length < output.len() => {
                output[length] = value as u8;
                length += 1;
                imagineos::console::write_all(b"*")?;
            }
            _ => {}
        }
    }
}

imagineos_rt::main!(with_args user_main);
