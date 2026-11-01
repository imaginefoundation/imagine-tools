//! Shell POSIX do ImagineOS.
//!
//! Sintaxe suportada (subconjunto fiel, com as limitações documentadas):
//!
//! * listas: `;`, `&&`, `||`, `&` (background);
//! * pipelines: `a | b | c` (até `MAX_PIPELINE` estágios);
//! * redirecionamento: `>`, `>>`, `<`;
//! * quoting: `'literal'`, `"com $VAR"`, escapes com `\`;
//! * expansão: `$VAR`, `${VAR}`, `$?`, `$$`;
//! * builtins: `cd`, `pwd`, `echo`, `export`, `unset`, `set`, `exit`, `true`,
//!   `false`, `:`, `help`.
//!
//! Limitações conhecidas (exigem mais kernel): sem `$(...)`, sem subshell
//! `( )`, sem substituição de processo, sem glob (`*`), sem funções, sem
//! `if`/`for`/`while` e sem status por estágio de pipeline (`waitpid`).

#![no_std]
#![no_main]

mod complete;
mod env;
mod exec;
mod expand;
mod highlight;
mod history;
mod lexer;
mod line;
mod parser;
mod prompt;
mod uri;

use core::panic::PanicInfo;

use env::Environment;
use exec::{Executor, STATUS_FAILURE};
use lexer::{tokenize, LexError, Token, Word, MAX_TOKENS};
use line::{Editor, LineResult, MAX_LINE};
use parser::{parse_list, Connector, Entry, ParseError, MAX_ENTRIES};

#[unsafe(no_mangle)]
extern "C" fn _start(
    argc: usize,
    argv: *const *const u8,
    _envc: usize,
    _envp: *const *const u8,
) -> ! {
    let mut environment = Environment::new();
    initialise_from_identity(&mut environment);

    let mut cwd = [0u8; uri::MAX_PATH];
    cwd[0] = b'/';
    let mut cwd_length = 1usize;

    // `shell -c "cmd"` executa um comando e sai; `shell script` executa as
    // linhas de um arquivo. Sem argumentos, entra no modo interativo.
    //
    // `argv[0]` é o nome do próprio programa e não conta como argumento; usar
    // `[script]` contra a fatia completa nunca casaria porque ela sempre tem
    // pelo menos um elemento.
    let arguments = read_arguments(argc, argv);
    let operands = &arguments[1..operand_count(argc)];
    match operands {
        [flag, command] if *flag == "-c" => {
            let status = run_line(
                command.as_bytes(),
                &mut environment,
                &mut cwd,
                &mut cwd_length,
                &history::History::new(),
            );
            imagineos::process::exit(if status == EXIT_REQUESTED { 0 } else { status });
        }
        [script] => {
            let status = run_script(
                script,
                &mut environment,
                &mut cwd,
                &mut cwd_length,
                &history::History::new(),
            );
            imagineos::process::exit(status);
        }
        _ => {}
    }

    interactive(&mut environment, &mut cwd, &mut cwd_length)
}

/// Laço interativo: lê uma linha com edição/histórico e a executa.
fn interactive(
    environment: &mut Environment,
    cwd: &mut [u8; uri::MAX_PATH],
    cwd_length: &mut usize,
) -> ! {
    let mut editor = Editor::new();
    let mut line = [0u8; MAX_LINE];
    let mut length = 0usize;

    prompt::write_prompt(&cwd[..*cwd_length]);
    loop {
        match editor.read_line(&mut line, &cwd[..*cwd_length], &mut length) {
            LineResult::Line => {}
            // `Ctrl-C` cancela a linha e mostra um prompt novo, sem executar.
            LineResult::Interrupted => {
                prompt::write_prompt(&cwd[..*cwd_length]);
                continue;
            }
            LineResult::Exit => imagineos::process::exit(1),
        }
        let status = run_line(
            &line[..length],
            environment,
            cwd,
            cwd_length,
            editor.history(),
        );
        line[..length].fill(0);
        length = 0;
        if status == EXIT_REQUESTED {
            imagineos::process::exit(0);
        }
        prompt::write_prompt(&cwd[..*cwd_length]);
    }
}

/// Sentinela devolvida quando `exit` foi executado.
const EXIT_REQUESTED: i32 = i32::MIN;

/// Quantos dos argumentos capturados são válidos (limitado pelo buffer).
fn operand_count(argc: usize) -> usize {
    argc.min(MAX_ARGUMENTS)
}

/// Argumentos máximos lidos: `argv[0]`, mais `-c` e o comando.
const MAX_ARGUMENTS: usize = 3;

/// Coleta os argumentos do processo em `&str`.
fn read_arguments(argc: usize, argv: *const *const u8) -> [&'static str; MAX_ARGUMENTS] {
    let mut arguments = [""; MAX_ARGUMENTS];
    if argv.is_null() {
        return arguments;
    }
    for index in 0..argc.min(MAX_ARGUMENTS) {
        let pointer = unsafe { *argv.add(index) };
        if pointer.is_null() {
            continue;
        }
        // Os argumentos vêm do kernel como strings NUL-terminadas na pilha.
        let mut length = 0usize;
        while unsafe { *pointer.add(length) } != 0 && length < 512 {
            length += 1;
        }
        let bytes = unsafe { core::slice::from_raw_parts(pointer, length) };
        if let Ok(text) = core::str::from_utf8(bytes) {
            arguments[index] = text;
        }
    }
    arguments
}

/// Executa cada linha de um arquivo de script, numa sub-rotina separada para
/// que `exit` dentro do script encerre apenas a execução do arquivo.
fn run_script(
    path: &str,
    environment: &mut Environment,
    cwd: &mut [u8; uri::MAX_PATH],
    cwd_length: &mut usize,
    history: &history::History,
) -> i32 {
    let mut buffer = [0u8; 4096];
    let length = match imagineos::fs::read_file(path, &mut buffer) {
        Ok(length) => length,
        Err(_) => {
            write(b"shell: nao foi possivel ler o script\n");
            return 1;
        }
    };
    let mut status = 0;
    for line in buffer[..length].split(|byte| *byte == b'\n') {
        if line.iter().all(|byte| byte.is_ascii_whitespace()) {
            continue;
        }
        status = run_line(line, environment, cwd, cwd_length, history);
        if status == EXIT_REQUESTED {
            // Um `exit` num script devolve o status acumulado, não encerra o
            // shell chamador.
            return 0;
        }
    }
    status
}

/// Tokeniza, parseia e executa uma linha. Devolve o status ou `EXIT_REQUESTED`.
fn run_line(
    line: &[u8],
    environment: &mut Environment,
    cwd: &mut [u8; uri::MAX_PATH],
    cwd_length: &mut usize,
    history: &history::History,
) -> i32 {
    if line.iter().all(|byte| byte.is_ascii_whitespace()) {
        return 0;
    }

    let mut tokens = [Token::Word(Word::EMPTY); MAX_TOKENS];
    let count = match tokenize(line, &mut tokens) {
        Ok(0) => return 0,
        Ok(count) => count,
        Err(error) => {
            report_lex_error(error);
            return STATUS_FAILURE;
        }
    };

    let mut entries = [Entry {
        pipeline: parser::Pipeline::EMPTY,
        connector: Connector::End,
    }; MAX_ENTRIES];
    let entry_count = match parse_list(&tokens[..count], &mut entries) {
        Ok(count) => count,
        Err(error) => {
            report_parse_error(error);
            return STATUS_FAILURE;
        }
    };

    let mut executor = Executor::new(environment, cwd, cwd_length, history);
    let status = executor.run_list(&entries[..entry_count]);
    if executor.should_exit {
        return EXIT_REQUESTED;
    }
    status
}

fn initialise_from_identity(environment: &mut Environment) {
    let Ok(identity) = imagineos::users::identity() else {
        return;
    };
    let username = &identity.username[..identity.username_length as usize];
    let hostname = &identity.hostname[..identity.hostname_length as usize];
    let _ = environment.set(b"USER", username, true);
    let _ = environment.set(b"HOSTNAME", hostname, true);
    if identity.uid != 0 {
        let mut home = [0u8; env::VALUE_SIZE];
        home[..6].copy_from_slice(b"/home/");
        let length = 6 + username.len();
        if length <= home.len() {
            home[6..length].copy_from_slice(username);
            let _ = environment.set(b"HOME", &home[..length], true);
        }
    }
}

fn report_lex_error(error: LexError) {
    let message: &[u8] = match error {
        LexError::UnterminatedQuote => b"shell: aspas nao fechadas\n",
        LexError::TooManyTokens => b"shell: linha com tokens demais\n",
        LexError::WordTooLong => b"shell: palavra longa demais\n",
    };
    write(message);
}

fn report_parse_error(error: ParseError) {
    let message: &[u8] = match error {
        ParseError::UnexpectedOperator => b"shell: operador inesperado\n",
        ParseError::PipelineTooLong => b"shell: pipeline com estagios demais\n",
        ParseError::TooManyWords => b"shell: argumentos demais\n",
        ParseError::MissingRedirectionTarget => b"shell: redirecionamento sem destino\n",
    };
    write(message);
}

/// Escreve `bytes` no console; `pub(crate)` para os módulos do shell usarem.
pub(crate) fn write(bytes: &[u8]) {
    if imagineos::console::write_all(bytes).is_err() {
        imagineos::process::exit(1);
    }
}

/// Escreve `bytes` no console, sem encerrar o shell em caso de erro.
pub(crate) fn write_error(bytes: &[u8]) {
    let _ = imagineos::console::write_all(bytes);
}

#[panic_handler]
fn panic(_info: &PanicInfo<'_>) -> ! {
    imagineos::process::exit(127)
}
