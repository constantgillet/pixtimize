//! Build script: locate the native libvips (and its GLib deps) via pkg-config.
//!
//! The `libvips-rs` crate only emits `-l` link flags, not the library search
//! paths. On distributions where libvips lives in a standard location (e.g.
//! Debian's `/usr/lib`) linking already works, but on Homebrew or any custom
//! prefix the linker needs the `-L` paths that pkg-config provides. Probing
//! here keeps the build portable across Linux, macOS, and CI.

fn main() {
    for lib in ["vips", "glib-2.0", "gobject-2.0"] {
        if let Err(err) = pkg_config::Config::new().probe(lib) {
            println!("cargo:warning=pkg-config could not locate {lib}: {err}");
        }
    }
}
