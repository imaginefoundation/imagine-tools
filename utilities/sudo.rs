#![no_std]
#![no_main]

use imagineos_rt::{Arguments, Environment};

fn user_main(mut arguments: Arguments<'_>, _environment: Environment<'_>) -> i32 {
    let mut command_arguments = [""; imagineos::abi::MAX_EXEC_ARGS];
    let mut count = 0;
    for argument in &mut arguments {
        let argument = match argument {
            Ok(argument) => argument,
            Err(error) => return error.code(),
        };
        if count == command_arguments.len() {
            return imagineos::Error::TOO_BIG.code();
        }
        command_arguments[count] = argument;
        count += 1;
    }
    if count == 0 {
        let _ = imagineos::console::write_error(b"sudo: uso: sudo comando [argumentos...]\n");
        return 2;
    }
    let identity = match imagineos::users::identity() {
        Ok(identity) => identity,
        Err(error) => return error.code(),
    };
    let username =
        match core::str::from_utf8(&identity.username[..identity.username_length as usize]) {
            Ok(username) => username,
            Err(_) => return imagineos::Error::INVALID_ARGUMENT.code(),
        };
    if imagineos::console::write_all(b"[sudo] password: ").is_err() {
        return 1;
    }
    let mut password = [0u8; imagineos::abi::ACCOUNT_PASSWORD_SIZE];
    let length = match read_password(&mut password) {
        Ok(length) => length,
        Err(error) => return error.code(),
    };
    let result = imagineos::users::authenticate(username, &password[..length], 0);
    password.fill(0);
    if result.is_err() {
        let _ =
            imagineos::console::write_error(b"sudo: usuario sem permissao ou senha incorreta\n");
        return 1;
    }
    let command = command_arguments[0];
    let mut path = [0u8; imagineos::abi::MAX_EXEC_ITEM_SIZE];
    let path_length = if command.starts_with('/') || command.contains('/') {
        let bytes = command.as_bytes();
        if bytes.len() > path.len() {
            return imagineos::Error::TOO_BIG.code();
        }
        path[..bytes.len()].copy_from_slice(bytes);
        bytes.len()
    } else {
        let prefix = b"/bin/";
        let bytes = command.as_bytes();
        if prefix.len() + bytes.len() > path.len() {
            return imagineos::Error::TOO_BIG.code();
        }
        path[..prefix.len()].copy_from_slice(prefix);
        path[prefix.len()..prefix.len() + bytes.len()].copy_from_slice(bytes);
        prefix.len() + bytes.len()
    };
    let Ok(path) = core::str::from_utf8(&path[..path_length]) else {
        return imagineos::Error::INVALID_ARGUMENT.code();
    };
    match imagineos::process::exec(path, &command_arguments[..count], &[]) {
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
