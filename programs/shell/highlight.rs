//! Realce de sintaxe da linha de comando.
//!
//! O shell conhece a gramática da própria linha, então pode colorir enquanto o
//! usuário digita: o primeiro token é o comando, o resto são argumentos, e
//! operadores/redirecionamentos, aspas e variáveis têm cores próprias. Cores
//! ANSI simples, no espírito do `zsh-syntax-highlighting`.

/// Palavras reservadas da linguagem do shell (`if`, `for`, ... não existem
/// ainda, mas o realce já as reconhece para quando existirem).

const KEYWORDS: [&[u8]; 7] = [b"if", b"then", b"else", b"fi", b"for", b"do", b"done"];

/// Nomes de arquivos/operadores que mudam de cor.
const REDIRECTS: [&[u8]; 3] = [b">", b">>", b"<"];

pub const RESET: &[u8] = b"\x1b[0m";
/// Nome de comando: verde-negrito.
const COMMAND: &[u8] = b"\x1b[1;32m";
/// Argumentos: cinza claro.
const ARGUMENT: &[u8] = b"\x1b[0;37m";
/// Palavras reservadas: magenta.
const KEYWORD: &[u8] = b"\x1b[1;35m";
/// Operadores (`|`, `&&`, `;`): amarelo.
const OPERATOR: &[u8] = b"\x1b[1;33m";
/// Redirecionamento: ciano.
const REDIRECT: &[u8] = b"\x1b[1;36m";
/// Conteúdo entre aspas: azul.
const QUOTED: &[u8] = b"\x1b[0;34m";
/// `$VAR`/`${VAR}`: amarelo-claro.
const VARIABLE: &[u8] = b"\x1b[0;93m";
/// Caminho absoluto/relativo: sublinhado.
const PATH: &[u8] = b"\x1b[4;37m";

/// Escreve `line` com realce, sem `\n`.
pub fn write_highlighted(line: &[u8]) {
    let mut state = State {
        first_word: true,
        quote: None,
    };
    let mut position = 0usize;
    while position < line.len() {
        let byte = line[position];
        if state.quote.is_some() {
            let end = quoted_end(line, position, state.quote);
            crate::write(QUOTED);
            crate::write(&line[position..end]);
            position = end;
            continue;
        }
        if byte == b' ' || byte == b'\t' {
            crate::write(b" ");
            position += 1;
            continue;
        }
        if byte == b'\'' || byte == b'"' {
            state.quote = Some(byte);
            let end = quoted_end(line, position + 1, Some(byte));
            crate::write(QUOTED);
            crate::write(&line[position..end]);
            position = end;
            state.quote = None;
            state.first_word = false;
            continue;
        }
        if byte == b'#' && state.first_word {
            crate::write(b"\x1b[0;90m");
            crate::write(&line[position..]);
            crate::write(RESET);
            return;
        }
        let end = word_end(line, position);
        let word = &line[position..end];
        color_for(word, &mut state);
        crate::write(word);
        crate::write(RESET);
        position = end;
    }
    crate::write(RESET);
}

struct State {
    /// Ainda não vimos o nome do comando nesta linha.
    first_word: bool,
    /// Delimitador de aspas aberto, se houver.
    quote: Option<u8>,
}

fn color_for(word: &[u8], state: &mut State) {
    if REDIRECTS.contains(&word) {
        crate::write(REDIRECT);
        return;
    }
    if word == b"|" || word == b"&&" || word == b"||" || word == b";" || word == b"&" {
        crate::write(OPERATOR);
        // Depois de um operador, o próximo token é de novo um comando.
        state.first_word = true;
        return;
    }
    if state.first_word {
        state.first_word = false;
        if KEYWORDS.contains(&word) {
            crate::write(KEYWORD);
        } else {
            crate::write(COMMAND);
        }
        return;
    }
    if KEYWORDS.contains(&word) {
        crate::write(KEYWORD);
        return;
    }
    if word.first() == Some(&b'$') {
        crate::write(VARIABLE);
        return;
    }
    if word.first() == Some(&b'/') || word.starts_with(b"./") || word.starts_with(b"../") {
        crate::write(PATH);
        return;
    }
    crate::write(ARGUMENT);
}

/// Fim do conteúdo entre aspas, começando em `position` com `delimiter` aberto.
fn quoted_end(line: &[u8], position: usize, delimiter: Option<u8>) -> usize {
    let Some(delimiter) = delimiter else {
        return line.len();
    };
    let mut index = position;
    while index < line.len() {
        if line[index] == b'\\' {
            index += 2;
            continue;
        }
        if line[index] == delimiter {
            return index + 1;
        }
        index += 1;
    }
    line.len()
}

/// Fim de uma palavra, parando em espaço ou em aspas (que mudam de cor).
fn word_end(line: &[u8], position: usize) -> usize {
    let mut index = position;
    while index < line.len() {
        let byte = line[index];
        if byte == b' ' || byte == b'\t' || byte == b'\'' || byte == b'"' {
            break;
        }
        index += 1;
    }
    index
}
