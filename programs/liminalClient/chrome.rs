//! Desenho das molduras de janela — o "chrome" — no relevo clássico do
//! Windows 3.1.
//!
//! O relevo usa quatro tons desenhados como faixas sólidas de 1 px, na mesma
//! receita das UIs dos anos 90:
//!
//! ```text
//!      light    highlight
//!      ┌────────────────────┐ shadow
//! dark │    face (silver)   │
//!      └────────────────────┘
//! ```
//!
//! Tudo com primitivas (`fill`) sobre a superfície da janela — sem bitmaps —
//! de modo que um tema precisa apenas das quatro cores para descrever o
//! relevo inteiro.
use crate::theme::{Relief, WindowTheme};
use imagineos::surface::Surface;

/// Converte um RGB de 24 bits no valor do buffer de superfície. As superfícies
/// do kernel compartilham o formato do framebuffer (tipicamente RGB 8:8:8 nos
/// deslocamentos 16/8/0 — o mesmo que os clientes já usam como constante).
fn rgb(value: u32) -> u32 {
    value
}

/// Elevação: o lado claro é o superior/esquerdo (botão em relevo para fora).
fn raised_edge(surface: &Surface, theme: &WindowTheme, x: i32, y: i32, w: i32, h: i32) {
    let light = rgb(theme.light);
    let highlight = rgb(theme.highlight);
    let shadow = rgb(theme.shadow);
    let dark = rgb(theme.dark);
    // Borda externa.
    surface.fill(x, y, w, 1, light);
    surface.fill(x, y + h - 1, w, 1, dark);
    surface.fill(x, y, 1, h, light);
    surface.fill(x + w - 1, y, 1, h, dark);
    // Borda interna.
    surface.fill(x + 1, y + 1, w - 2, 1, highlight);
    surface.fill(x + 1, y + h - 2, w - 2, 1, shadow);
    surface.fill(x + 1, y + 1, 1, h - 2, highlight);
    surface.fill(x + w - 2, y + 1, 1, h - 2, shadow);
}

/// Afundamento: invertido — o lado sombreado é o superior/esquerdo (campo de
/// texto, interior de painel).
fn sunken_edge(surface: &Surface, theme: &WindowTheme, x: i32, y: i32, w: i32, h: i32) {
    let light = rgb(theme.light);
    let highlight = rgb(theme.highlight);
    let shadow = rgb(theme.shadow);
    let dark = rgb(theme.dark);
    surface.fill(x, y, w, 1, dark);
    surface.fill(x, y + h - 1, w, 1, light);
    surface.fill(x, y, 1, h, dark);
    surface.fill(x + w - 1, y, 1, h, light);
    surface.fill(x + 1, y + 1, w - 2, 1, shadow);
    surface.fill(x + 1, y + h - 2, w - 2, 1, highlight);
    surface.fill(x + 1, y + 1, 1, h - 2, shadow);
    surface.fill(x + w - 2, y + 1, 1, h - 2, highlight);
}

/// Borda plana: um único tom escuro emoldurando (janelas inativas do 3.x).
fn flat_edge(surface: &Surface, theme: &WindowTheme, x: i32, y: i32, w: i32, h: i32) {
    let shadow = rgb(theme.shadow);
    surface.fill(x, y, w, 1, shadow);
    surface.fill(x, y + h - 1, w, 1, shadow);
    surface.fill(x, y, 1, h, shadow);
    surface.fill(x + w - 1, y, 1, h, shadow);
}

/// Desenha a moldura no relevo configurado pelo tema.
pub fn edge(
    surface: &Surface,
    theme: &WindowTheme,
    relief: Relief,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
) {
    if w <= 2 || h <= 2 {
        return;
    }
    match relief {
        Relief::Raised => raised_edge(surface, theme, x, y, w, h),
        Relief::Sunken => sunken_edge(surface, theme, x, y, w, h),
        Relief::Flat => flat_edge(surface, theme, x, y, w, h),
    }
}

/// Botão de controle (`silver` em relevo, com o glifo central).
pub fn button(
    surface: &Surface,
    theme: &WindowTheme,
    x: i32,
    y: i32,
    size: i32,
    glyph: Glyph,
) {
    if size < 4 {
        return;
    }
    let face = rgb(theme.window);
    surface.fill(x + 1, y + 1, size - 2, size - 2, face);
    raised_edge(surface, theme, x, y, size, size);
    let ink = rgb(theme.title_text_ink());
    match glyph {
        Glyph::Close => {
            // "×": duas diagonais de 2 px.
            let step = (size - 6) / 3;
            for index in 0..(size - 6) {
                let offset = (index * 2).min(2);
                surface.fill(x + 3 + index, y + 3 + offset, 1, 1, ink);
                surface.fill(x + 3 + index, y + 3 + 1 + offset, 1, 1, ink);
            }
            let _ = step;
        }
        Glyph::Minimize => {
            // "▁": barra inferior.
            surface.fill(x + 3, y + size - 5, size - 6, 2, ink);
        }
        Glyph::Box => {
            // "□": quadrado central.
            surface.fill(x + 3, y + 3, size - 6, size - 6, ink);
            surface.fill(x + 4, y + 4, size - 8, size - 8, face);
        }
    }
}

/// Glifo central de um botão de controle.
#[derive(Clone, Copy)]
#[allow(dead_code)]
pub enum Glyph {
    Close,
    Minimize,
    Box,
}

/// Desenha a barra de título: faixa sólida com o título à esquerda e os
/// botões de controle à direita (ordem do 3.1: minimizar, maximizar, fechar).
pub fn title_bar(
    surface: &Surface,
    theme: &WindowTheme,
    focused: bool,
    x: i32,
    y: i32,
    width: i32,
    title: &str,
) {
    if width < 8 {
        return;
    }
    let background = if focused {
        theme.title_active
    } else {
        theme.title_inactive
    };
    surface.fill(x, y, width, theme.title_height, background);
    let text_color = rgb(theme.title_text);
    let mut cursor_x = x + 4;
    let baseline = y + (theme.title_height - 8) / 2;
    for character in title.chars() {
        if cursor_x + 9 > x + width - theme.button_size * 3 - 10 {
            break;
        }
        draw_glyph(surface, cursor_x, baseline, character, text_color);
        cursor_x += 9;
    }
}

fn draw_glyph(surface: &Surface, x: i32, y: i32, character: char, color: u32) {
    let bitmap = imagineos::font::glyph(character);
    for (row, bits) in bitmap.iter().enumerate() {
        for column in 0..8 {
            if bits & (0x80 >> column) != 0 {
                surface.pixel(x + column as i32, y + row as i32, color);
            }
        }
    }
}
