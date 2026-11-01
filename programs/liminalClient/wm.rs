//! Gerenciador de janelas do Liminal (lado cliente da stack).
//!
//! Cada janela é uma `Surface` do kernel cujo buffer contém a moldura em
//! relevo (Windows 3.1) e o conteúdo. O WM posiciona cada superfície com
//! `move_to`, trata eventos (arrasto pela barra de título, foco, botões) e
//! redesenha só o que mudou.
//!
//! O "desktop" também é uma superfície: uma faixa fina no rodapé com o nome
//! do tema carregado do `.tpf` (o painel do EDE ocupa o topo).
use crate::chrome::{self, Glyph};
use crate::theme::{Relief, Theme};
use imagineos::abi::SurfaceEvent;
use imagineos::surface::{self, Surface, EVENT_KEY, EVENT_MOUSE_BUTTON, EVENT_MOUSE_MOVE};
use imagineos::{console, process};

/// Dimensões das janelas do WM (o teto do protocolo é 320x200).
const WINDOW_WIDTH: i32 = 300;
const WINDOW_HEIGHT: i32 = 170;
/// O desktop é a superfície 0 e cobre a tela inteira até o teto do protocolo.
const DESKTOP_WIDTH: i32 = 320;
const DESKTOP_HEIGHT: i32 = 200;
/// Altura da faixa do desktop (rodapé, quando a tela é maior que a superfície).
const DESKTOP_BAR_HEIGHT: i32 = 20;

/// O WM mantém no máximo 3 janelas + 1 desktop (deixando slots para o painel
/// e um cliente externo).
const MAX_WINDOWS: usize = 3;

struct Window {
    surface: Surface,
    title: [u8; 24],
    title_len: usize,
    x: i32,
    y: i32,
    kind: AppKind,
}

#[derive(Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
enum AppKind {
    Terminal,
    Monitor,
    About,
}

impl Window {
    fn title(&self) -> &str {
        core::str::from_utf8(&self.title[..self.title_len]).unwrap_or("")
    }

    fn set_title(&mut self, title: &str) {
        let length = title.len().min(self.title.len());
        self.title[..length].copy_from_slice(&title.as_bytes()[..length]);
        self.title_len = length;
    }
}

pub struct WindowManager {
    theme: Theme,
    desktop: Option<Surface>,
    windows: [Option<Window>; MAX_WINDOWS],
    window_count: usize,
    focused: usize,
    /// Janela em arrasto (índice) e o deslocamento do ponteiro dentro dela.
    dragging: Option<(usize, i32, i32)>,
    drag_moved: bool,
    theme_name: [u8; 48],
    theme_name_len: usize,
}

impl WindowManager {
    /// Boot do WM: carrega o tema e cria o desktop + janelas iniciais.
    ///
    /// O servidor (liminalServer) precisa estar no ar — sem ele, as superfícies
    /// existem mas nunca aparecem; o init só lança os dois juntos, então falha
    /// aqui significa "sem sessão gráfica" e devolvemos o controle ao getty.
    pub fn boot() -> Result<Self, ()> {
        let theme = Theme::load();
        let mut wm = Self {
            theme,
            desktop: None,
            windows: core::array::from_fn(|_| None),
            window_count: 0,
            focused: 0,
            dragging: None,
            drag_moved: false,
            theme_name: theme.name_bytes(),
            theme_name_len: theme.name_len(),
        };
        wm.create_desktop()?;
        wm.spawn("Astrid Terminal", AppKind::Terminal, 12, 28)?;
        wm.spawn("System Monitor", AppKind::Monitor, 60, 96)?;
        Ok(wm)
    }

    fn create_desktop(&mut self) -> Result<(), ()> {
        let desktop = Surface::create(DESKTOP_WIDTH as usize, DESKTOP_HEIGHT as usize)
            .map_err(|_| ())?;
        desktop.present().map_err(|_| ())?;
        self.desktop = Some(desktop);
        self.draw_desktop();
        Ok(())
    }

    fn spawn(&mut self, title: &str, kind: AppKind, x: i32, y: i32) -> Result<(), ()> {
        if self.window_count >= MAX_WINDOWS {
            return Err(());
        }
        let surface = Surface::create(WINDOW_WIDTH as usize, WINDOW_HEIGHT as usize)
            .map_err(|_| ())?;
        surface.present().map_err(|_| ())?;
        let _ = surface.move_to(x, y);
        let mut title_bytes = [0u8; 24];
        let length = title.len().min(title_bytes.len());
        title_bytes[..length].copy_from_slice(&title.as_bytes()[..length]);
        let index = self.window_count;
        self.windows[index] = Some(Window {
            surface,
            title: title_bytes,
            title_len: length,
            x,
            y,
            kind,
        });
        self.window_count += 1;
        self.focused = index;
        self.redraw(index);
        Ok(())
    }

    pub fn run(&mut self) -> ! {
        loop {
            self.poll_events();
            if process::yield_now().is_err() {
                process::exit(1);
            }
        }
    }

    fn poll_events(&mut self) {
        for _ in 0..32 {
            let event = match surface::poll_event() {
                Ok(Some(event)) => event,
                Ok(None) | Err(_) => break,
            };
            match event.kind {
                EVENT_MOUSE_BUTTON => self.on_mouse_button(&event),
                EVENT_MOUSE_MOVE => self.on_mouse_move(&event),
                EVENT_KEY => self.on_key(&event),
                _ => {}
            }
        }
    }

    /// Índice da janela dona da superfície do evento.
    fn window_index(&self, surface_id: u32) -> Option<usize> {
        self.windows[..self.window_count].iter().position(|slot| {
            slot.as_ref().is_some_and(|window| window.surface.id() == surface_id)
        })
    }

    /// A barra de título da janela sob as coordenadas locais do evento?
    #[allow(dead_code)]
    fn holds_title(_window: &Window, x: i32, y: i32) -> bool {
        x >= 0 && x < WINDOW_WIDTH && y >= 0 && y < 20
    }

    fn on_mouse_button(&mut self, event: &SurfaceEvent) {
        let pressed = event.pressed != 0;
        let Some(index) = self.window_index(event.surface) else {
            return;
        };
        if pressed {
            // Foco + possível arrasto pela barra de título.
            self.focused = index;
            // Empilha: re-presenta a janela focada (o kernel renova o z).
            if let Some(window) = self.windows[index].as_ref() {
                let _ = window.surface.present();
            }
            let over_title = event.y >= 0 && event.y < 20 && event.x >= 0 && event.x < WINDOW_WIDTH;
            if over_title {
                // Botões de controle (à direita da barra): fechar e minimizar.
                if self.handle_title_buttons(index, event.x, event.y) {
                    return;
                }
                self.dragging = Some((index, event.x, event.y));
                self.drag_moved = false;
            }
            self.redraw(index);
        } else {
            // Fim de arrasto: consolida a posição (a kernel já recebeu os
            // moves intermediários).
            self.dragging = None;
        }
    }

    /// Testa os três botões de controle do canto direito da barra de título.
    /// Devolve `true` quando o clique foi consumido por um botão.
    fn handle_title_buttons(&mut self, index: usize, x: i32, _y: i32) -> bool {
        let close_x = WINDOW_WIDTH - 18;
        let min_x = WINDOW_WIDTH - 34;
        if x >= close_x && x < close_x + 14 {
            // Fechar: a janela sai da lista (a superfície é destruída).
            self.close(index);
            return true;
        }
        if x >= min_x && x < min_x + 14 {
            // Minimizar: esconde a superfície (pode ser reaberta pela barra
            // do desktop no futuro).
            if let Some(window) = self.windows[index].as_ref() {
                let _ = window.surface.hide();
            }
            return true;
        }
        false
    }

    fn close(&mut self, index: usize) {
        if let Some(window) = self.windows[index].take() {
            let _ = surface::destroy(&window.surface);
        }
        // Compacta: move as janelas seguintes para a esquerda.
        let mut write = index;
        for read in (index + 1)..self.window_count {
            self.windows[write] = self.windows[read].take();
            write += 1;
        }
        self.window_count -= 1;
        if self.focused >= self.window_count {
            self.focused = self.window_count.saturating_sub(1);
        }
    }

    fn on_mouse_move(&mut self, event: &SurfaceEvent) {
        let Some((index, grab_x, grab_y)) = self.dragging else {
            return;
        };
        if self.window_index(event.surface) != Some(index) {
            return;
        }
        let new_x = (self.windows[index].as_ref().map_or(0, |w| w.x) + event.x - grab_x).max(0);
        let new_y = (self.windows[index].as_ref().map_or(0, |w| w.y) + event.y - grab_y).max(0);
        self.drag_moved = true;
        if let Some(window) = self.windows[index].as_mut() {
            window.x = new_x;
            window.y = new_y;
            let _ = window.surface.move_to(new_x, new_y);
        }
    }

    fn on_key(&mut self, event: &SurfaceEvent) {
        let key = char::from_u32(event.key).unwrap_or('\0');
        let Some(index) = self.window_index(event.surface) else {
            return;
        };
        // Demonstração mínima de teclado por janela: o terminal ecoa a tecla
        // no título; 'q' fecha a janela focada.
        match key {
            'q' | 'Q' => self.close(index),
            _ => {
                if let Some(window) = self.windows[index].as_mut() {
                    window.set_title(match window.kind {
                        AppKind::Terminal => "Astrid Terminal",
                        AppKind::Monitor => "System Monitor",
                        AppKind::About => "About Liminal",
                    });
                    self.redraw(index);
                }
            }
        }
    }

    /// Redesenha a janela inteira (moldura + conteúdo) e publica o frame.
    fn redraw(&mut self, index: usize) {
        let Some(window) = self.windows[index].as_ref() else {
            return;
        };
        let focused = index == self.focused;
        let theme = self.theme.window;
        let surface = &window.surface;

        // Fundo: a face da janela.
        surface.clear(surface_color(theme.window));
        // Moldura em relevo ao redor de todo o buffer.
        chrome::edge(
            surface,
            &theme,
            Relief::Raised,
            0,
            0,
            WINDOW_WIDTH,
            WINDOW_HEIGHT,
        );
        // Barra de título em relevo invertido (campo "para fora" no 3.1).
        chrome::title_bar(
            surface,
            &theme,
            focused,
            3,
            3,
            WINDOW_WIDTH - 6,
            window.title(),
        );
        // Botões de controle na barra.
        let button_y = 6;
        let button_size = 14;
        chrome::button(
            surface,
            &theme,
            WINDOW_WIDTH - 18,
            button_y,
            button_size,
            Glyph::Close,
        );
        chrome::button(
            surface,
            &theme,
            WINDOW_WIDTH - 34,
            button_y,
            button_size,
            Glyph::Minimize,
        );
        // Conteúdo por aplicação.
        let client = window.kind;
        let client_surface = &window.surface;
        match client {
            AppKind::Terminal => draw_terminal(client_surface, &theme),
            AppKind::Monitor => draw_monitor(client_surface, &theme, focused),
            AppKind::About => draw_about(client_surface, &theme),
        }
        let _ = surface.flush();
    }

    /// Faixa do desktop no rodapé com o nome do tema (do themeConfig.json).
    fn draw_desktop(&mut self) {
        let Some(desktop) = self.desktop.as_ref() else {
            return;
        };
        let theme = self.theme.window;
        desktop.clear(surface_color(theme.desktop));
        // Faixa inferior com relevo invertido.
        let bar_y = DESKTOP_HEIGHT - DESKTOP_BAR_HEIGHT;
        desktop.fill(0, bar_y, DESKTOP_WIDTH, DESKTOP_BAR_HEIGHT, surface_color(theme.window));
        chrome::edge(
            desktop,
            &theme,
            Relief::Sunken,
            0,
            bar_y,
            DESKTOP_WIDTH,
            DESKTOP_BAR_HEIGHT,
        );
        // Nome do tema à esquerda da faixa.
        let name = core::str::from_utf8(&self.theme_name[..self.theme_name_len]).unwrap_or("");
        let mut text = [0u8; 64];
        let prefix = b"Liminal WM \xe2\x80\x94 ";
        let mut length = 0;
        for &byte in prefix {
            text[length] = byte;
            length += 1;
        }
        for &byte in name.as_bytes() {
            if length < text.len() {
                text[length] = byte;
                length += 1;
            }
        }
        if let Ok(label) = core::str::from_utf8(&text[..length]) {
            desktop.text(
                6,
                bar_y + (DESKTOP_BAR_HEIGHT as usize - 8) as i32 / 2,
                label,
                surface_color(theme.title_text),
            );
        }
        let _ = desktop.flush();
    }
}

/// Conteúdos de demonstração (clientes embutidos no WM até existir IPC).
fn draw_terminal(surface: &Surface, theme: &crate::theme::WindowTheme) {
    let ink = surface_color(theme.title_text_ink());
    let accent = surface_color(theme.title_active);
    surface.fill(8, 26, WINDOW_WIDTH - 18, WINDOW_HEIGHT - 36, surface_color(theme.desktop));
    chrome::edge(
        surface,
        theme,
        Relief::Sunken,
        8,
        26,
        WINDOW_WIDTH - 18,
        WINDOW_HEIGHT - 36,
    );
    surface.text(16, 34, "ImagineOS Liminal", accent);
    surface.text(16, 50, "Display server: liminal-server", ink);
    surface.text(16, 64, "Window manager: liminal-client", ink);
    surface.text(16, 78, "Arraste a barra de titulo", ink);
    surface.text(16, 92, "q fecha esta janela", ink);
}

fn draw_monitor(surface: &Surface, theme: &crate::theme::WindowTheme, focused: bool) {
    let ink = surface_color(theme.title_text_ink());
    surface.text(12, 30, "WM de pesquisa", ink);
    surface.text(12, 46, "Superficies compostas pelo servidor", ink);
    surface.text(12, 60, if focused { "Foco: esta janela" } else { "Foco: outra janela" }, ink);
    // "Gauge" com relevo.
    chrome::edge(surface, theme, Relief::Sunken, 12, 80, 120, 16);
    surface.fill(14, 82, 70, 12, surface_color(theme.title_active));
    surface.text(12, 110, "Theme: .tpf carregado do disco", ink);
}

fn draw_about(surface: &Surface, theme: &crate::theme::WindowTheme) {
    let ink = surface_color(theme.title_text_ink());
    surface.text(12, 40, "Liminal Client", ink);
    surface.text(12, 56, "Window Manager + Compositor", ink);
}

/// As superfícies compartilham o formato RGB do framebuffer (8:8:8).
fn surface_color(rgb: u32) -> u32 {
    rgb
}

// Evita warnings dos itens usados só em algumas janelas.
#[allow(dead_code)]
fn _unused() {
    let _ = console::write_all(b"");
}
