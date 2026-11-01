//! `systemfetch` — informações do sistema, com a logo do ImagineOS.
//!
//! Lê tudo de `SYS_SYSTEM_INFO` (uma chamada só, para os números serem coerentes
//! entre si) e imprime a logo ao lado, com as cores da marca. `--no-logo` deixa
//! a saída em texto puro, útil para redirecionar para um arquivo.

#![no_std]
#![no_main]

use imagineos::abi::SystemInfo;
use imagineos::fmt;

/// Sequências de controle (ver a nota em `top.rs`: não podem aparecer dentro
/// das strings de formato).
const BOLD_GREEN: &str = "\x1b[1;32m";
const RESET: &str = "\x1b[0m";
const BOLD_CYAN: &str = "\x1b[1;36m";

/// Logo do ImagineOS em blocos (arte ASCII).
const LOGO: [&str; 18] = [
    "        @@@@@@@@         ",
    "       @@      @@        ",
    "      @          @       ",
    "      @          @       ",
    "@@@    @@      @@    @@@ ",
    "  @@@    @@@@@@    @@@   ",
    "      @@@      @@@       ",
    " @@@     @@@@@@     @@@  ",
    "    @@@          @@@     ",
    "        @@    @@         ",
    "         @@  @@          ",
    "         @@  @@          ",
    "         @@  @@          ",
    "         @@  @@          ",
    "         @@  @@          ",
    "         @@  @@          ",
    "         @@  @@          ",
    "         @@  @@          ",
];

fn user_main(
    mut args: imagineos_rt::Arguments<'_>,
    _env: imagineos_rt::Environment<'_>,
) -> imagineos_rt::imagineos::Result<()> {
    let mut show_logo = true;
    while let Some(argument) = args.next() {
        let argument = argument?;
        match argument {
            "--help" => {
                imagineos_rt::println!(
                    "Usage: systemfetch [OPTION]\n\
                     \n\
                     Show system information: uptime, shell, resolution, CPU and RAM.\n\
                     \n\
                     Options:\n\
                       --no-logo    omit the ImagineOS logo\n\
                       --help       show this help"
                )?;
                return Ok(());
            }
            "--no-logo" => show_logo = false,
            _ => {
                imagineos_rt::imagineos::console::write_error(
                    b"systemfetch: opcao desconhecida (tente --help)\n",
                )?;
                return Err(imagineos_rt::imagineos::Error::INVALID_ARGUMENT);
            }
        }
    }

    let info = imagineos::system::info()?;
    let mut fields = Fields::collect(&info)?;
    if show_logo {
        print_with_logo(&mut fields)?;
    } else {
        print_plain(&mut fields)?;
    }
    Ok(())
}

/// Campos já formatados, prontos para alinhar com a logo.
struct Fields {
    lines: [Line; 7],
    count: usize,
}

struct Line {
    label: [u8; 16],
    label_length: usize,
    value: [u8; 64],
    value_length: usize,
}

impl Fields {
    fn collect(info: &SystemInfo) -> imagineos_rt::imagineos::Result<Self> {
        let mut fields = Self {
            lines: core::array::from_fn(|_| Line {
                label: [0; 16],
                label_length: 0,
                value: [0; 64],
                value_length: 0,
            }),
            count: 0,
        };
        fields.push(b"OS", b"ImagineOS (DNU)")?;
        fields.push(b"Prompt", &prompt_text())?;
        fields.push(b"Uptime", &format_uptime(info.uptime_microseconds))?;
        fields.push(b"Shell", b"/bin/shell")?;
        fields.push(b"Resolution", &format_resolution(info))?;
        fields.push(b"CPU", &format_cpu(info))?;
        fields.push(b"Memory", &format_memory(info))?;
        Ok(fields)
    }

    fn push(&mut self, label: &[u8], value: &[u8]) -> imagineos_rt::imagineos::Result<()> {
        if self.count == self.lines.len() {
            return Ok(());
        }
        let line = &mut self.lines[self.count];
        let label_length = label.len().min(line.label.len());
        line.label[..label_length].copy_from_slice(&label[..label_length]);
        line.label_length = label_length;
        let value_length = value.len().min(line.value.len());
        line.value[..value_length].copy_from_slice(&value[..value_length]);
        line.value_length = value_length;
        self.count += 1;
        Ok(())
    }
}

/// Imprime a logo à esquerda e os campos à direita.
fn print_with_logo(fields: &mut Fields) -> imagineos_rt::imagineos::Result<()> {
    for (index, art) in LOGO.iter().enumerate() {
        // As linhas centrais da arte ganham destaque em ciano; as bordas ficam
        // sem cor para a silhueta ficar limpa.
        let highlighted = index >= 3 && index <= 5;
        if highlighted {
            imagineos_rt::print!("{BOLD_CYAN}{art}{RESET}")?;
        } else {
            imagineos_rt::print!("{art}")?;
        }
        if let Some(line) = fields.lines.get(index) {
            if line.label_length != 0 {
                print_field(line)?;
            }
        }
        imagineos_rt::println!()?;
    }
    // Campos que sobram quando a logo tem menos linhas que os campos.
    for line in fields.lines.iter().take(fields.count).skip(LOGO.len()) {
        print_field(line)?;
        imagineos_rt::println!()?;
    }
    Ok(())
}

fn print_field(line: &Line) -> imagineos_rt::imagineos::Result<()> {
    // Rótulo em verde-negrito, alinhado em 9 colunas, valor sem cor.
    imagineos_rt::print!(
        "{BOLD_GREEN}{:<11}{RESET}{}",
        core::str::from_utf8(&line.label[..line.label_length]).unwrap_or(""),
        core::str::from_utf8(&line.value[..line.value_length]).unwrap_or("")
    )
}

fn print_plain(fields: &mut Fields) -> imagineos_rt::imagineos::Result<()> {
    for line in fields.lines.iter().take(fields.count) {
        imagineos_rt::println!(
            "{}: {}",
            core::str::from_utf8(&line.label[..line.label_length]).unwrap_or(""),
            core::str::from_utf8(&line.value[..line.value_length]).unwrap_or("")
        )?;
    }
    Ok(())
}

/// Prompt no formato `user@host:cwd$`, igual ao do shell.
fn prompt_text() -> [u8; 64] {
    let mut buffer = [0u8; 64];
    let mut length = 0usize;
    let mut username = [0u8; 32];
    let mut hostname = [0u8; 32];
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
    for part in [
        &username[..username_length],
        b"@".as_slice(),
        &hostname[..hostname_length],
        b":~".as_slice(),
        if is_root { b"# ".as_slice() } else { b"$ ".as_slice() },
    ] {
        let available = buffer.len().saturating_sub(length);
        let copied = part.len().min(available);
        buffer[length..length + copied].copy_from_slice(&part[..copied]);
        length += copied;
    }
    buffer
}

/// `1d 2h 3m 4.567s`, com as unidades que não são zero.
fn format_uptime(microseconds: u64) -> [u8; 64] {
    let mut buffer = [0u8; 64];
    let total_seconds = microseconds / 1_000_000;
    let days = total_seconds / 86_400;
    let hours = total_seconds % 86_400 / 3_600;
    let minutes = total_seconds % 3_600 / 60;
    let seconds = total_seconds % 60;
    let millis = microseconds % 1_000_000 / 1000;
    let mut cursor = 0usize;
    if days != 0 {
        cursor += write_part(&mut buffer[cursor..], days, b"d ");
    }
    if days != 0 || hours != 0 {
        cursor += write_part(&mut buffer[cursor..], hours, b"h ");
    }
    if days != 0 || hours != 0 || minutes != 0 {
        cursor += write_part(&mut buffer[cursor..], minutes, b"m ");
    }
    // Sempre mostra os segundos com milissegundos: é o que dá a sensação de
    // "relógio vivo" e confirma que o TSC está contando.
    cursor += write_part(&mut buffer[cursor..], seconds, b".");
    cursor += write_fixed3(&mut buffer[cursor..], millis);
    let _ = copy_bytes(&mut buffer[cursor..], b"s");
    buffer
}

fn write_part(buffer: &mut [u8], value: u64, suffix: &[u8]) -> usize {
    let mut digits = [0u8; 20];
    let text = fmt::decimal(&mut digits, value);
    let mut cursor = copy_bytes(buffer, text);
    cursor += copy_bytes(&mut buffer[cursor..], suffix);
    cursor
}

/// Três dígitos com zeros à esquerda (milissegundos).
fn write_fixed3(buffer: &mut [u8], value: u64) -> usize {
    let hundreds = value / 100;
    let tens = value % 100 / 10;
    let units = value % 10;
    if buffer.len() < 3 {
        return 0;
    }
    buffer[0] = b'0' + hundreds as u8;
    buffer[1] = b'0' + tens as u8;
    buffer[2] = b'0' + units as u8;
    3
}

fn copy_bytes(buffer: &mut [u8], bytes: &[u8]) -> usize {
    let length = bytes.len().min(buffer.len());
    buffer[..length].copy_from_slice(&bytes[..length]);
    length
}

fn format_resolution(info: &SystemInfo) -> [u8; 64] {
    let mut buffer = [0u8; 64];
    if info.framebuffer_width == 0 || info.framebuffer_height == 0 {
        return copy_into(buffer, b"text console");
    }
    let mut digits = [0u8; 20];
    let mut cursor = 0usize;
    let width = fmt::decimal(&mut digits, info.framebuffer_width as u64);
    cursor += copy_bytes(&mut buffer[cursor..], width);
    cursor += copy_bytes(&mut buffer[cursor..], b"x");
    let height = fmt::decimal(&mut digits, info.framebuffer_height as u64);
    let _ = copy_bytes(&mut buffer[cursor..], height);
    buffer
}

fn format_cpu(info: &SystemInfo) -> [u8; 64] {
    let mut buffer = [0u8; 64];
    let mut cursor = copy_bytes(&mut buffer, b"x86_64 @ ");
    if info.tsc_frequency == 0 {
        let _ = copy_bytes(&mut buffer[cursor..], b"frequencia desconhecida");
        return buffer;
    }
    // A frequência do TSC vem em Hz; converte para MHz inteiros.
    let mhz = info.tsc_frequency / 1_000_000;
    cursor += copy_bytes(&mut buffer[cursor..], fmt::decimal(&mut [0u8; 20], mhz));
    let _ = copy_bytes(&mut buffer[cursor..], b" MHz (TSC)");
    buffer
}

fn format_memory(info: &SystemInfo) -> [u8; 64] {
    let mut buffer = [0u8; 64];
    let used = info.memory_total.saturating_sub(info.memory_free);
    let mut cursor = 0usize;
    cursor += copy_bytes(&mut buffer[cursor..], fmt::decimal(&mut [0u8; 20], used / 1024 / 1024));
    cursor += copy_bytes(&mut buffer[cursor..], b" MiB / ");
    cursor += copy_bytes(
        &mut buffer[cursor..],
        fmt::decimal(&mut [0u8; 20], info.memory_total / 1024 / 1024),
    );
    let _ = copy_bytes(&mut buffer[cursor..], b" MiB");
    buffer
}

fn copy_into(mut buffer: [u8; 64], bytes: &[u8]) -> [u8; 64] {
    let length = bytes.len().min(buffer.len());
    buffer[..length].copy_from_slice(&bytes[..length]);
    buffer
}

imagineos_rt::main!(with_args user_main);
