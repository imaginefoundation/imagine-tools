#![no_std]
#![no_main]

const SYSTEM: &str = "ImagineOS";                   // Distribuição
const CODENAME: &str = "astrid";                        // Codinome
const RELEASE: &str = "0.2.1";                      // Versão
const MACHINE: &str = "x86_64";                     // Arquitetura
const OPERATING_SYSTEM: &str = "DNU/ImagineOS";     // Sistema Operacional

/// Prefixo do campo `-v`; a versão da ABI vem do kernel em tempo de execução.
const VERSION_PREFIX: &str = "Dreamcore (DNU) ABI ";

/// `-v`: consulta `SYS_ABI_VERSION` em vez de repetir um número embutido.
///
/// Um valor fixo aqui mentiria sempre que o kernel mudasse a ABI — foi o que
/// aconteceu quando a ABI chegou à versão 4 e o `uname` continuava anunciando 2.
/// Se a syscall falhar, escreve o prefixo sem número em vez de abortar.
fn write_version() -> imagineos_rt::imagineos::Result<()> {
    imagineos_rt::print!("{VERSION_PREFIX}")?;
    match imagineos_rt::imagineos::syscall::abi_version() {
        Ok(version) => imagineos_rt::print!("{version}"),
        Err(_) => Ok(()),
    }
}

#[derive(Default)]
struct Fields {
    system: bool,
    node: bool,
    release: bool,
    version: bool,
    machine: bool,
    operating_system: bool,
}

fn user_main(
    mut args: imagineos_rt::Arguments<'_>,
    _env: imagineos_rt::Environment<'_>,
) -> imagineos_rt::imagineos::Result<()> {
    let mut fields = Fields::default();
    let mut selected = false;
    while let Some(argument) = args.next() {
        let argument = argument?;
        if argument == "--help" {
            imagineos_rt::println!(
                "Usage: uname [OPTION]...\n\
                 -a, --all                 show all system information\n\
                 -s, --kernel-name         show the system name\n\
                 -n, --nodename            show the network node hostname\n\
                 -r, --kernel-release      show the kernel release\n\
                 -v, --kernel-version      show the kernel version\n\
                 -m, --machine             show the machine architecture\n\
                 -o, --operating-system    show the operating system"
            )?;
            return Ok(());
        }
        if argument == "--all" || argument == "-a" {
            fields = Fields {
                system: true,
                node: true,
                release: true,
                version: true,
                machine: true,
                operating_system: true,
            };
            selected = true;
            continue;
        }
        if argument.starts_with("--") {
            match argument {
                "--kernel-name" => fields.system = true,
                "--nodename" => fields.node = true,
                "--kernel-release" => fields.release = true,
                "--kernel-version" => fields.version = true,
                "--machine" => fields.machine = true,
                "--operating-system" => fields.operating_system = true,
                _ => return invalid_option(),
            }
            selected = true;
            continue;
        }
        let Some(short_options) = argument.strip_prefix('-') else {
            return invalid_option();
        };
        if short_options.is_empty() {
            return invalid_option();
        }
        for option in short_options.bytes() {
            match option {
                b'a' => {
                    fields = Fields {
                        system: true,
                        node: true,
                        release: true,
                        version: true,
                        machine: true,
                        operating_system: true,
                    };
                }
                b's' => fields.system = true,
                b'n' => fields.node = true,
                b'r' => fields.release = true,
                b'v' => fields.version = true,
                b'm' => fields.machine = true,
                b'o' => fields.operating_system = true,
                _ => return invalid_option(),
            }
            selected = true;
        }
    }

    if !selected {
        fields.system = true;
    }
    let mut separator = false;
    write_field(fields.system, SYSTEM, &mut separator)?;
    write_field(fields.node, CODENAME, &mut separator)?;
    write_field(fields.release, RELEASE, &mut separator)?;
    if fields.version {
        if separator {
            imagineos_rt::print!(" ")?;
        }
        write_version()?;
        separator = true;
    }
    write_field(fields.machine, MACHINE, &mut separator)?;
    write_field(fields.operating_system, OPERATING_SYSTEM, &mut separator)?;
    imagineos_rt::println!()?;
    Ok(())
}

fn write_field(
    enabled: bool,
    value: &str,
    separator: &mut bool,
) -> imagineos_rt::imagineos::Result<()> {
    if enabled {
        if *separator {
            imagineos_rt::print!(" ")?;
        }
        imagineos_rt::print!("{value}")?;
        *separator = true;
    }
    Ok(())
}

fn invalid_option<T>() -> imagineos_rt::imagineos::Result<T> {
    imagineos_rt::imagineos::console::write_error(b"uname: invalid option (try --help)\n")?;
    Err(imagineos_rt::imagineos::Error::INVALID_ARGUMENT)
}

imagineos_rt::main!(with_args user_main);
