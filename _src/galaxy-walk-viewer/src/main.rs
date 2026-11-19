#![windows_subsystem = "windows"]

mod app;
mod bridge;
mod camera;
mod export;
mod render;
mod scene;
mod settings;
mod shaders;
mod sun;
mod ui;

use eframe::egui;

fn main() -> eframe::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let mut screenshot: Option<std::path::PathBuf> = None;
    let mut open: Option<std::path::PathBuf> = None;
    let mut bridge: Option<std::path::PathBuf> = None;
    let mut i = 1;
    while i < args.len() {
        if args[i] == "--screenshot" && i + 1 < args.len() {
            screenshot = Some(args[i + 1].clone().into());
            i += 1;
        } else if args[i] == "--bridge" && i + 1 < args.len() {
            bridge = Some(args[i + 1].clone().into());
            i += 1;
        } else if !args[i].starts_with("--") {
            open = Some(args[i].clone().into());
        }
        i += 1;
    }

    let mut viewport = egui::ViewportBuilder::default()
        .with_title("Galaxy Walk")
        .with_inner_size([1440.0, 900.0])
        .with_min_inner_size([900.0, 600.0]);
    if screenshot.is_some() {
        viewport = viewport.with_active(false);
        #[cfg(target_os = "windows")]
        {
            viewport = viewport.with_window_level(egui::WindowLevel::AlwaysOnBottom);
        }
    }
    #[allow(unused_mut)]
    let mut options = eframe::NativeOptions {
        viewport,
        renderer: eframe::Renderer::Glow,
        depth_buffer: 24,
        multisampling: 4,
        vsync: true,
        ..Default::default()
    };
    #[cfg(target_os = "macos")]
    if screenshot.is_some() {
        options.event_loop_builder = Some(Box::new(|b| {
            use winit::platform::macos::EventLoopBuilderExtMacOS;
            b.with_activate_ignoring_other_apps(false);
        }));
    }
    eframe::run_native(
        "Galaxy Walk",
        options,
        Box::new(move |cc| Ok(Box::new(app::App::new(cc, screenshot, open, bridge)))),
    )
}
