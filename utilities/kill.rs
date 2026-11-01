#![no_std]
#![no_main]

fn user_main(
    mut args: imagineos_rt::Arguments<'_>,
    _env: imagineos_rt::Environment<'_>,
) -> imagineos_rt::imagineos::Result<()> {
    let mut first = args.next().transpose()?;
    if first == Some("-l") || first == Some("--list") {
        imagineos_rt::println!("1) SIGHUP  2) SIGINT  9) SIGKILL  11) SIGSEGV")?;
        imagineos_rt::println!("15) SIGTERM  18) SIGCONT  19) SIGSTOP")?;
        return Ok(());
    }

    let mut signal = imagineos::signals::SIGTERM;
    if let Some(option) = first {
        if option == "--help" || option == "-h" {
            imagineos_rt::println!(
                "Usage: kill [-SIGNAL | -s SIGNAL] PID...\n\
                 Send a signal to one or more process IDs. Default: SIGTERM.\n\
                 Use kill -l to list supported signals."
            )?;
            return Ok(());
        }
        if let Some(number) = option.strip_prefix("-s") {
            let name = if number.is_empty() {
                args.next().transpose()?.ok_or_else(invalid_argument)?
            } else {
                number
            };
            signal = parse_signal(name).ok_or_else(invalid_argument)?;
            first = args.next().transpose()?;
        } else if option.starts_with('-') {
            signal = parse_signal(&option[1..]).ok_or_else(invalid_argument)?;
            first = args.next().transpose()?;
        }
    }

    let mut sent = false;
    while let Some(pid_text) = first {
        let pid = parse_pid(pid_text).ok_or_else(invalid_argument)?;
        if imagineos::signals::send(pid, signal).is_err() {
            imagineos_rt::imagineos::console::write_error(b"kill: unable to signal process\n")?;
            return Err(imagineos_rt::imagineos::Error::INVALID_ARGUMENT);
        }
        sent = true;
        first = args.next().transpose()?;
    }
    if !sent {
        imagineos_rt::imagineos::console::write_error(b"kill: missing process ID\n")?;
        return Err(imagineos_rt::imagineos::Error::INVALID_ARGUMENT);
    }
    Ok(())
}

fn parse_pid(value: &str) -> Option<usize> {
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    value.parse().ok()
}

fn parse_signal(value: &str) -> Option<u64> {
    match value {
        "1" | "HUP" | "SIGHUP" => Some(imagineos::signals::SIGHUP),
        "2" | "INT" | "SIGINT" => Some(imagineos::signals::SIGINT),
        "9" | "KILL" | "SIGKILL" => Some(imagineos::signals::SIGKILL),
        "11" | "SEGV" | "SIGSEGV" => Some(imagineos::signals::SIGSEGV),
        "15" | "TERM" | "SIGTERM" => Some(imagineos::signals::SIGTERM),
        "18" | "CONT" | "SIGCONT" => Some(imagineos::signals::SIGCONT),
        "19" | "STOP" | "SIGSTOP" => Some(imagineos::signals::SIGSTOP),
        _ => None,
    }
}

fn invalid_argument() -> imagineos_rt::imagineos::Error {
    imagineos_rt::imagineos::Error::INVALID_ARGUMENT
}

imagineos_rt::main!(with_args user_main);
