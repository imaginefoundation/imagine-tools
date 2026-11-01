//! liminalClient — o window manager / compositor da stack Liminal.
//!
//! É a metade "cliente" do display server (liminalServer). Não toca no
//! framebuffer: cada janela é uma superfície do kernel (`Surface`), cujo
//! buffer é composto pelo servidor na posição que o WM escolhe via
//! [`Surface::move_to`].
//!
//! Responsabilidades aqui:
//!
//! 1. carregar o tema `.tpf` (`system/themes/current.tpf`) — o formato
//!    empacota `themeConfig.json` (nome, autor, versão, descrição e nome do
//!    compositor) e o CSS `window/windowTheme.css`;
//! 2. desenhar as molduras no estilo Windows 3.1: relevo de quatro tons
//!    (`#light`/`#highlight` em cima e à esquerda, `#dark`/`#shadow` embaixo
//!    e à direita), barra de título navy com texto branco e botões de face
//!    `silver` (`chrome.rs`);
//! 3. gerenciar as janelas: criar, focar, arrastar pela barra de título e
//!    empilhar (o z de cada superfície é atualizado via `present`);
//! 4. desenhar o desktop (faixa inferior com o nome do tema).
//!
//! Uso: `/bin/liminal-client` — o init sobe junto com o liminal-server.
#![no_std]
#![no_main]

mod chrome;
mod theme;
mod tpf;
mod wm;

use core::panic::PanicInfo;
use imagineos::process;

#[unsafe(no_mangle)]
extern "C" fn _start(
    _argc: usize,
    _argv: *const *const u8,
    _envc: usize,
    _envp: *const *const u8,
) -> ! {
    let mut wm = match wm::WindowManager::boot() {
        Ok(wm) => wm,
        Err(_) => {
            // Sem servidor (ou sem superfícies livres): sai em silêncio. O
            // init volta ao getty de texto.
            process::exit(1);
        }
    };
    wm.run()
}

#[panic_handler]
fn panic(_info: &PanicInfo<'_>) -> ! {
    let _ = imagineos::console::write_all(b"liminal-client: panic\n");
    process::exit(101)
}
