//! Editor de linha interativo: histórico, edição e autocompletar.
//!
//! O laço de entrada do shell delegava tudo a um `[u8; 256]` e descartava em
//! silêncio o que não coubesse — uma linha longa era truncada sem aviso, e o
//! usuário via o comando ser executado pela metade. Aqui a linha vive em um
//! buffer do tamanho de `MAX_LINE`, a inserção recusa (com aviso) em vez de
//! perder bytes, e o cursor é de fato movido com as setas.
//!
//! Teclas reconhecidas (sequências ANSI enviadas pelo teclado do kernel):
//!
//! * `↑`/`↓` — navegação no histórico;
//! * `←`/`→` — movimento do cursor, com `Home`/`End` também;
//! * `Backspace`/`Delete` — apaga antes/depois do cursor;
//! * `Tab` — autocompleta o caminho em edição;
//! * `Ctrl-C` — cancela a linha; `Ctrl-L` — limpa a tela.

use crate::highlight::write_highlighted;
use crate::history::History;
use crate::{write, write_error};

/// Comprimento máximo de uma linha de comando.
///
/// O limite existe porque o buffer é fixo na pilha (o shell não tem alocador), e
/// é generoso o bastante para qualquer comando digitado à mão. O que mudou em
/// relação ao comportamento anterior é que **estourar o limite é reportado**, e
/// não mais um truncamento silencioso.
pub const MAX_LINE: usize = 1024;

/// Resultado da leitura de uma linha.
pub enum LineResult {
    /// Linha confirmada (sem o `\n`), no buffer informado.
    Line,
    /// `Ctrl-C`: o chamador deve descartar e mostrar um prompt novo.
    Interrupted,
    /// O console falhou ou o processo deve terminar.
    Exit,
}

/// Estado do editor entre linhas: histórico e o rascunho preservado.
pub struct Editor {
    history: History,
    /// Linha em edição, preservada para que `↑`/`↓` não a percam.
    draft: [u8; MAX_LINE],
    draft_length: usize,
}

impl Editor {
    pub const fn new() -> Self {
        Self {
            history: History::new(),
            draft: [0; MAX_LINE],
            draft_length: 0,
        }
    }

    /// Lê uma linha do console para `line`, devolvendo o comprimento em `length`.
    pub fn read_line(
        &mut self,
        line: &mut [u8; MAX_LINE],
        cwd: &[u8],
        length: &mut usize,
    ) -> LineResult {
        let mut cursor = 0usize;
        *length = 0;
        let mut history_position = self.history.len();

        loop {
            let character = match imagineos::console::read_char() {
                Ok(character) => character,
                Err(_) => return LineResult::Exit,
            };
            match character {
                '\r' | '\n' => {
                    write(b"\n");
                    if *length != 0 {
                        self.history.push(&line[..*length]);
                    }
                    return LineResult::Line;
                }
                // Ctrl-C: cancela a linha corrente.
                '\u{3}' => {
                    write(b"^C\n");
                    *length = 0;
                    return LineResult::Interrupted;
                }
                // Ctrl-L: limpa a tela e redesenha o prompt.
                '\u{c}' => {
                    write(b"\x1b[2J\x1b[H");
                    redraw(&line[..*length], cursor, cwd);
                    continue;
                }
                // Ctrl-U: apaga a linha inteira.
                '\u{15}' => {
                    for _ in 0..cursor {
                        write(b"\x08 \x08");
                    }
                    let tail = line[*length..].len();
                    let _ = tail;
                    line[..*length].fill(0);
                    *length = 0;
                    cursor = 0;
                    continue;
                }
                '\t' => {
                    if self.complete(line, length, &mut cursor, cwd) {
                        redraw(&line[..*length], cursor, cwd);
                    }
                    continue;
                }
                '\u{8}' | '\u{7f}' => {
                    if cursor > 0 {
                        let removed = previous_boundary(&line[..*length], cursor);
                        remove_range(line, length, removed, cursor);
                        cursor = removed;
                        redraw(&line[..*length], cursor, cwd);
                    }
                    continue;
                }
                // Escapes ANSI: setas, Home/End, Delete.
                '\u{1b}' => {
                    let escape = read_escape();
                    match escape {
                        Escape::Up => {
                            if history_position > 0 {
                                if history_position == self.history.len() {
                                    self.draft_length = *length;
                                    self.draft[..*length].copy_from_slice(&line[..*length]);
                                }
                                history_position -= 1;
                                self.load_history(history_position, line, length);
                                cursor = *length;
                                redraw(&line[..*length], cursor, cwd);
                            }
                        }
                        Escape::Down => {
                            if history_position < self.history.len() {
                                history_position += 1;
                                if history_position == self.history.len() {
                                    let draft = self.draft_length;
                                    line[..draft].copy_from_slice(&self.draft[..draft]);
                                    *length = draft;
                                } else {
                                    self.load_history(history_position, line, length);
                                }
                                cursor = *length;
                                redraw(&line[..*length], cursor, cwd);
                            }
                        }
                        Escape::Left => {
                            if cursor > 0 {
                                cursor = previous_boundary(&line[..*length], cursor);
                                write(b"\x1b[D");
                            }
                        }
                        Escape::Right => {
                            if cursor < *length {
                                let step = next_boundary(&line[..*length], cursor);
                                let steps = step - cursor;
                                cursor = step;
                                for _ in 0..steps {
                                    write(b"\x1b[C");
                                }
                            }
                        }
                        Escape::Home => {
                            cursor = 0;
                            write(b"\r");
                            crate::prompt::write_prompt(cwd);
                        }
                        Escape::End => {
                            while cursor < *length {
                                write(b"\x1b[C");
                                cursor = next_boundary(&line[..*length], cursor);
                            }
                        }
                        Escape::Delete => {
                            if cursor < *length {
                                let next = next_boundary(&line[..*length], cursor);
                                remove_range(line, length, cursor, next);
                                redraw(&line[..*length], cursor, cwd);
                            }
                        }
                        Escape::Unknown => {}
                    }
                    continue;
                }
                value if !value.is_control() && !is_keyboard_event(value) => {
                    let mut encoded = [0u8; 4];
                    let bytes = value.encode_utf8(&mut encoded).as_bytes();
                    if *length + bytes.len() > line.len() {
                        // Antes o excedente era descartado sem aviso e a linha
                        // seguia truncada; agora o usuário é informado.
                        write_error(b"\nshell: limite de caracteres da linha atingido\n");
                        redraw(&line[..*length], cursor, cwd);
                        continue;
                    }
                    insert_bytes(line, length, cursor, bytes);
                    cursor += bytes.len();
                    redraw(&line[..*length], cursor, cwd);
                }
                _ => {}
            }
            if imagineos::process::yield_now().is_err() {
                return LineResult::Exit;
            }
        }
    }

    /// Histórico da sessão, para o `↑`/`↓` e o builtin `history`.
    pub fn history(&self) -> &History {
        &self.history
    }

    fn load_history(&self, position: usize, line: &mut [u8; MAX_LINE], length: &mut usize) {
        if let Some(entry) = self.history.get(position) {
            let copied = entry.len().min(line.len());
            line[..copied].copy_from_slice(&entry[..copied]);
            *length = copied;
        }
    }

    /// Autocompleta o último componente de caminho da linha.
    ///
    /// Devolve `true` quando a linha mudou (o chamador redesenha). Um único
    /// candidato é completado direto; vários candidatos são listados e o
    /// prefixo comum é aplicado, como fazem os shells do Unix.
    fn complete(
        &self,
        line: &mut [u8; MAX_LINE],
        length: &mut usize,
        cursor: &mut usize,
        cwd: &[u8],
    ) -> bool {
        // O alvo é copiado para um buffer próprio: `completion_target` empresta
        // `line`, e as escritas abaixo precisam de `line` mutável.
        let mut prefix_storage = [0u8; MAX_LINE];
        let (start, prefix_length) = {
            let Some((start, prefix)) = completion_target(&line[..*length], *cursor) else {
                return false;
            };
            prefix_storage[..prefix.len()].copy_from_slice(prefix);
            (start, prefix.len())
        };
        let prefix = &prefix_storage[..prefix_length];
        let Some((matches, common, common_length)) = crate::complete::resolve(prefix, cwd) else {
            return false;
        };
        if matches == 0 {
            return false;
        }
        let is_directory = crate::complete::last_is_directory(prefix, cwd);
        // `resolve` devolve o prefixo **já completado**; se o alvo é um diretório,
        // acrescentamos a barra para o Tab seguinte entrar nele.
        let mut completed = common;
        let mut completed_length = common_length;
        if is_directory && completed_length < completed.len() {
            completed[completed_length] = b'/';
            completed_length += 1;
        }
        let removed = *length - start;
        let new_length = start + completed_length;
        if new_length > line.len() {
            return false;
        }
        let tail = *cursor;
        let total = *length;
        line.copy_within(tail..total, new_length);
        line[start..new_length].copy_from_slice(&completed[..completed_length]);
        *length = total - removed + completed_length;
        *cursor = new_length;
        if matches > 1 {
            write(b"\n");
            crate::complete::list(prefix, cwd);
            write(b"\n");
        }
        true
    }
}

/// Reescreve a linha corrente a partir do prompt, posicionando o cursor.
fn redraw(line: &[u8], cursor: usize, cwd: &[u8]) {
    write(b"\r\x1b[K");
    crate::prompt::write_prompt(cwd);
    write_highlighted(line);
    // Após reescrever tudo, o cursor está no fim; recua até a posição lógica.
    let mut position = line.len();
    while position > cursor {
        write(b"\x1b[D");
        position = previous_boundary(line, position);
    }
}

fn insert_bytes(line: &mut [u8; MAX_LINE], length: &mut usize, at: usize, bytes: &[u8]) {
    line.copy_within(at..*length, at + bytes.len());
    line[at..at + bytes.len()].copy_from_slice(bytes);
    *length += bytes.len();
}

fn remove_range(line: &mut [u8; MAX_LINE], length: &mut usize, start: usize, end: usize) {
    line.copy_within(end..*length, start);
    let removed = end - start;
    line[*length - removed..*length].fill(0);
    *length -= removed;
}

/// Início do caractere UTF-8 anterior a `position`.
fn previous_boundary(line: &[u8], position: usize) -> usize {
    let mut previous = position.saturating_sub(1);
    while previous > 0 && line[previous] & 0xc0 == 0x80 {
        previous -= 1;
    }
    previous
}

/// Fim do caractere UTF-8 que começa em `position`.
fn next_boundary(line: &[u8], position: usize) -> usize {
    let mut next = (position + 1).min(line.len());
    while next < line.len() && line[next] & 0xc0 == 0x80 {
        next += 1;
    }
    next
}

/// Sequência de escape ANSI reconhecida.
enum Escape {
    Up,
    Down,
    Left,
    Right,
    Home,
    End,
    Delete,
    Unknown,
}

/// Lê o restante de uma sequência de escape (`\x1b[...`).
fn read_escape() -> Escape {
    let Ok(second) = imagineos::console::read_char() else {
        return Escape::Unknown;
    };
    match second {
        // Setas em modo "application" (o que o kernel envia) e em modo normal.
        'A' | 'O' => read_arrow(),
        'B' => Escape::Down,
        'C' => Escape::Right,
        'D' => Escape::Left,
        'H' => Escape::Home,
        'F' => Escape::End,
        // CSI: `\x1b[<n>~` para Home/End/Delete.
        '[' => read_csi(),
        _ => Escape::Unknown,
    }
}

fn read_arrow() -> Escape {
    // `\x1bO` (SS3) é seguido da letra; `\x1b[A` já foi consumido.
    match imagineos::console::read_char() {
        Ok('A') => Escape::Up,
        Ok('B') => Escape::Down,
        Ok('C') => Escape::Right,
        Ok('D') => Escape::Left,
        Ok('H') => Escape::Home,
        Ok('F') => Escape::End,
        _ => Escape::Unknown,
    }
}

fn read_csi() -> Escape {
    let mut value = 0u32;
    let mut saw_digit = false;
    loop {
        match imagineos::console::read_char() {
            Ok('0'..='9') => {
                value = value.saturating_mul(10).saturating_add(1);
                saw_digit = true;
            }
            Ok('~') => {
                return match (saw_digit, value) {
                    (true, 1) | (true, 7) => Escape::Home,
                    (true, 4) | (true, 8) => Escape::End,
                    (true, 3) => Escape::Delete,
                    _ => Escape::Unknown,
                }
            }
            Ok('A') => return Escape::Up,
            Ok('B') => return Escape::Down,
            Ok('C') => return Escape::Right,
            Ok('D') => return Escape::Left,
            Ok('H') => return Escape::Home,
            Ok('F') => return Escape::End,
            _ => return Escape::Unknown,
        }
    }
}

/// Trecho da linha que o Tab deve completar: `(início, prefixo)`.
///
/// O alvo é a palavra sob o cursor, limitada por espaço/`;`/`|`/`&`.
fn completion_target(line: &[u8], cursor: usize) -> Option<(usize, &[u8])> {
    let end = cursor.min(line.len());
    let mut start = end;
    while start > 0 {
        let byte = line[start - 1];
        if byte == b' ' || byte == b'\t' || byte == b';' || byte == b'|' || byte == b'&' {
            break;
        }
        start -= 1;
    }
    if start == end {
        return None;
    }
    Some((start, &line[start..end]))
}

/// Sequências que o kernel usa para teclas especiais fora dos escapes ANSI.
#[allow(dead_code)]
fn is_keyboard_event(character: char) -> bool {
    ('\u{f0000}'..='\u{f0007}').contains(&character)
}
