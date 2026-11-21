//! Chat markdown renderer exposed to QML as the `markdown` context object.
//!
//! Kept apart from `AppBridge`: that one lives in a `RefCell` that the worker
//! poll path borrows mutably, and rendering is called from delegate bindings
//! at any time.

use std::cell::RefCell;

use opencode_markdown::Renderer;
use qmetaobject::*;

#[derive(QObject, Default)]
pub struct MarkdownBridge {
    base: qt_base_class!(trait QObject),
    renderer: RefCell<Renderer>,

    /// Segments of a chat body as JSON `{rev, keep, segs}` (see
    /// `opencode_markdown::Update::to_json`). `prev_rev` is the `rev` the
    /// delegate shows, 0 for none; QML numbers are doubles, exact for any rev
    /// a session reaches.
    render: qt_method!(
        fn render(&self, body: QString, truncated: bool, live: bool, prev_rev: f64) -> QString {
            let mut r = self.renderer.borrow_mut();
            let u = r.render(&body.to_string(), truncated, live, prev_rev as u64);
            // What streaming costs on the device: RUST_LOG=debug.
            log::debug!("markdown: rev {} keeps {} segments, sends {}; {:?}", u.rev, u.keep, u.segs.len(), r.stats());
            u.to_json().into()
        }
    ),
}
