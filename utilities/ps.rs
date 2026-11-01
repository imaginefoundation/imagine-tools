#![no_std]
#![no_main]

fn user_main(
    mut args: imagineos_rt::Arguments<'_>,
    _env: imagineos_rt::Environment<'_>,
) -> imagineos_rt::imagineos::Result<()> {
    if let Some(argument) = args.next() {
        let argument = argument?;
        if argument == "-h" || argument == "--help" {
            imagineos_rt::println!("Usage: ps\nList the current process snapshot.")?;
            return Ok(());
        }
        imagineos_rt::imagineos::console::write_error(b"ps: unexpected argument\n")?;
        return Err(imagineos_rt::imagineos::Error::INVALID_ARGUMENT);
    }

    let mut processes = [imagineos::abi::ProcessInfo::default(); imagineos::abi::MAX_PROCESSES];
    let count = imagineos::process::list(&mut processes)?;
    imagineos_rt::println!("{:<6} {:<8} NAME", "PID", "STATE")?;
    for process in processes.iter().take(count) {
        let length = process
            .name
            .iter()
            .position(|byte| *byte == 0)
            .unwrap_or(process.name.len());
        let name = core::str::from_utf8(&process.name[..length])
            .map_err(|_| imagineos_rt::imagineos::Error::INVALID_ARGUMENT)?;
        let state = match process.state {
            imagineos::abi::PROCESS_RUNNING => "running",
            imagineos::abi::PROCESS_STOPPED => "stopped",
            _ => "unknown",
        };
        imagineos_rt::println!("{:<6} {:<8} {}", process.pid, state, name)?;
    }
    Ok(())
}

imagineos_rt::main!(with_args user_main);
