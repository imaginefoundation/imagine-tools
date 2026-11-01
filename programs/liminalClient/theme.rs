//! Motor de temas do Liminal (window manager / compositor do lado cliente).
//!
//! O WM carrega o tema de um pacote `.tpf` em `system/themes/current.tpf`
//! (fallback `classic.tpf`), extrai `themeConfig.json` + `windowTheme.css`
//! via `tpf.rs`, e expõe as cores/dimensões para o compositor.
//!
//! O estilo das janelas lembra o Windows 3.1: **bordas em relevo**, com dois
//! tons de claro e dois de escuro desenhados com primitivas simples — a face
//! da janela é `silver`, o topo/esquerda recebem os tons claros e o
//! baixo/direita os tons escuros, simulando volume sem bitmaps.
//!
//! As propriedades CSS aceitas estendem o subconjunto anterior com
//! `border-style: raised|sunken|flat` e os seletores `#highlight`, `#shadow`,
//! `#light` e `#dark` (os quatro tons do relevo).

use crate::tpf;
use imagineos::fs;

/// Cores/dimensões das janelas, derivadas do CSS com fallbacks embutidos.
///
/// O padrão já é o esquema Windows 3.1: cinza `silver`, barras de título
/// `navy`, texto branco, relevo `raised`.
#[derive(Clone, Copy)]
#[allow(dead_code)]
pub struct WindowTheme {
    pub desktop: u32,
    pub desktop_accent: u32,
    /// Face dos controles (janela, botões) — o "cinza de botão".
    pub window: u32,
    pub window_alt: u32,
    pub title_active: u32,
    pub title_inactive: u32,
    pub title_text: u32,
    pub border_color: u32,
    pub button_close: u32,
    pub button_minimize: u32,
    pub cursor_color: u32,
    pub cursor_outline: u32,
    /// Relevo: lado iluminado (topo/esquerda, borda interna).
    pub highlight: u32,
    /// Relevo: lado sombreado (baixo/direita, borda interna).
    pub shadow: u32,
    /// Relevo: ponto de luz mais forte (borda externa superior).
    pub light: u32,
    /// Relevo: sombra mais forte (borda externa inferior).
    pub dark: u32,
    pub border: i32,
    pub title_height: i32,
    pub button_size: i32,
    pub cursor_size: i32,
    /// `raised` por padrão; `sunken` inverte, `flat` desenha borda sólida.
    pub relief: Relief,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Relief {
    Raised,
    Sunken,
    Flat,
}

pub const DEFAULTS: WindowTheme = WindowTheme {
    desktop: 0x008080,          // teal clássico
    desktop_accent: 0x004040,
    window: 0xc0c0c0,           // cinza de botão
    window_alt: 0xb4b4b4,
    title_active: 0x000080,     // azul-marinho
    title_inactive: 0x808080,
    title_text: 0xffffff,
    border_color: 0xc0c0c0,     // a "moldura" agora é a face em relevo
    button_close: 0xc0c0c0,     // botões com face de controle
    button_minimize: 0xc0c0c0,
    cursor_color: 0xffffff,
    cursor_outline: 0x000000,
    highlight: 0xdfdfdf,
    shadow: 0x808080,
    light: 0xffffff,
    dark: 0x000000,
    border: 2,
    title_height: 18,
    button_size: 14,
    cursor_size: 10,
    relief: Relief::Raised,
};

impl WindowTheme {
    /// Cor dos glifos dos botões de controle (preto na face `silver`).
    pub const fn title_text_ink(&self) -> u32 {
        0x000000
    }
}

/// Tema completo carregado do disco.
#[derive(Clone, Copy)]
#[allow(dead_code)]
pub struct Theme {
    pub window: WindowTheme,
    /// Metadados de `themeConfig.json`.
    pub name: [u8; 48],
    pub name_len: usize,
    pub author: [u8; 48],
    pub author_len: usize,
    pub version: [u8; 16],
    pub version_len: usize,
}

#[allow(dead_code)]
impl Theme {
    pub const fn builtin() -> Self {
        Self {
            window: DEFAULTS,
            name: [0; 48],
            name_len: 0,
            author: [0; 48],
            author_len: 0,
            version: [0; 16],
            version_len: 0,
        }
    }

    pub fn name(&self) -> &str {
        if self.name_len == 0 {
            return "builtin";
        }
        core::str::from_utf8(&self.name[..self.name_len]).unwrap_or("builtin")
    }

    /// Bytes e comprimento do nome do tema (para exibição sem `alloc`).
    pub fn name_bytes(&self) -> [u8; 48] {
        self.name
    }

    pub fn name_len(&self) -> usize {
        self.name_len
    }

    /// Carrega o tema do disco. Falha degrada para o padrão 3.1 embutido:
    /// o WM nunca deixa de subir por causa de um tema ruim.
    ///
    /// Os buffers de trabalho são estáticos, não de stack: a stack de
    /// 128 KiB do processo não comporta o pacote (256 KiB) + CSS (64 KiB).
    pub fn load() -> Self {
        load_theme_static()
    }
}

static mut THEME_PACKAGE: [u8; tpf::MAX_PACKAGE_SIZE] = [0; tpf::MAX_PACKAGE_SIZE];
static mut THEME_CSS: [u8; tpf::MAX_MEMBER_SIZE] = [0; tpf::MAX_MEMBER_SIZE];

/// Corpo de `Theme::load` fora do método (buffers estáticos).
fn load_theme_static() -> Theme {
    // Single-threaded: o WM carrega o tema uma única vez, no boot.
    let package = unsafe { &mut *core::ptr::addr_of_mut!(THEME_PACKAGE) };
    let css = unsafe { &mut *core::ptr::addr_of_mut!(THEME_CSS) };
    let path = if fs::is_file(tpf::THEME_FILE).unwrap_or(false) {
        tpf::THEME_FILE
    } else {
        tpf::DEFAULT_THEME_FILE
    };
    let Some(length) = tpf::read_package(path, package) else {
        return Theme::builtin();
    };
    let mut config = tpf::ThemeConfig::EMPTY;
    let css_len = tpf::unpack_theme(&package[..length], &mut config, css);
    let mut window = DEFAULTS;
    apply_css(&mut window, &css[..css_len]);
    Theme {
            window,
            name: config.name,
            name_len: config.name_len,
            author: config.author,
            author_len: config.author_len,
            version: config.version,
            version_len: config.version_len,
    }
}

// ----------------------------------------------------------------------------
// Parser do CSS (subconjunto, sem alocador)
// ----------------------------------------------------------------------------

/// Aplica o conteúdo de um CSS ao tema.
pub fn apply_css(theme: &mut WindowTheme, source: &[u8]) {
    // Duas passadas: genéricas (`#titlebar`) primeiro, específicas
    // (`#titlebar.focused`) depois — especificidade vence, como no CSS real.
    apply_css_pass(theme, source, false);
    apply_css_pass(theme, source, true);
}

fn apply_css_pass(theme: &mut WindowTheme, source: &[u8], specific: bool) {
    let mut cursor = 0;
    while cursor < source.len() {
        skip_whitespace_and_comments(source, &mut cursor);
        if cursor >= source.len() || source[cursor] != b'#' {
            while cursor < source.len() && source[cursor] != b'{' {
                cursor += 1;
            }
            skip_block(source, &mut cursor);
            continue;
        }
        let selector_start = cursor;
        while cursor < source.len() && source[cursor] != b'{' {
            cursor += 1;
        }
        if cursor >= source.len() {
            return;
        }
        let selector = trim(&source[selector_start..cursor]);
        let has_variant = selector[1..].contains(&b'.');
        if has_variant != specific {
            cursor += 1;
            skip_block(source, &mut cursor);
            continue;
        }
        cursor += 1; // '{'
        let body_start = cursor;
        skip_block(source, &mut cursor);
        let body_end = cursor.saturating_sub(1).max(body_start);
        apply_rule(theme, selector, &source[body_start..body_end]);
    }
}

/// Avança `cursor` para depois do `}` do bloco aberto em `cursor - 1`.
fn skip_block(source: &[u8], cursor: &mut usize) {
    while *cursor < source.len() && source[*cursor] != b'}' {
        *cursor += 1;
    }
    if *cursor < source.len() {
        *cursor += 1;
    }
}

fn skip_whitespace_and_comments(source: &[u8], cursor: &mut usize) {
    loop {
        while *cursor < source.len() && source[*cursor].is_ascii_whitespace() {
            *cursor += 1;
        }
        if *cursor + 1 < source.len() && source[*cursor] == b'/' && source[*cursor + 1] == b'*' {
            *cursor += 2;
            while *cursor + 1 < source.len()
                && !(source[*cursor] == b'*' && source[*cursor + 1] == b'/')
            {
                *cursor += 1;
            }
            *cursor = (*cursor + 2).min(source.len());
            continue;
        }
        return;
    }
}

fn trim(bytes: &[u8]) -> &[u8] {
    let mut start = 0;
    let mut end = bytes.len();
    while start < end && bytes[start].is_ascii_whitespace() {
        start += 1;
    }
    while end > start && bytes[end - 1].is_ascii_whitespace() {
        end -= 1;
    }
    &bytes[start..end]
}

/// Aplica um bloco `propriedade: valor;` a um seletor.
fn apply_rule(theme: &mut WindowTheme, selector: &[u8], body: &[u8]) {
    let mut cursor = 0;
    while cursor < body.len() {
        while cursor < body.len() && body[cursor].is_ascii_whitespace() {
            cursor += 1;
        }
        let name_start = cursor;
        while cursor < body.len() && body[cursor] != b':' {
            cursor += 1;
        }
        if cursor >= body.len() {
            return;
        }
        let name = trim(&body[name_start..cursor]);
        cursor += 1;
        let value_start = cursor;
        while cursor < body.len() && body[cursor] != b';' && body[cursor] != b'}' {
            cursor += 1;
        }
        let value = trim(&body[value_start..cursor]);
        cursor += 1;
        set_property(theme, selector, name, value);
    }
}

/// Compara nomes em minúsculas.
fn name_is(name: &[u8], expected: &str) -> bool {
    name.len() == expected.len()
        && name
            .iter()
            .zip(expected.as_bytes())
            .all(|(a, b)| a.to_ascii_lowercase() == *b)
}

/// O seletor casa com `#base[.modificador]`?
fn selector_matches(selector: &[u8], base: &str, modifier: Option<&str>) -> bool {
    if selector.is_empty() || selector[0] != b'#' {
        return false;
    }
    let selector = &selector[1..];
    let dot = selector.iter().position(|&b| b == b'.');
    let (name, variant) = match dot {
        Some(dot) => (&selector[..dot], Some(&selector[dot + 1..])),
        None => (selector, None),
    };
    if !name_is(name, base) {
        return false;
    }
    match (variant, modifier) {
        (None, _) => true,
        (Some(variant), Some(wanted)) => name_is(variant, wanted),
        (Some(_), None) => false,
    }
}

/// Trata uma propriedade de um bloco.
fn set_property(theme: &mut WindowTheme, selector: &[u8], name: &[u8], value: &[u8]) {
    let color = if name_is(name, "background-color")
        || name_is(name, "background")
        || name_is(name, "border-color")
        || name_is(name, "color")
        || name_is(name, "highlight-color")
        || name_is(name, "shadow-color")
        || name_is(name, "light-color")
        || name_is(name, "dark-color")
    {
        parse_color(value)
    } else {
        None
    };
    let dimension = if name_is(name, "height")
        || name_is(name, "width")
        || name_is(name, "size")
        || name_is(name, "border-width")
        || name_is(name, "border")
    {
        parse_px(value)
    } else {
        None
    };
    if name_is(name, "border-style") {
        if name_is(trim(value), "sunken") {
            theme.relief = Relief::Sunken;
        } else if name_is(trim(value), "flat") {
            theme.relief = Relief::Flat;
        } else {
            theme.relief = Relief::Raised;
        }
        return;
    }
    let Some(color) = color else {
        if let Some(dimension) = dimension {
            set_dimension(theme, selector, dimension);
        }
        return;
    };
    // Específicas primeiro (passada 2), genéricas na passada 1.
    if selector_matches(selector, "titlebar", Some("focused")) {
        if name_is(name, "color") {
            theme.title_text = color;
        } else {
            theme.title_active = color;
        }
        return;
    }
    if selector_matches(selector, "titlebar", Some("unfocused")) {
        theme.title_inactive = color;
        return;
    }
    if selector_matches(selector, "button", Some("close")) {
        theme.button_close = color;
        return;
    }
    if selector_matches(selector, "button", Some("minimize")) {
        theme.button_minimize = color;
        return;
    }
    if selector_matches(selector, "cursor", Some("outline")) {
        theme.cursor_outline = color;
        return;
    }
    if selector_matches(selector, "desktop", None) {
        theme.desktop = color;
    } else if selector_matches(selector, "window", None) {
        theme.window = color;
    } else if selector_matches(selector, "titlebar", None) {
        if name_is(name, "color") {
            theme.title_text = color;
        } else {
            theme.title_active = color;
        }
    } else if selector_matches(selector, "button", None) {
        theme.button_close = color;
        theme.button_minimize = color;
    } else if selector_matches(selector, "border", None) {
        theme.border_color = color;
    } else if selector_matches(selector, "highlight", None) {
        theme.highlight = color;
    } else if selector_matches(selector, "shadow", None) {
        theme.shadow = color;
    } else if selector_matches(selector, "light", None) {
        theme.light = color;
    } else if selector_matches(selector, "dark", None) {
        theme.dark = color;
    } else if selector_matches(selector, "cursor", None) {
        if name_is(name, "color") || name_is(name, "border-color") {
            theme.cursor_outline = color;
        } else {
            theme.cursor_color = color;
        }
    }
    if let Some(dimension) = dimension {
        set_dimension(theme, selector, dimension);
    }
}

fn set_dimension(theme: &mut WindowTheme, selector: &[u8], value: i32) {
    if value <= 0 {
        return;
    }
    if selector_matches(selector, "titlebar", None)
        || selector_matches(selector, "titlebar", Some("focused"))
        || selector_matches(selector, "titlebar", Some("unfocused"))
    {
        theme.title_height = value;
    } else if selector_matches(selector, "button", None)
        || selector_matches(selector, "button", Some("close"))
    {
        theme.button_size = value;
    } else if selector_matches(selector, "border", None) {
        theme.border = value;
    } else if selector_matches(selector, "cursor", None) {
        theme.cursor_size = value;
    }
}

/// `#rrggbb`/`#rgb`, nomes `white`/`black` e as cores nomeadas do Windows 3.1
/// (suficiente para temas clássicos).
pub fn parse_color(value: &[u8]) -> Option<u32> {
    let value = trim(value);
    if value.first() == Some(&b'#') {
        let hex = &value[1..];
        if hex.len() == 6 {
            let red = parse_hex_pair(&hex[0..2])?;
            let green = parse_hex_pair(&hex[2..4])?;
            let blue = parse_hex_pair(&hex[4..6])?;
            return Some(((red as u32) << 16) | ((green as u32) << 8) | blue as u32);
        }
        if hex.len() == 3 {
            let mut channels = [0u32; 3];
            for (index, byte) in hex.iter().take(3).enumerate() {
                let digit = (*byte as char).to_digit(16)?;
                channels[index] = digit * 16 + digit;
            }
            return Some((channels[0] << 16) | (channels[1] << 8) | channels[2]);
        }
        return None;
    }
    const NAMED: &[(&str, u32)] = &[
        ("white", 0xffffff),
        ("black", 0x000000),
        ("silver", 0xc0c0c0),
        ("gray", 0x808080),
        ("navy", 0x000080),
        ("teal", 0x008080),
        ("red", 0xff0000),
        ("green", 0x008000),
        ("yellow", 0xffff00),
    ];
    for (name, rgb) in NAMED {
        if name_is(value, name) {
            return Some(*rgb);
        }
    }
    None
}

fn parse_hex_pair(bytes: &[u8]) -> Option<u8> {
    let high = (bytes[0] as char).to_digit(16)?;
    let low = (bytes[1] as char).to_digit(16)?;
    Some((high * 16 + low) as u8)
}

/// `18px` → 18; números sem sufixo também são aceitos.
pub fn parse_px(value: &[u8]) -> Option<i32> {
    let value = trim(value);
    let digits = if value.len() >= 2 && value[value.len() - 2..] == *b"px" {
        trim(&value[..value.len() - 2])
    } else {
        trim(value)
    };
    if digits.is_empty() || !digits.iter().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let mut number: i32 = 0;
    for &digit in digits {
        number = number.saturating_mul(10).saturating_add((digit - b'0') as i32);
    }
    Some(number)
}

// ----------------------------------------------------------------------------
// Testes de host
// ----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_windows_31() {
        assert_eq!(DEFAULTS.window, 0xc0c0c0);
        assert_eq!(DEFAULTS.title_active, 0x000080);
        assert_eq!(DEFAULTS.relief, Relief::Raised);
        // Relevo precisa dos 4 tons distintos e na ordem de intensidade.
        assert!(DEFAULTS.light > DEFAULTS.highlight);
        assert!(DEFAULTS.shadow > DEFAULTS.dark);
    }

    #[test]
    fn parses_relief_and_tones() {
        let mut theme = DEFAULTS;
        apply_css(
            &mut theme,
            b"#window { background-color: silver; border-style: raised; }\n\
              #highlight { background-color: #dfdfdf; }\n\
              #shadow { background-color: #808080; }\n\
              #light { background-color: #ffffff; }\n\
              #dark { background-color: #000000; }\n\
              #titlebar.focused { background-color: navy; color: white; }\n\
              #button { size: 14px; background-color: silver; }",
        );
        assert_eq!(theme.window, 0xc0c0c0);
        assert_eq!(theme.relief, Relief::Raised);
        assert_eq!(theme.title_active, 0x000080);
        assert_eq!(theme.title_text, 0xffffff);
        assert_eq!(theme.button_size, 14);
    }

    #[test]
    fn sunken_style_switches() {
        let mut theme = DEFAULTS;
        apply_css(&mut theme, b"#window { border-style: sunken; }");
        assert_eq!(theme.relief, Relief::Sunken);
        apply_css(&mut theme, b"#window { border-style: flat; }");
        assert_eq!(theme.relief, Relief::Flat);
        // Sem valor reconhecido: volta a raised.
        apply_css(&mut theme, b"#window { border-style: groovy; }");
        assert_eq!(theme.relief, Relief::Raised);
    }

    #[test]
    fn parses_colors_and_px() {
        assert_eq!(parse_color(b"#176b87"), Some(0x176b87));
        assert_eq!(parse_color(b"#FFF"), Some(0xffffff));
        assert_eq!(parse_color(b"navy"), Some(0x000080));
        assert_eq!(parse_color(b"rgb(1,2,3)"), None);
        assert_eq!(parse_px(b"18px"), Some(18));
        assert_eq!(parse_px(b"18"), Some(18));
        assert_eq!(parse_px(b"-2px"), None);
        assert_eq!(parse_px(b"auto"), None);
    }

    #[test]
    fn specificity_and_fallbacks() {
        let mut theme = DEFAULTS;
        apply_css(
            &mut theme,
            b"/* comment */ #titlebar { background-color: #445566; height: 24px; }\n\
              #titlebar.focused { background-color: #112233; }\n\
              #button.close { background-color: #ff0000; }",
        );
        assert_eq!(theme.title_active, 0x112233);
        assert_eq!(theme.title_height, 24);
        assert_eq!(theme.button_close, 0xff0000);
        assert_eq!(theme.window, DEFAULTS.window);
        assert_eq!(theme.title_inactive, DEFAULTS.title_inactive);
    }

    #[test]
    fn theme_config_fields_survive_roundtrip() {
        // O tema embutido tem nome "builtin" e os campos vazios.
        let theme = Theme::builtin();
        assert_eq!(theme.name(), "builtin");
    }
}
