#![no_std]
#![no_main]

fn user_main(
    mut args: imagineos_rt::Arguments<'_>,
    _env: imagineos_rt::Environment<'_>,
) -> imagineos_rt::imagineos::Result<()> {
    if let Some(argument) = args.next() {
        let argument = argument?;
        if argument == "-h" || argument == "--help" {
            imagineos_rt::println!(
                "Usage: shutdown\n\
                 Request an orderly system shutdown through PID 1."
            )?;
            return Ok(());
        }
        imagineos_rt::imagineos::console::write_error(b"shutdown: unexpected argument\n")?;
        return Err(imagineos_rt::imagineos::Error::INVALID_ARGUMENT);
    }

    imagineos_rt::imagineos::console::write_all(b"shutdown: requesting system shutdown\n")?;
    imagineos_rt::imagineos::shutdown::request()
}

imagineos_rt::main!(with_args user_main);
