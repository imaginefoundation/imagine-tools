//! liminalServer — o display server do ImagineOS.
//!
//! É a metade "servidor" da stack Liminal: o único processo que mapeia o
//! framebuffer e desenha na tela. Seu papel é estreito:
//!
//! 1. mapeia o framebuffer e assume o console do kernel (`release_console`);
//! 2. a cada quadro, consulta o kernel pelas superfícies visíveis
//!    (`surfaces_snapshot`) e copia cada buffer para o backbuffer na posição
//!    `(x, y)` que o dono escolheu — quem decide o layout é o WM
//!    (liminalClient), não o servidor;
//! 3. desenha o cursor do ponteiro;
//! 4. roteia entrada: eventos de teclado/mouse vão para a superfície sob o
//!    ponteiro (coordenadas locais), na ordem de empilhamento.
//!
//! Toda a política de janelas (tema, títulos, arrasto, foco) vive no
//! liminalClient — o window manager/compositor — que publica as janelas dele
//! como superfícies comuns. O servidor não sabe o que é "janela": só
//! retângulos.
//!
//! Uso: `/bin/liminal-server` — sobe pelo init quando `/etc/display.on` existe.

#![no_std]
#![no_main]

use core::panic::PanicInfo;
use imagineos::abi::{
    SurfaceEvent, SurfaceSnapshot, INPUT_KEY, INPUT_MOUSE_BUTTON, INPUT_MOUSE_MOVE, MAX_SURFACES,
    SURFACE_EVENT_KEY, SURFACE_EVENT_MOUSE_BUTTON, SURFACE_EVENT_MOUSE_MOVE,
};
use imagineos::display::{self, Framebuffer};
use imagineos::process;

const BACKBUFFER_MAX_WIDTH: usize = 800;
const BACKBUFFER_MAX_HEIGHT: usize = 600;
const SCREEN_PIXELS: usize = BACKBUFFER_MAX_WIDTH * BACKBUFFER_MAX_HEIGHT;

static mut BACKBUFFER: [u32; SCREEN_PIXELS] = [0; SCREEN_PIXELS];

/// Snapshot de superfície com o endereço convertido para uso local.
struct Entry {
    id: u32,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    pitch: u32,
    pixels: *const u32,
    z: u32,
}

struct DisplayServer {
    framebuffer: Framebuffer,
    entries: [Option<Entry>; MAX_SURFACES],
    entry_count: usize,
    pointer_x: i32,
    pointer_y: i32,
    cursor_saved: [u32; 16 * 16],
    cursor_rect: (i32, i32, i32, i32),
    cursor_present: bool,
}

impl DisplayServer {
    fn new(framebuffer: Framebuffer) -> Self {
        Self {
            framebuffer,
            entries: core::array::from_fn(|_| None),
            entry_count: 0,
            pointer_x: 32,
            pointer_y: 32,
            cursor_saved: [0; 16 * 16],
            cursor_rect: (0, 0, 0, 0),
            cursor_present: false,
        }
    }

    /// Sincroniza com o snapshot do kernel: renova posições, remove as
    /// desaparecidas e adiciona as novas. Devolve `true` quando algo mudou.
    fn sync(&mut self) -> bool {
        let mut snapshots = [SurfaceSnapshot::default(); MAX_SURFACES];
        let count = display::surfaces_snapshot(&mut snapshots);
        let mut changed = false;

        for index in 0..self.entry_count {
            let Some(entry) = self.entries[index].take() else {
                continue;
            };
            match snapshots[..count].iter().find(|s| s.id == entry.id) {
                Some(snapshot) => {
                    let moved = snapshot.x != entry.x || snapshot.y != entry.y;
                    self.entries[index] = Some(Entry {
                        id: entry.id,
                        x: snapshot.x,
                        y: snapshot.y,
                        width: snapshot.width as i32,
                        height: snapshot.height as i32,
                        pitch: snapshot.pitch,
                        pixels: snapshot.address as *const u32,
                        z: snapshot.z,
                    });
                    if moved {
                        changed = true;
                    }
                }
                None => changed = true,
            }
        }
        let mut write = 0;
        for read in 0..self.entry_count {
            if let Some(entry) = self.entries[read].take() {
                self.entries[write] = Some(entry);
                write += 1;
            }
        }
        self.entry_count = write;
        for snapshot in &snapshots[..count] {
            if self.entries[..self.entry_count]
                .iter()
                .flatten()
                .any(|entry| entry.id == snapshot.id)
            {
                continue;
            }
            if self.entry_count >= MAX_SURFACES {
                break;
            }
            self.entries[self.entry_count] = Some(Entry {
                id: snapshot.id,
                x: snapshot.x,
                y: snapshot.y,
                width: snapshot.width as i32,
                height: snapshot.height as i32,
                pitch: snapshot.pitch,
                pixels: snapshot.address as *const u32,
                z: snapshot.z,
            });
            self.entry_count += 1;
            changed = true;
        }
        changed
    }

    /// Superfície de maior z sob o ponto (ordem de empilhamento do WM).
    fn at(&self, x: i32, y: i32) -> Option<u32> {
        let mut best: Option<&Entry> = None;
        for entry in self.entries[..self.entry_count].iter().flatten() {
            if x >= entry.x
                && x < entry.x + entry.width
                && y >= entry.y
                && y < entry.y + entry.height
                && best.map_or(true, |b| entry.z >= b.z)
            {
                best = Some(entry);
            }
        }
        best.map(|entry| entry.id)
    }

    fn origin(&self, id: u32) -> (i32, i32) {
        self.entries[..self.entry_count]
            .iter()
            .flatten()
            .find(|entry| entry.id == id)
            .map_or((0, 0), |entry| (entry.x, entry.y))
    }

    fn compose(&mut self) {
        let width = self.framebuffer.width() as i32;
        let height = self.framebuffer.height() as i32;
        let pitch = BACKBUFFER_MAX_WIDTH;
        let backbuffer = core::ptr::addr_of_mut!(BACKBUFFER).cast::<u32>();

        // Fundo: preto sólido (o WM cobre o desktop com a superfície dele).
        // A área útil é o menor dos dois: o backbuffer estático tem teto
        // fixo, e o modo real do firmware (OVMF costuma dar 1024x768) pode
        // ser maior — compondo além do array seria um page fault.
        let usable_width = width.min(BACKBUFFER_MAX_WIDTH as i32) as usize;
        let usable_height = height.min(BACKBUFFER_MAX_HEIGHT as i32) as usize;
        for row in 0..usable_height {
            let line = row * pitch;
            for column in 0..usable_width {
                unsafe { backbuffer.add(line + column).write(0) };
            }
        }
        // Superfícies em ordem de z (inserção, no máximo 8).
        let mut order = [0usize; MAX_SURFACES];
        for index in 0..self.entry_count {
            order[index] = index;
        }
        for index in 1..self.entry_count {
            let mut cursor = index;
            while cursor > 0 {
                let left = self.entries[order[cursor - 1]].as_ref().map_or(0, |e| e.z);
                let right = self.entries[order[cursor]].as_ref().map_or(0, |e| e.z);
                if left <= right {
                    break;
                }
                order.swap(cursor - 1, cursor);
                cursor -= 1;
            }
        }
        for &index in &order[..self.entry_count] {
            let Some(entry) = self.entries[index].as_ref() else {
                continue;
            };
            let columns = (entry.width.min(width).max(0)) as usize;
            let rows = (entry.height.min(height).max(0)) as usize;
            // Clamps de segurança: nada pode sair do backbuffer.
            let origin_x = entry.x.max(0) as usize;
            let origin_y = entry.y.max(0) as usize;
            let columns = columns.min(BACKBUFFER_MAX_WIDTH.saturating_sub(origin_x));
            let rows = rows.min(BACKBUFFER_MAX_HEIGHT.saturating_sub(origin_y));
            for row in 0..rows {
                let source = row * entry.pitch as usize;
                let destination =
                    (entry.y.max(0) as usize + row) * pitch + entry.x.max(0) as usize;
                for column in 0..columns {
                    let pixel = unsafe { entry.pixels.add(source + column).read() };
                    unsafe { backbuffer.add(destination + column).write(pixel) };
                }
            }
        }
        self.draw_cursor();
        // Presenta o quadro inteiro. O custo de recomposição fica no WM, que
        // é quem redesenha com menos frequência.
        for row in 0..usable_height {
            let line = unsafe {
                core::slice::from_raw_parts(backbuffer.add(row * pitch), usable_width)
            };
            self.framebuffer.copy_row(line, 0, row);
        }
        self.cursor_present = true;
    }

    /// Move apenas o cursor: restaura o fundo salvo e redesenha na nova
    /// posição. Caminho quente entre composições.
    fn move_cursor(&mut self) {
        if !self.cursor_present {
            self.compose();
            return;
        }
        let (old_x, old_y, old_w, old_h) = self.cursor_rect;
        let pitch = BACKBUFFER_MAX_WIDTH;
        let backbuffer = core::ptr::addr_of_mut!(BACKBUFFER).cast::<u32>();
        if old_w > 0 && old_h > 0 {
            for row in 0..old_h as usize {
                for column in 0..old_w as usize {
                    let value = self.cursor_saved[row * 16 + column];
                    unsafe {
                        backbuffer
                            .add((old_y as usize + row) * pitch + old_x as usize + column)
                            .write(value)
                    };
                }
            }
        }
        self.draw_cursor();
        let left = old_x.min(self.pointer_x).max(0);
        let top = old_y.min(self.pointer_y).max(0);
        let right = (old_x + old_w)
            .max(self.pointer_x + 12)
            .min(self.framebuffer.width() as i32);
        let bottom = (old_y + old_h)
            .max(self.pointer_y + 16)
            .min(self.framebuffer.height() as i32);
        if left < right && top < bottom {
            for row in top..bottom {
                let index = row as usize * pitch + left as usize;
                let count = (right - left) as usize;
                let line = unsafe { core::slice::from_raw_parts(backbuffer.add(index), count) };
                self.framebuffer.copy_row(line, left as usize, row as usize);
            }
        }
    }

    /// Desenha a seta do cursor no backbuffer e salva o fundo por baixo.
    fn draw_cursor(&mut self) {
        let pitch = BACKBUFFER_MAX_WIDTH;
        let backbuffer = core::ptr::addr_of_mut!(BACKBUFFER).cast::<u32>();
        let screen_w = self.framebuffer.width() as i32;
        let screen_h = self.framebuffer.height() as i32;
        let left = self.pointer_x.max(0);
        let top = self.pointer_y.max(0);
        let right = (self.pointer_x + 12).min(screen_w);
        let bottom = (self.pointer_y + 16).min(screen_h);
        if left < right && top < bottom {
            for row in top..bottom {
                for column in left..right {
                    let index = row as usize * pitch + column as usize;
                    let value = unsafe { backbuffer.add(index).read() };
                    self.cursor_saved[(row - top) as usize * 16 + (column - left) as usize] =
                        value;
                }
            }
        }
        self.cursor_rect = (left, top, right - left, bottom - top);
        // Seta clássica: contorno preto, corpo branco (desenhada com
        // primitivas, sem bitmap — mesma escola do relevo do WM).
        for row in 0..12 {
            let line_width = (row / 2 + 1).min(12 - row / 2).min(12);
            for column in 0..line_width {
                self.pixel(self.pointer_x + column, self.pointer_y + row, 0x000000);
            }
        }
        for row in 1..10 {
            let line_width = (row / 2).max(1);
            for column in 0..line_width {
                self.pixel(self.pointer_x + column, self.pointer_y + row, 0xffffff);
            }
        }
    }

    fn pixel(&self, x: i32, y: i32, rgb: u32) {
        // Tela E backbuffer: vale o menor dos dois (o array estático tem
        // teto fixo, o modo real do firmware pode ser maior).
        if x < 0
            || y < 0
            || x >= (self.framebuffer.width() as i32).min(BACKBUFFER_MAX_WIDTH as i32)
            || y >= (self.framebuffer.height() as i32).min(BACKBUFFER_MAX_HEIGHT as i32)
        {
            return;
        }
        let pitch = BACKBUFFER_MAX_WIDTH;
        let backbuffer = core::ptr::addr_of_mut!(BACKBUFFER).cast::<u32>();
        let index = y as usize * pitch + x as usize;
        unsafe { backbuffer.add(index).write(self.framebuffer.color(rgb)) };
    }

    fn poll_events(&mut self) {
        for _ in 0..64 {
            let event = match display::poll_input() {
                Ok(Some(event)) => event,
                Ok(None) | Err(_) => break,
            };
            match event.kind {
                INPUT_MOUSE_MOVE => {
                    let moved = self.pointer_x != event.x || self.pointer_y != event.y;
                    self.pointer_x = event.x;
                    self.pointer_y = event.y;
                    if moved {
                        self.move_cursor();
                    }
                    if let Some(surface) = self.at(self.pointer_x, self.pointer_y) {
                        let (origin_x, origin_y) = self.origin(surface);
                        display::send_surface_event(
                            surface,
                            SurfaceEvent {
                                kind: SURFACE_EVENT_MOUSE_MOVE,
                                x: self.pointer_x - origin_x,
                                y: self.pointer_y - origin_y,
                                ..Default::default()
                            },
                        );
                    }
                }
                INPUT_MOUSE_BUTTON => {
                    if let Some(surface) = self.at(self.pointer_x, self.pointer_y) {
                        let (origin_x, origin_y) = self.origin(surface);
                        display::send_surface_event(
                            surface,
                            SurfaceEvent {
                                kind: SURFACE_EVENT_MOUSE_BUTTON,
                                x: self.pointer_x - origin_x,
                                y: self.pointer_y - origin_y,
                                button: event.button,
                                pressed: event.pressed,
                                ..Default::default()
                            },
                        );
                    }
                }
                INPUT_KEY => {
                    let key = char::from_u32(event.key).unwrap_or('\0');
                    if key == 'q' || key == 'Q' || key == '\u{f0007}' {
                        // Sair do modo gráfico: devolve a tela ao console.
                        process::exit(0);
                    }
                    // Teclas vão para a superfície com maior z (a focada).
                    if let Some(surface) = self.focused() {
                        display::send_surface_event(
                            surface,
                            SurfaceEvent {
                                kind: SURFACE_EVENT_KEY,
                                key: event.key,
                                ..Default::default()
                            },
                        );
                    }
                }
                _ => {}
            }
        }
    }

    fn focused(&self) -> Option<u32> {
        self.entries[..self.entry_count]
            .iter()
            .flatten()
            .max_by_key(|entry| entry.z)
            .map(|entry| entry.id)
    }

    fn run(&mut self) -> ! {
        self.compose();
        loop {
            self.sync();
            self.poll_events();
            if process::yield_now().is_err() {
                process::exit(1);
            }
        }
    }
}

#[unsafe(no_mangle)]
extern "C" fn _start(
    _argc: usize,
    _argv: *const *const u8,
    _envc: usize,
    _envp: *const *const u8,
) -> ! {
    let framebuffer = match display::map_framebuffer() {
        Ok(framebuffer) => framebuffer,
        Err(_) => {
            let _ = imagineos::console::write_all(b"liminal-server: sem framebuffer\n");
            process::exit(1);
        }
    };
    if display::release_console().is_err() {
        let _ = imagineos::console::write_all(b"liminal-server: falha ao assumir a tela\n");
        process::exit(1);
    }
    let mut server = DisplayServer::new(framebuffer);
    server.run()
}

#[panic_handler]
fn panic(_info: &PanicInfo<'_>) -> ! {
    // Devolve a tela ao console antes de morrer, senão o sistema fica sem
    // saída visível depois de um erro do servidor.
    let _ = imagineos::console::write_all(b"liminal-server: panic\n");
    process::exit(101)
}
