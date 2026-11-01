//! `top` — atividade dos processos.
//!
//! A cada atualização o utilitário lê `SYS_PROCESS_LIST` e `SYS_SYSTEM_INFO` e
//! redesenha a tabela no lugar, usando sequências ANSI (`CSI H`/`CSI K`) em vez
//! de rolar o console. O kernel implementa esses comandos no console de texto.
//!
//! O kernel não mantém tempo de CPU acumulado por processo (o escalonador é
//! round-robin por prioridade), então a coluna `PRIO` mostra o **peso** com que
//! cada processo é escalonado — o quantum da sua prioridade —, que é o que
//! determina quanto tempo ele recebe.

#![no_std]
#![no_main]

use imagineos::abi::{ProcessInfo, PROCESS_NAME_SIZE};
use imagineos::fmt;

/// Sequências de controle, em constantes separadas.
///
/// Não podem ser escritas dentro das strings de formato: o `{` de a sequencia de escape é
/// lido pelo `print!` como início de um argumento.
const RESET: &str = "\x1b[0m";
const BOLD_CYAN: &str = "\x1b[1;36m";
const BOLD_WHITE: &str = "\x1b[1;37m";
const DIM: &str = "\x1b[0;90m";
/// Apaga do cursor até o fim da linha.
const ERASE_LINE: &str = "\x1b[K";
/// Limpa a tela inteira.
const CLEAR: &str = "\x1b[2J";
/// Cursor para o canto superior esquerdo.
const HOME: &str = "\x1b[H";

/// Quantas linhas cabem sem rolar a tela.
const MAX_ROWS: usize = 16;
/// Cedências de CPU entre quadros (o console não tem `sleep`).
const YIELDS_PER_REFRESH: u32 = 20_000;

fn user_main(
    mut args: imagineos_rt::Arguments<'_>,
    _env: imagineos_rt::Environment<'_>,
) -> imagineos_rt::imagineos::Result<()> {
    let mut once = false;
    while let Some(argument) = args.next() {
        let argument = argument?;
        match argument {
            "--help" => {
                imagineos_rt::println!(
                    "Usage: top [OPTION]\n\
                     \n\
                     Show process activity. Press Ctrl-C to quit.\n\
                     \n\
                     Options:\n\
                       -1, --once    print one snapshot and exit\n\
                       --help        show this help"
                )?;
                return Ok(());
            }
            "-1" | "--once" => once = true,
            _ => {
                imagineos_rt::imagineos::console::write_error(
                    b"top: opcao desconhecida (tente --help)\n",
                )?;
                return Err(imagineos_rt::imagineos::Error::INVALID_ARGUMENT);
            }
        }
    }

    let mut first = true;
    loop {
        let mut processes = [ProcessInfo::default(); imagineos::abi::MAX_PROCESSES];
        let count = imagineos::process::list(&mut processes)?;
        let info = imagineos::system::info()?;

        // Limpa a tela no primeiro quadro; depois só reposiciona o cursor, para
        // a tabela ser reescrita no lugar em vez de rolar o console.
        imagineos_rt::print!("{}", if first { CLEAR } else { HOME })?;
        first = false;
        draw(&processes[..count], &info)?;
        if once {
            imagineos_rt::println!()?;
            return Ok(());
        }
        for _ in 0..YIELDS_PER_REFRESH {
            imagineos::process::yield_now()?;
        }
    }
}

fn draw(
    processes: &[ProcessInfo],
    info: &imagineos::abi::SystemInfo,
) -> imagineos_rt::imagineos::Result<()> {
    let used = info.memory_total.saturating_sub(info.memory_free);
    imagineos_rt::print!("{}top{} - uptime ", BOLD_CYAN, RESET)?;
    write_uptime(info.uptime_microseconds)?;
    imagineos_rt::println!(
        "   {} processos   RAM {} MiB / {} MiB{}",
        info.process_count,
        used / 1024 / 1024,
        info.memory_total / 1024 / 1024,
        ERASE_LINE
    )?;

    // Cabeçalho da tabela: 8 colunas com cor própria e sem linha de apagar.
    imagineos_rt::print!("{}{:>5}  {:<9} {:>5} {:>6} {:>8}  {}{}\n", BOLD_WHITE, "PID", "ESTADO", "PRIO", "FATIA", "CPU", "COMANDO", RESET)?;

    for process in processes.iter().take(MAX_ROWS) {
        let marker = if process.running != 0 { ">" } else { " " };
        let share = share(process.quantum_ticks, processes);
        imagineos_rt::println!(
            "{} {:>5}  {:<9} {:>5} {:>6} {:>7}%  {}{}",
            marker,
            process.pid,
            state_name(process.state),
            process.priority,
            process.quantum_ticks,
            share,
            text(&process.name),
            ERASE_LINE
        )?;
    }
    if processes.len() > MAX_ROWS {
        imagineos_rt::println!(
            "{}  ... {} processos a mais{}",
            DIM,
            processes.len() - MAX_ROWS,
            RESET
        )?;
    }
    imagineos_rt::print!("{}Ctrl-C para sair{}{}", DIM, RESET, ERASE_LINE)?;
    Ok(())
}

/// Fatia de CPU estimada do processo, em porcentagem inteira.
///
/// O escalonador dá a cada processo `quantum_ticks` ticks por vez, então a
/// proporção do tempo total é `quantum / soma_dos_quantums`. É uma estimativa do
/// peso, não uma medição do passado — o kernel não acumula tempo por processo.
fn share(quantum: u32, processes: &[ProcessInfo]) -> u32 {
    let total: u32 = processes
        .iter()
        .filter(|process| process.state == imagineos::abi::PROCESS_RUNNING)
        .map(|process| process.quantum_ticks)
        .sum();
    if total == 0 {
        return 0;
    }
    (quantum as u64 * 100 / total as u64) as u32
}

fn state_name(state: u32) -> &'static str {
    match state {
        imagineos::abi::PROCESS_RUNNING => "running",
        imagineos::abi::PROCESS_STOPPED => "stopped",
        imagineos::abi::PROCESS_WAITING => "waiting",
        imagineos::abi::PROCESS_ZOMBIE => "zombie",
        _ => "unknown",
    }
}

fn write_uptime(microseconds: u64) -> imagineos_rt::imagineos::Result<()> {
    let total_seconds = microseconds / 1_000_000;
    imagineos_rt::print!(
        "{:02}:{:02}:{:02}",
        total_seconds / 3_600,
        total_seconds % 3_600 / 60,
        total_seconds % 60
    )
}

/// Nome do processo como texto, cortando no NUL.
fn text(name: &[u8; PROCESS_NAME_SIZE]) -> &str {
    fmt::field_text(name)
}

imagineos_rt::main!(with_args user_main);
