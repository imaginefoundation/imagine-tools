//! Histórico de comandos do shell.
//!
//! Ring buffer de entradas de tamanho fixo: o shell não tem alocador, então o
//! armazenamento é um array na pilha e a política é "a mais antiga sai". A
//! numeração das entradas é monotônica (`1` é o primeiro comando da sessão) e
//! não reinicia quando o buffer dá a volta, para que o utilitário `history`
//! possa listar em ordem numérica mesmo depois de o buffer rodar.

/// Entradas guardadas por sessão.
pub const MAX_ENTRIES: usize = 64;
/// Comprimento máximo de uma entrada guardada.
pub const MAX_ENTRY: usize = 256;

pub struct History {
    entries: [[u8; MAX_ENTRY]; MAX_ENTRIES],
    lengths: [usize; MAX_ENTRIES],
    /// Posição de escrita no ring buffer.
    next: usize,
    /// Quantas entradas já foram gravadas (pode exceder `MAX_ENTRIES`).
    total: usize,
}

impl History {
    pub const fn new() -> Self {
        Self {
            entries: [[0; MAX_ENTRY]; MAX_ENTRIES],
            lengths: [0; MAX_ENTRIES],
            next: 0,
            total: 0,
        }
    }

    /// Quantas entradas estão disponíveis agora.
    pub fn len(&self) -> usize {
        self.total.min(MAX_ENTRIES)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Número da primeira entrada ainda disponível (1-based).
    pub fn first_number(&self) -> usize {
        self.total.saturating_sub(self.len()) + 1
    }

    /// Acrescenta um comando. Linhas em branco e repetições consecutivas são
    /// ignoradas, como fazem os shells interativos.
    pub fn push(&mut self, line: &[u8]) {
        if line.iter().all(|byte| byte.is_ascii_whitespace()) {
            return;
        }
        if let Some(last) = self.last() {
            if last == line {
                return;
            }
        }
        let length = line.len().min(MAX_ENTRY);
        let slot = self.next;
        self.entries[slot][..length].copy_from_slice(&line[..length]);
        self.lengths[slot] = length;
        self.next = (self.next + 1) % MAX_ENTRIES;
        self.total += 1;
    }

    /// Entrada `position` na ordem cronológica (`0` = mais antiga).
    pub fn get(&self, position: usize) -> Option<&[u8]> {
        let available = self.len();
        if position >= available {
            return None;
        }
        // Quando o buffer já deu a volta, a entrada mais antiga é a que será
        // sobrescrita em seguida.
        let start = if self.total > MAX_ENTRIES {
            self.next
        } else {
            0
        };
        let slot = (start + position) % MAX_ENTRIES;
        Some(&self.entries[slot][..self.lengths[slot]])
    }

    fn last(&self) -> Option<&[u8]> {
        if self.is_empty() {
            return None;
        }
        self.get(self.len() - 1)
    }

    /// Executa `visit` para cada entrada, na ordem cronológica, com o número.
    pub fn for_each(&self, mut visit: impl FnMut(usize, &[u8])) {
        let first = self.first_number();
        for position in 0..self.len() {
            if let Some(entry) = self.get(position) {
                visit(first + position, entry);
            }
        }
    }
}
