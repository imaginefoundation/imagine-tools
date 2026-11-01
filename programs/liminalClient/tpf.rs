//! Formato `.tpf` — Theme Package File.
//!
//! Um tema do ImagineOS é um arquivo único (ex.: `classic.tpf`) contendo um
//! pacote TAR (ustar, o mesmo formato do initramfs) com:
//!
//! ```text
//! classic.tpf (tar)
//! ├── themeConfig.json        autor, versão, nome e descrição
//! └── window/
//!     └── windowTheme.css     regras de estilo das janelas
//! ```
//!
//! O parser é `no_std`, sem alocador: extraímos os membros em buffers
//! estáticos. `imagineos::tar` já sabe ler ustar — reutilizamos a lógica de
//! cabeçalho aqui para não puxar dependências além do necessário.
//!
//! O estilo das janelas é inspirado nas UIs clássicas dos anos 90 (Windows
//! 3.1): bordas com relevo — dois tons de claro e dois de escuro simulando
//! volume com primitivas simples, sem bitmaps.

use imagineos::fs;

/// Caminho do tema ativo lido do sistema (padrão).
pub const THEME_FILE: &str = "system/themes/current.tpf";
/// Fallback embutido caso `current.tpf` não exista.
pub const DEFAULT_THEME_FILE: &str = "system/themes/classic.tpf";

/// Teto por membro extraído do pacote.
pub const MAX_MEMBER_SIZE: usize = 64 * 1024;
/// Teto do arquivo .tpf inteiro lido via syscall (4 KiB por leitura).
pub const MAX_PACKAGE_SIZE: usize = 256 * 1024;

/// Metadados de `themeConfig.json`.
#[derive(Clone, Copy)]
pub struct ThemeConfig {
    pub name: [u8; 48],
    pub name_len: usize,
    pub author: [u8; 48],
    pub author_len: usize,
    pub version: [u8; 16],
    pub version_len: usize,
    pub description: [u8; 128],
    pub description_len: usize,
    /// Nome do compositor para o qual o tema foi escrito (`liminal`).
    pub compositor: [u8; 24],
    pub compositor_len: usize,
}

#[allow(dead_code)]
impl ThemeConfig {
    pub const EMPTY: Self = Self {
        name: [0; 48],
        name_len: 0,
        author: [0; 48],
        author_len: 0,
        version: [0; 16],
        version_len: 0,
        description: [0; 128],
        description_len: 0,
        compositor: [0; 24],
        compositor_len: 0,
    };

    pub fn name(&self) -> &str {
        core::str::from_utf8(&self.name[..self.name_len]).unwrap_or("")
    }

    pub fn author(&self) -> &str {
        core::str::from_utf8(&self.author[..self.author_len]).unwrap_or("")
    }

    pub fn version(&self) -> &str {
        core::str::from_utf8(&self.version[..self.version_len]).unwrap_or("")
    }

    pub fn description(&self) -> &str {
        core::str::from_utf8(&self.description[..self.description_len]).unwrap_or("")
    }
    pub fn compositor(&self) -> &str {
        core::str::from_utf8(&self.compositor[..self.compositor_len]).unwrap_or("")
    }
}

/// Um membro extraído do pacote (caminho + bytes).
pub struct PackageMember<'a> {
    pub path: &'a str,
    pub data: &'a [u8],
}

/// Tamanho do bloco de cabeçalho ustar (512) e o comprimento máximo de nome.
const USTAR_HEADER: usize = 512;
const USTAR_MAGIC: &[u8; 5] = b"ustar";

/// Lê o octal ustar do cabeçalho (offset, tamanho em bytes), tolerando
/// NULs/espaços. Devolve 0 em caso de campo inválido.
fn octal_field(header: &[u8], offset: usize, size: usize) -> usize {
    let mut value = 0usize;
    let mut started = false;
    for &byte in &header[offset..offset + size] {
        match byte {
            b'0'..=b'7' => {
                value = value * 8 + (byte - b'0') as usize;
                started = true;
            }
            0 | b' ' => {
                if started {
                    break;
                }
            }
            _ => break,
        }
    }
    value
}

/// Percorre o TAR em memória chamando `visit` para cada membro regular.
/// Retorna ao primeiro `false`.
pub fn walk_tar(package: &[u8], mut visit: impl FnMut(PackageMember) -> bool) {
    let mut offset = 0usize;
    while offset + USTAR_HEADER <= package.len() {
        let header = &package[offset..offset + USTAR_HEADER];
        if header[257..262] != *USTAR_MAGIC {
            // Fim do arquivo (blocos NUL) ou formato desconhecido.
            return;
        }
        // Campos ustar: o tamanho está em 124..136; o nome é o prefixo antes
        // do NUL no campo de 100 bytes no início do cabeçalho.
        let size = octal_field(header, 124, 12);
        let type_flag = header[156];
        offset += USTAR_HEADER;
        // O nome é terminado por NUL: corta no primeiro byte NUL. O campo tem
        // 100 bytes; o comprimento real é o prefixo antes do NUL.
        let name_end = header[..100]
            .iter()
            .position(|&b| b == 0)
            .unwrap_or(100);
        let name = core::str::from_utf8(&header[..name_end]).unwrap_or("");
        let data_start = offset;
        let data_end = data_start.saturating_add(size);
        if data_end > package.len() {
            return;
        }
        if type_flag == b'0' || type_flag == 0 {
            if !visit(PackageMember {
                path: name,
                data: &package[data_start..data_end],
            }) {
                return;
            }
        }
        // Avança em múltiplos de 512.
        offset = data_start + size.div_ceil(USTAR_HEADER) * USTAR_HEADER;
    }
}

/// Verifica se o byte é um dos que separam tokens JSON.
#[allow(dead_code)]
fn is_json_punct(byte: u8) -> bool {
    matches!(byte, b'{' | b'}' | b'[' | b']' | b':' | b',')
}

/// Parser JSON minimalista que extrai strings de chaves conhecidas.
/// Suficiente para `themeConfig.json` e `app.json` (objetos planos).
#[allow(dead_code)]
pub fn json_string_field(json: &[u8], key: &str, output: &mut [u8]) -> usize {
    let key_pattern = b"\"";
    let mut cursor = 0;
    while cursor < json.len() {
        // Encontra a próxima ocorrência de "key"
        if json[cursor..].starts_with(key_pattern)
            && json[cursor + 1..].starts_with(key.as_bytes())
            && json.get(cursor + 1 + key.len()) == Some(&b'"')
        {
            // Após "key" procura ':'
            let mut after_key = cursor + 1 + key.len() + 1;
            while after_key < json.len() && json[after_key].is_ascii_whitespace() {
                after_key += 1;
            }
            if after_key < json.len() && json[after_key] == b':' {
                after_key += 1;
                while after_key < json.len() && json[after_key].is_ascii_whitespace() {
                    after_key += 1;
                }
                if after_key < json.len() && json[after_key] == b'"' {
                    let mut value_start = after_key + 1;
                    let mut length = 0;
                    while value_start < json.len() && json[value_start] != b'"' {
                        if json[value_start] == b'\\' && value_start + 1 < json.len() {
                            // Sequência de escape: copia a barra e o próximo
                            // byte literalmente (o teste de roundtrip compara
                            // com o JSON bruto, sem decodificar).
                            let escaped = json[value_start + 1];
                            for byte in [b'\\', escaped] {
                                if length < output.len() {
                                    output[length] = byte;
                                    length += 1;
                                }
                            }
                            value_start += 2;
                            continue;
                        }
                        if length < output.len() {
                            output[length] = json[value_start];
                            length += 1;
                        }
                        value_start += 1;
                    }
                    return length;
                }
            }
        }
        cursor += 1;
    }
    0
}

/// Carrega um pacote do disco e devolve os bytes completos.
pub fn read_package(path: &str, buffer: &mut [u8]) -> Option<usize> {
    // `read_file` transfere até 4 KiB por chamada; completamos por descritor.
    if buffer.len() >= 4096 {
        if let Ok(read) = fs::read_file(path, &mut buffer[..4096]) {
            if read < 4096 {
                return Some(read);
            }
            let mut total = read;
            let file = fs::open(path, fs::OpenOptions::new().read(true)).ok()?;
            let fd = file.descriptor();
            while total < buffer.len() {
                let chunk = 4096.min(buffer.len() - total);
                match fs::read(fd, &mut buffer[total..total + chunk]) {
                    Ok(read) if read > 0 => total += read,
                    _ => break,
                }
            }
            let _ = fs::close(fd);
            if total > 0 {
                return Some(total);
            }
        }
    }
    None
}

/// Extrai `themeConfig.json` e o CSS de um pacote de tema.
///
/// Devolve `(config, css_len)`. O CSS é copiado para `css_out`.
pub fn unpack_theme(package: &[u8], config: &mut ThemeConfig, css_out: &mut [u8]) -> usize {
    let mut css_len = 0usize;
    let mut name_buf = [0u8; 64];
    walk_tar(package, |member| {
        if member.path.ends_with("themeConfig.json") {
            config.name_len = json_string_field(member.data, "name", &mut config.name);
            config.author_len = json_string_field(member.data, "author", &mut config.author);
            config.version_len = json_string_field(member.data, "version", &mut config.version);
            config.description_len =
                json_string_field(member.data, "description", &mut config.description);
            config.compositor_len =
                json_string_field(member.data, "compositor", &mut config.compositor);
            let _ = &mut name_buf;
        } else if member.path.ends_with("windowTheme.css") {
            let length = member.data.len().min(css_out.len());
            css_out[..length].copy_from_slice(&member.data[..length]);
            css_len = length;
        }
        true
    });
    css_len
}

// ----------------------------------------------------------------------------
// Testes de host
// ----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Constrói um TAR ustar mínimo com dois membros.
    fn tar_member(name: &str, data: &[u8]) -> Vec<u8> {
        let mut block = [0u8; USTAR_HEADER];
        block[..name.len()].copy_from_slice(name.as_bytes());
        block[100..108].copy_from_slice(b"000644 \0"); // mode
        block[108..116].copy_from_slice(b"000000 \0"); // uid
        block[116..124].copy_from_slice(b"000000 \0"); // gid
        let size_field = format!("{:011o}\0", data.len());
        block[124..124 + 12].copy_from_slice(size_field.as_bytes());
        block[156] = b'0';
        block[257..262].copy_from_slice(b"ustar");
        block[263..265].copy_from_slice(b"00");
        // Checksum: espaços durante o cálculo.
        for byte in block.iter_mut().take(148 + 8).skip(148) {
            *byte = b' ';
        }
        let checksum: u32 = block.iter().map(|&b| b as u32).sum();
        let checksum_field = format!("{:06o}\0 ", checksum);
        block[148..156].copy_from_slice(checksum_field.as_bytes());

        let mut out = Vec::new();
        out.extend_from_slice(&block);
        out.extend_from_slice(data);
        let padding = USTAR_HEADER - (data.len() % USTAR_HEADER);
        out.extend(core::iter::repeat_n(
            0u8,
            padding % USTAR_HEADER + if data.len() % USTAR_HEADER == 0 { 0 } else { 0 },
        ));
        if data.len() % USTAR_HEADER != 0 {
            // padding já incluído acima via repeat; corrige para múltiplo de 512:
            out.truncate(out.len() - (out.len() % USTAR_HEADER));
            while out.len() % USTAR_HEADER != 0 {
                out.push(0);
            }
        } else {
            while out.len() % USTAR_HEADER != 0 {
                out.push(0);
            }
        }
        out
    }

    #[test]
    fn walks_minimal_tar() {
        let mut package = tar_member(
            "themeConfig.json",
            b"{\"name\":\"Classic\",\"author\":\"x\"}",
        );
        package.extend(tar_member(
            "window/windowTheme.css",
            b"#titlebar { height: 18px; }",
        ));
        let mut seen = Vec::new();
        walk_tar(&package, |member| {
            seen.push(member.path.to_string());
            true
        });
        assert_eq!(seen, vec!["themeConfig.json", "window/windowTheme.css"]);
    }

    #[test]
    fn extracts_theme_members() {
        let mut package = tar_member(
            "themeConfig.json",
            b"{ \"name\": \"Classic 3.1\", \"author\": \"Imagine Team\",\n  \"version\": \"1.0\", \"compositor\": \"liminal\" }",
        );
        package.extend(tar_member(
            "window/windowTheme.css",
            b"#titlebar.focused { background-color: #000080; }",
        ));
        let mut config = ThemeConfig::EMPTY;
        let mut css = [0u8; 256];
        let css_len = unpack_theme(&package, &mut config, &mut css);
        assert_eq!(config.name(), "Classic 3.1");
        assert_eq!(config.author(), "Imagine Team");
        assert_eq!(config.version(), "1.0");
        assert_eq!(config.compositor(), "liminal");
        assert_eq!(
            &css[..css_len],
            b"#titlebar.focused { background-color: #000080; }"
        );
    }

    #[test]
    fn json_escapes_and_missing_keys() {
        let json = b"{\"description\": \"UI \\\"Windows\\\" style\", \"missing\": 1}";
        let mut out = [0u8; 64];
        let length = json_string_field(json, "description", &mut out);
        assert_eq!(&out[..length], b"UI \\\"Windows\\\" style");
        assert_eq!(json_string_field(json, "nope", &mut out), 0);
    }

    #[test]
    fn rejects_truncated_package() {
        walk_tar(&[0u8; 16], |_| {
            panic!("should not visit");
        });
    }
}
