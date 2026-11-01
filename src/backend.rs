//! Compile-time renderer selection. Always set explicitly: with both features
//! compiled, eframe would default to wgpu.

#[cfg(not(any(feature = "glow", feature = "wgpu")))]
compile_error!("enable feature `glow` or `wgpu`");

pub const BACKEND: &str = if cfg!(feature = "glow") { "glow" } else { "wgpu" };

pub fn renderer() -> eframe::Renderer {
    #[cfg(feature = "glow")]
    {
        eframe::Renderer::Glow
    }
    #[cfg(all(not(feature = "glow"), feature = "wgpu"))]
    {
        eframe::Renderer::Wgpu
    }
}
