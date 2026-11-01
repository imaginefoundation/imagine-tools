//! Autocompletar de caminhos para o `Tab`.
//!
//! Sem alocador, a estratégia é varrer o diretório duas vezes: a primeira conta
//! e calcula o prefixo comum dos candidatos, a segunda imprime os nomes. O
//! prefixo comum é o que se aplica à linha — completar para um candidato
//! arbitrário quando há vários seria adivinhação.

use crate::uri;
use crate::{write, write_error};

/// Diretório com entradas demais para listar de uma vez.
const MAX_LISTED: usize = 64;

/// Resolve `prefix` para `(quantidade de candidatos, prefixo completado, tamanho)`.
///
/// `prefix` é o texto já digitado (pode conter `/`); `cwd` o diretório atual
/// para caminhos relativos. O prefixo comum devolvido é o **resultado** já com o
/// que foi digitado preservado, não só o nome do arquivo.
pub fn resolve(prefix: &[u8], cwd: &[u8]) -> Option<(usize, [u8; uri::MAX_PATH], usize)> {
    let (directory, name_prefix) = split_prefix(prefix, cwd)?;
    let directory = core::str::from_utf8(&directory[..directory_length(&directory)]).ok()?;
    let mut entries = [0u8; 4096];
    let length = imagineos::fs::list_directory_with_hidden(directory, &mut entries, false).ok()?;

    let mut matches = 0usize;
    let mut common = [0u8; uri::MAX_PATH];
    let mut common_length = 0usize;
    for entry in split_entries(&entries[..length]) {
        if !entry.starts_with(name_prefix) {
            continue;
        }
        if matches == 0 {
            common[..entry.len()].copy_from_slice(entry);
            common_length = entry.len();
        } else {
            common_length = shared_prefix(&common[..common_length], entry);
        }
        matches += 1;
    }
    if matches == 0 {
        return None;
    }

    // O resultado é o prefixo digitado com o nome comum completado no lugar do
    // último componente.
    let mut completed = [0u8; uri::MAX_PATH];
    let base_length = prefix.len() - name_prefix.len();
    completed[..base_length].copy_from_slice(&prefix[..base_length]);
    let total = base_length + common_length;
    if total > completed.len() {
        return None;
    }
    completed[base_length..total].copy_from_slice(&common[..common_length]);
    Some((matches, completed, total))
}

/// Verdadeiro quando o último componente do prefixo é um diretório.
pub fn last_is_directory(prefix: &[u8], cwd: &[u8]) -> bool {
    let Some((directory, name)) = split_prefix(prefix, cwd) else {
        return false;
    };
    if name.is_empty() {
        return true;
    }
    let mut path = [0u8; uri::MAX_PATH];
    let base = directory_length(&directory);
    let length = base + 1 + name.len();
    if length > path.len() {
        return false;
    }
    path[..base].copy_from_slice(&directory[..base]);
    path[base] = b'/';
    path[base + 1..length].copy_from_slice(name);
    match core::str::from_utf8(&path[..length]) {
        Ok(path) => imagineos::fs::is_dir(path).unwrap_or(false),
        Err(_) => false,
    }
}

/// Comprimento útil de um buffer de caminho com zeros à direita.
fn directory_length(directory: &[u8; uri::MAX_PATH]) -> usize {
    directory
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(directory.len())
}

/// Lista os candidatos de `prefix`, uma linha por nome.
pub fn list(prefix: &[u8], cwd: &[u8]) {
    let Some((directory, name_prefix)) = split_prefix(prefix, cwd) else {
        return;
    };
    let base = directory_length(&directory);
    let Ok(directory_text) = core::str::from_utf8(&directory[..base]) else {
        return;
    };
    let mut entries = [0u8; 4096];
    let Ok(length) = imagineos::fs::list_directory_with_hidden(directory_text, &mut entries, false)
    else {
        return;
    };
    let mut shown = 0usize;
    for entry in split_entries(&entries[..length]) {
        if !entry.starts_with(name_prefix) {
            continue;
        }
        if shown == MAX_LISTED {
            write_error(b"shell: ha mais candidatos do que os listados\n");
            break;
        }
        write(entry);
        if is_directory_entry(&directory[..base], entry) {
            write(b"/");
        }
        write(b"  ");
        shown += 1;
    }
}

/// Separa o prefixo digitado em `(diretório, nome parcial)`.
fn split_prefix<'a>(prefix: &'a [u8], cwd: &'a [u8]) -> Option<([u8; uri::MAX_PATH], &'a [u8])> {
    let text = core::str::from_utf8(prefix).ok()?;
    let (base, name) = match text.rfind('/') {
        Some(index) => (&text[..=index], &text[index + 1..]),
        None => ("", text),
    };
    let mut directory = [0u8; uri::MAX_PATH];
    if base.is_empty() {
        // Sem barra: o diretório é o atual.
        let length = cwd.len().min(directory.len());
        directory[..length].copy_from_slice(&cwd[..length]);
        // `list_directory` não aceita caminho vazio.
        let length = if length == 0 { 1 } else { length };
        if length == 1 && cwd.is_empty() {
            directory[0] = b'/';
        }
        return Some((directory, name.as_bytes()));
    }
    let mut resolved = [0u8; uri::MAX_PATH];
    let length = uri::resolve(cwd, base.as_bytes(), &mut resolved)?;
    directory[..length].copy_from_slice(&resolved[..length]);
    Some((directory, name.as_bytes()))
}

fn is_directory_entry(directory: &[u8], entry: &[u8]) -> bool {
    let total = directory.len() + 1 + entry.len();
    if total > uri::MAX_PATH {
        return false;
    }
    let mut path = [0u8; uri::MAX_PATH];
    path[..directory.len()].copy_from_slice(directory);
    let separator = usize::from(directory.last() != Some(&b'/'));
    if separator == 1 {
        path[directory.len()] = b'/';
    }
    let start = directory.len() + separator;
    path[start..start + entry.len()].copy_from_slice(entry);
    let length = start + entry.len();
    match core::str::from_utf8(&path[..length]) {
        Ok(path) => imagineos::fs::is_dir(path).unwrap_or(false),
        Err(_) => false,
    }
}

/// Itera as entradas separadas por `\n`.
fn split_entries(buffer: &[u8]) -> impl Iterator<Item = &[u8]> {
    buffer
        .split(|byte| *byte == b'\n')
        .filter(|entry| !entry.is_empty())
}

fn shared_prefix(left: &[u8], right: &[u8]) -> usize {
    let mut length = 0usize;
    while length < left.len() && length < right.len() && left[length] == right[length] {
        length += 1;
    }
    length
}
