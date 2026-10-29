//! Single-instance lock: bind a fixed localhost TCP port; a second launch
//! connects and sends `focus`, then exits. First instance accepts and raises
//! its window.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::OnceLock;

pub const PORT: u16 = 59590;
/// The one window handle we own; set at open_window time.
pub(crate) static WINDOW: OnceLock<gpui_kit::AnyWindowHandle> = OnceLock::new();

pub fn remember_window(handle: gpui_kit::AnyWindowHandle) {
    let _ = WINDOW.set(handle);
}

/// Bind → we are the first instance (listener kept alive by the caller).
/// Bind failure → another instance is running: poke it and tell main to exit.
pub fn acquire() -> Result<TcpListener, ()> {
    match TcpListener::bind(("127.0.0.1", PORT)) {
        Ok(l) => Ok(l),
        Err(_) => {
            // Best-effort focus relay; failure just means the other instance
            // is shutting down — exit anyway, single instance is the contract.
            if let Ok(mut s) = std::net::TcpStream::connect(("127.0.0.1", PORT)) {
                let _ = s.write_all(b"focus");
                let _ = s.flush();
                let _ = s.read(&mut [0u8; 1]); // wait for peer close
            }
            Err(())
        }
    }
}

/// First-instance side: accept focus pokes forever and raise the window.
/// Called inside the GPUI run closure; smol converts the std listener.
pub fn listen(listener: TcpListener, cx: &mut gpui_kit::App) {
    let listener = smol::net::TcpListener::from(smol::Async::new(listener).expect("listener"));
    cx.spawn(async move |cx| loop {
        let Ok((mut stream, _)) = listener.accept().await else {
            break;
        };
        let mut buf = [0u8; 16];
        use smol::io::AsyncReadExt as _;
        let _ = stream.read(&mut buf).await;
        let _ = cx.update(|cx| {
            cx.activate(true);
            if let Some(h) = WINDOW.get() {
                let _ = h.update(cx, |_, window, _| window.activate_window());
            }
        });
    })
    .detach();
}
