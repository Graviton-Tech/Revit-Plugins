//! User interface: theme, menu, toolbar, mode panels, settings windows,
//! material editor and the overlays drawn on top of the 3D view.

use crate::app::{App, Issue, Mode, ACCENT};
use crate::camera::{CameraPose, SavedView, VideoPath};
use crate::scene::{self, BumpType, GroupKind, MaterialType, TransparencyType};
use crate::settings::{Bitrate, CaptureResolution, Horizon, Preset, Projection, Quality, RenderingStyle, VisualSettings};
use eframe::egui::{self, Align2, Color32, FontId, Pos2, Rect, RichText, Sense, Stroke, Ui, Vec2};
use glam::Vec3;
use std::ops::RangeInclusive;

pub const BG_DARK: Color32 = Color32::from_rgb(0x1A, 0x1A, 0x1A);
pub const BG_DARKER: Color32 = Color32::from_rgb(0x11, 0x11, 0x11);
pub const PANEL: Color32 = Color32::from_rgb(0x24, 0x24, 0x24);
pub const TEXT: Color32 = Color32::from_rgb(0xF0, 0xF0, 0xF0);
pub const TEXT_DIM: Color32 = Color32::from_rgb(0xB6, 0xB6, 0xB6);

/// Places used by the sun / location dialog (latitude, longitude, GMT offset).
const PLACES: &[(&str, f32, f32, f32)] = &[
    ("Cologne", 50.94, 6.96, 1.0),
    ("London", 51.51, -0.13, 0.0),
    ("New York", 40.71, -74.01, -5.0),
    ("Los Angeles", 34.05, -118.24, -8.0),
    ("Dubai", 25.20, 55.27, 4.0),
    ("Cairo", 30.04, 31.24, 2.0),
    ("Singapore", 1.35, 103.82, 8.0),
    ("Tokyo", 35.68, 139.69, 9.0),
    ("Sydney", -33.87, 151.21, 10.0),
    ("Sao Paulo", -23.55, -46.63, -3.0),
];

/// Reference albedo colours offered in the material editor (estimated sRGB values).
const ALBEDO_REFS: &[(&str, [f32; 3])] = &[
    ("Coal", [0.04, 0.04, 0.04]),
    ("New concrete", [0.55, 0.55, 0.53]),
    ("Grass", [0.21, 0.32, 0.09]),
    ("White paint", [0.85, 0.85, 0.84]),
    ("Fresh snow", [0.93, 0.93, 0.95]),
    ("Desert sand", [0.70, 0.58, 0.42]),
];

pub fn style(ctx: &egui::Context) {
    let mut v = egui::Visuals::dark();
    v.panel_fill = BG_DARK;
    v.window_fill = PANEL;
    v.extreme_bg_color = BG_DARKER;
    v.faint_bg_color = Color32::from_rgb(0x2A, 0x2A, 0x2A);
    v.selection.bg_fill = ACCENT;
    v.selection.stroke = Stroke::new(1.0, Color32::WHITE);
    v.hyperlink_color = ACCENT;
    v.widgets.hovered.bg_stroke = Stroke::new(1.0, ACCENT);
    v.widgets.active.bg_fill = Color32::from_rgb(0xCC, 0x60, 0x00);
    v.override_text_color = Some(TEXT);
    ctx.set_visuals(v);
    ctx.style_mut(|s| {
        s.spacing.item_spacing = egui::vec2(8.0, 6.0);
        s.spacing.slider_width = 170.0;
        s.spacing.button_padding = egui::vec2(8.0, 4.0);
    });
}

// ------------------------------------------------------------------ small helpers

fn section(ui: &mut Ui, title: &str) {
    ui.add_space(6.0);
    ui.label(RichText::new(title.to_uppercase()).size(11.5).color(ACCENT).strong());
    ui.separator();
}

fn slider(ui: &mut Ui, label: &str, v: &mut f32, r: RangeInclusive<f32>, suffix: &str) -> bool {
    ui.label(label);
    let c = ui.add(egui::Slider::new(v, r).suffix(suffix)).changed();
    ui.end_row();
    c
}

fn check(ui: &mut Ui, label: &str, v: &mut bool) {
    ui.label(label);
    ui.checkbox(v, "");
    ui.end_row();
}

fn grid(ui: &mut Ui, id: &str, add: impl FnOnce(&mut Ui)) {
    egui::Grid::new(id).num_columns(2).spacing([14.0, 6.0]).min_col_width(150.0).show(ui, add);
}

fn tool_button(ui: &mut Ui, icon: &str, text: &str, active: bool, tip: &str) -> bool {
    let rt = RichText::new(format!("{icon}  {text}")).size(13.5).color(if active { Color32::BLACK } else { TEXT });
    let b = egui::Button::new(rt).fill(if active { ACCENT } else { Color32::TRANSPARENT }).min_size(egui::vec2(0.0, 30.0));
    ui.add(b).on_hover_text(tip).clicked()
}

fn pick_file(title: &str, name: &str, exts: &[&str]) -> Option<String> {
    crate::export::dialog("").set_title(title).add_filter(name, exts).pick_file().map(|p| p.display().to_string())
}

// ------------------------------------------------------------------ menu, toolbar, status bar

pub fn menu_bar(app: &mut App, ctx: &egui::Context) {
    egui::TopBottomPanel::top("menu").frame(egui::Frame::new().fill(BG_DARKER).inner_margin(egui::Margin::symmetric(8, 3))).show(ctx, |ui| {
        egui::menu::bar(ui, |ui| {
            ui.menu_button("File", |ui| {
                if ui.button("New project").clicked() {
                    app.load_defaults();
                    ui.close_menu();
                }
                if ui.button("Open project...").clicked() {
                    if let Some(p) = crate::export::dialog(&app.vs.default_folder).add_filter("Galaxy Walk project", &["gwproj"]).pick_file() {
                        app.load_project(&p);
                    }
                    ui.close_menu();
                }
                if ui.button("Save project").clicked() {
                    app.save_project(ctx, false);
                    ui.close_menu();
                }
                if ui.button("Save project as...").clicked() {
                    app.save_project(ctx, true);
                    ui.close_menu();
                }
                ui.separator();
                if ui.button("Import model (OBJ)...").clicked() {
                    app.import_model(ctx);
                    ui.close_menu();
                }
                ui.checkbox(&mut app.live_updates, "Live updates (reload changed models)");
                ui.separator();
                if ui.button("📂 Open save folder").clicked() {
                    crate::export::reveal(&crate::export::output_dir(&app.vs.default_folder));
                    ui.close_menu();
                }
                if let Some(p) = app.last_saved.clone() {
                    if ui.button(format!("Show last saved file ({})", p.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default())).clicked() {
                        crate::export::reveal(&p);
                        ui.close_menu();
                    }
                }
                ui.separator();
                if ui.button("Exit").clicked() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            });
            ui.menu_button("Capture", |ui| {
                if ui.button("Render image          Shift+F11").clicked() {
                    app.render_image(ctx, None);
                    ui.close_menu();
                }
                if ui.button("Batch rendering...").clicked() {
                    app.show_batch = true;
                    ui.close_menu();
                }
                if ui.button("Render panorama (mono)").clicked() {
                    let pose = app.camera.pose;
                    app.render_panorama(ctx, false, pose, None);
                    ui.close_menu();
                }
                if ui.button("Render panorama (stereo)").clicked() {
                    let pose = app.camera.pose;
                    app.render_panorama(ctx, true, pose, None);
                    ui.close_menu();
                }
                if ui.button("Export video frames...").clicked() {
                    app.start_video_export(ctx);
                    ui.close_menu();
                }
            });
            ui.menu_button("View", |ui| {
                for (i, n) in ["Front", "Right", "Back", "Left", "Top"].iter().enumerate() {
                    if ui.button(format!("{n} view      {}", i + 1)).clicked() {
                        app.standard_view(i);
                        ui.close_menu();
                    }
                }
                ui.separator();
                ui.checkbox(&mut app.gs.show_minimap, "Map");
                ui.checkbox(&mut app.gs.show_help, "Help panel   H");
                ui.checkbox(&mut app.vs.show_safe_frame, "Safe frame");
            });
            ui.menu_button("Settings", |ui| {
                if ui.button("Visual settings...").clicked() {
                    app.show_visual = true;
                    ui.close_menu();
                }
                if ui.button("General settings...").clicked() {
                    app.show_general = true;
                    ui.close_menu();
                }
                if ui.button("Sun & location...").clicked() {
                    app.show_sun = true;
                    ui.close_menu();
                }
                if ui.button("Material editor...").clicked() {
                    app.show_material = true;
                    ui.close_menu();
                }
            });
            ui.menu_button("Help", |ui| {
                if ui.button("Controls").clicked() {
                    app.gs.show_help = true;
                    ui.close_menu();
                }
                if ui.button("About").clicked() {
                    app.show_about = true;
                    ui.close_menu();
                }
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let name = app.project_path.as_ref().and_then(|p| p.file_stem()).map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "Untitled project".into());
                ui.label(RichText::new(name).color(TEXT_DIM));
            });
        });
    });
}

pub fn toolbar(app: &mut App, ctx: &egui::Context) {
    egui::TopBottomPanel::top("toolbar").exact_height(42.0).frame(egui::Frame::new().fill(BG_DARK).inner_margin(egui::Margin::symmetric(8, 5))).show(ctx, |ui| {
        ui.horizontal_centered(|ui| {
            // live link / walk-fly
            let walk = app.camera.walk_mode;
            if tool_button(ui, if walk { "🚶" } else { "✈" }, if walk { "Walk" } else { "Fly" }, false, "Toggle walk / fly mode (Space)") {
                app.camera.walk_mode = !walk;
            }
            ui.separator();
            for (m, icon, label, tip) in [
                (Mode::Views, "📌", "Views", "Saved views"),
                (Mode::Video, "🎬", "Video Editor", "Keyframe video path"),
                (Mode::Assets, "🌳", "Asset Library", "Place trees, furniture, people, cars and lights"),
                (Mode::Info, "ℹ", "Object Info", "Click an object to see its details"),
                (Mode::Issues, "⚑", "Issues", "Create and track review issues"),
            ] {
                if tool_button(ui, icon, label, app.mode == m, tip) {
                    app.mode = if app.mode == m { Mode::Walkthrough } else { m };
                }
            }
            ui.separator();
            if tool_button(ui, "🎨", "Materials", app.show_material, "Material editor") {
                app.show_material = !app.show_material;
            }
            if tool_button(ui, "☀", "Sun", app.show_sun, "Sun position, date and location") {
                app.show_sun = !app.show_sun;
            }
            if tool_button(ui, "🗺", "Map", app.gs.show_minimap, "Show the map") {
                app.gs.show_minimap = !app.gs.show_minimap;
            }
            ui.separator();
            if tool_button(ui, "📷", "Screenshot", false, "Render an image (Shift+F11)") {
                app.render_image(ctx, None);
            }
            if tool_button(ui, "🗐", "Batch", app.show_batch, "Batch render saved views") {
                app.show_batch = !app.show_batch;
            }
            if tool_button(ui, "🌐", "Panorama", false, "Render a 360° panorama") {
                let pose = app.camera.pose;
                app.render_panorama(ctx, false, pose, None);
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if tool_button(ui, "⚙", "", app.show_general, "General settings") {
                    app.show_general = !app.show_general;
                }
                if tool_button(ui, "🎚", "Visual Settings", app.show_visual, "Visual settings and presets") {
                    app.show_visual = !app.show_visual;
                }
                let names: Vec<String> = app.presets.iter().map(|p| p.name.clone()).collect();
                let mut sel = app.active_preset;
                egui::ComboBox::from_id_salt("tb_preset").width(140.0).selected_text(names.get(sel).cloned().unwrap_or_default()).show_ui(ui, |ui| {
                    for (i, n) in names.iter().enumerate() {
                        ui.selectable_value(&mut sel, i, n);
                    }
                });
                if sel != app.active_preset {
                    app.apply_preset(sel);
                }
                ui.label(RichText::new("Preset").color(TEXT_DIM));
            });
        });
    });
}

pub fn status_bar(app: &mut App, ctx: &egui::Context) {
    egui::TopBottomPanel::bottom("status").exact_height(26.0).frame(egui::Frame::new().fill(BG_DARKER).inner_margin(egui::Margin::symmetric(10, 4))).show(ctx, |ui| {
        ui.horizontal_centered(|ui| {
            let p = app.camera.pose.pos;
            ui.label(RichText::new(if app.camera.walk_mode { "WALK" } else { "FLY" }).color(ACCENT).strong());
            ui.label(RichText::new(format!("X {:.1}  Y {:.1}  Z {:.1}", p.x, -p.z, p.y)).color(TEXT_DIM));
            ui.separator();
            ui.label(RichText::new(format!("☀ {}  {:02}.{:02}.{}", app.sun.time_string(), app.sun.day, app.sun.month, app.sun.year)).color(TEXT_DIM));
            ui.separator();
            let bridged = app.bridge.is_some();
            if let Some(b) = app.bridge.as_mut() {
                ui.label(RichText::new("Revit").color(ACCENT).strong());
                ui.checkbox(&mut b.follow, "Follow view").on_hover_text("Move to the Revit camera and sun whenever they change");
                ui.checkbox(&mut b.send_back, "Send camera").on_hover_text("Write the walkthrough camera so Revit can match it");
                ui.label(RichText::new(b.status.clone()).color(TEXT_DIM));
            }
            if !bridged {
                ui.label(RichText::new(format!("Quality: {}", app.vs.quality.name())).color(TEXT_DIM));
                ui.label(RichText::new(format!("Preset: {}{}", app.presets.get(app.active_preset).map(|p| p.name.as_str()).unwrap_or(""), if app.preset_modified() { " *" } else { "" })).color(TEXT_DIM));
            }
            if let Some(ve) = &app.video_export {
                ui.separator();
                ui.label(RichText::new(format!("Exporting video frame {} / {}", ve.frame, ve.total)).color(ACCENT));
            } else if !app.exports.is_empty() {
                ui.separator();
                ui.spinner();
                ui.label(RichText::new(format!("Rendering ({} jobs)", app.exports.len())).color(ACCENT));
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let dt = ctx.input(|i| i.stable_dt).max(1e-4);
                let dir = crate::export::output_dir(&app.vs.default_folder);
                let shown = match &app.last_saved {
                    Some(p) => format!("Last saved: {}", p.display()),
                    None => format!("Saves to: {}", dir.display()),
                };
                if ui.small_button("📂 Output").on_hover_text(format!("{shown}\nOpen the folder where images, panoramas, videos and projects are saved")).clicked() {
                    crate::export::reveal(app.last_saved.as_deref().unwrap_or(&dir));
                }
                ui.separator();
                ui.label(RichText::new(format!("{:.0} fps", 1.0 / dt)).color(TEXT_DIM));
                ui.label(RichText::new(format!("{} objects", app.scene.objects.len())).color(TEXT_DIM));
            });
        });
    });
}

// ------------------------------------------------------------------ side panels (modes)

pub fn side_panels(app: &mut App, ctx: &egui::Context) {
    if app.mode == Mode::Walkthrough {
        return;
    }
    let title = match app.mode {
        Mode::Views => "Views",
        Mode::Video => "Video Editor",
        Mode::Assets => "Asset Library",
        Mode::Info => "Object Info",
        Mode::Issues => "Issues",
        Mode::Walkthrough => "",
    };
    egui::SidePanel::left("mode_panel").resizable(true).default_width(310.0).min_width(260.0).frame(egui::Frame::new().fill(PANEL).inner_margin(egui::Margin::same(10))).show(ctx, |ui| {
        ui.horizontal(|ui| {
            ui.heading(RichText::new(title).strong());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("✖").on_hover_text("Close panel (Esc)").clicked() {
                    app.mode = Mode::Walkthrough;
                }
            });
        });
        ui.separator();
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| match app.mode {
            Mode::Views => views_panel(app, ui, ctx),
            Mode::Video => video_panel(app, ui, ctx),
            Mode::Assets => assets_panel(app, ui, ctx),
            Mode::Info => info_panel(app, ui),
            Mode::Issues => issues_panel(app, ui, ctx),
            Mode::Walkthrough => {}
        });
    });
}

fn views_panel(app: &mut App, ui: &mut Ui, ctx: &egui::Context) {
    ui.horizontal(|ui| {
        ui.add(egui::TextEdit::singleline(&mut app.new_view_name).hint_text("New view name").desired_width(170.0));
        if ui.button("➕ Create view").clicked() {
            let name = if app.new_view_name.trim().is_empty() { format!("View {}", app.views.len() + 1) } else { app.new_view_name.trim().to_string() };
            app.create_view(name);
            app.new_view_name.clear();
        }
    });
    ui.horizontal(|ui| {
        if ui.button("⏮ Previous").on_hover_text("Page Up").clicked() {
            app.cycle_view(-1);
        }
        if ui.button("Next ⏭").on_hover_text("Page Down").clicked() {
            app.cycle_view(1);
        }
    });
    ui.label(RichText::new("★ marks favourites; Page Up / Page Down cycle through them.").small().color(TEXT_DIM));
    ui.add_space(4.0);
    let mut jump = None;
    let mut delete = None;
    let mut update = None;
    let mut rename: Option<(usize, String)> = None;
    let presets: Vec<String> = app.presets.iter().map(|p| p.name.clone()).collect();
    for (i, v) in app.views.iter_mut().enumerate() {
        egui::Frame::new().fill(BG_DARK).corner_radius(4.0).inner_margin(egui::Margin::same(6)).show(ui, |ui| {
            ui.horizontal(|ui| {
                // thumbnail stand-in: sky gradient showing the stored time of day
                let (r, _) = ui.allocate_exact_size(egui::vec2(64.0, 40.0), Sense::hover());
                let day = ((v.sun.hour - 6.0) / 12.0 * std::f32::consts::PI).sin().max(0.0);
                let top = lerp_color(Color32::from_rgb(20, 24, 48), Color32::from_rgb(90, 150, 220), day);
                let bot = lerp_color(Color32::from_rgb(60, 40, 60), Color32::from_rgb(200, 220, 235), day);
                ui.painter().rect_filled(Rect::from_min_max(r.min, Pos2::new(r.max.x, r.center().y)), 3.0, top);
                ui.painter().rect_filled(Rect::from_min_max(Pos2::new(r.min.x, r.center().y), r.max), 3.0, bot);
                ui.painter().text(r.center(), Align2::CENTER_CENTER, v.sun.time_string(), FontId::proportional(10.0), Color32::WHITE);
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        let star = if v.favorite { RichText::new("★").color(ACCENT) } else { RichText::new("☆").color(TEXT_DIM) };
                        if ui.add(egui::Button::new(star).frame(false)).on_hover_text("Favourite").clicked() {
                            v.favorite = !v.favorite;
                        }
                        let mut name = v.name.clone();
                        if ui.add(egui::TextEdit::singleline(&mut name).desired_width(150.0)).changed() {
                            rename = Some((i, name));
                        }
                    });
                    ui.horizontal(|ui| {
                        if ui.small_button("Go").clicked() {
                            jump = Some(i);
                        }
                        if ui.small_button("Update").on_hover_text("Store the current camera, sun and field of view").clicked() {
                            update = Some(i);
                        }
                        if ui.small_button("🗑").clicked() {
                            delete = Some(i);
                        }
                        let sel = v.preset.clone().unwrap_or_else(|| "(none)".into());
                        egui::ComboBox::from_id_salt(("vpreset", i)).width(80.0).selected_text(sel).show_ui(ui, |ui| {
                            ui.selectable_value(&mut v.preset, None, "(none)");
                            for p in &presets {
                                ui.selectable_value(&mut v.preset, Some(p.clone()), p);
                            }
                        });
                    });
                });
            });
        });
    }
    if let Some((i, n)) = rename {
        app.views[i].name = n;
    }
    if let Some(i) = jump {
        app.jump_to_view(i, false);
    }
    if let Some(i) = update {
        let preset = app.views[i].preset.clone();
        app.views[i] = SavedView { name: app.views[i].name.clone(), pose: app.camera.pose, sun: app.sun, fov: app.vs.field_of_view, projection: app.vs.projection, favorite: app.views[i].favorite, preset };
        app.notify(ctx, "View updated");
    }
    if let Some(i) = delete {
        app.views.remove(i);
    }
}

fn lerp_color(a: Color32, b: Color32, t: f32) -> Color32 {
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgb(l(a.r(), b.r()), l(a.g(), b.g()), l(a.b(), b.b()))
}

fn video_panel(app: &mut App, ui: &mut Ui, ctx: &egui::Context) {
    ui.horizontal_wrapped(|ui| {
        if ui.button("➕ Add keyframe").on_hover_text("Adds the current camera (+). With a keyframe selected it is inserted after it.").clicked() {
            app.video.add(app.camera.pose, app.selected_keyframe);
        }
        if app.playing.is_some() {
            if ui.button("⏹ Stop").clicked() {
                app.playing = None;
            }
        } else if ui.button("▶ Preview").clicked() && app.video.keyframes.len() >= 2 {
            app.playing = Some(0.0);
            app.camera.walk_mode = false;
        }
        if ui.button("🎞 Export video").clicked() {
            app.start_video_export(ctx);
        }
    });
    ui.horizontal(|ui| {
        if ui.button("Load path...").clicked() {
            if let Some(p) = crate::export::dialog(&app.vs.default_folder).add_filter("Video path", &["json"]).pick_file() {
                match std::fs::read_to_string(&p).ok().and_then(|t| serde_json::from_str::<VideoPath>(&t).ok()) {
                    Some(v) => app.video = v,
                    None => app.notify(ctx, "That file is not a video path."),
                }
            }
        }
        if ui.button("Save path...").clicked() {
            if let Some(p) = crate::export::dialog(&app.vs.default_folder).add_filter("Video path", &["json"]).set_file_name("video_path.json").save_file() {
                let _ = std::fs::write(p, serde_json::to_string_pretty(&app.video).unwrap_or_default());
            }
        }
        if ui.button("Clear").clicked() {
            app.video.keyframes.clear();
            app.selected_keyframe = None;
        }
    });

    // timeline
    let dur = app.video.duration().max(0.001);
    let (r, resp) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 46.0), Sense::click());
    let p = ui.painter_at(r);
    p.rect_filled(r, 4.0, BG_DARKER);
    for s in 0..=(dur.ceil() as i32) {
        let x = r.left() + 8.0 + (r.width() - 16.0) * (s as f32 / dur);
        if x > r.right() {
            break;
        }
        p.line_segment([Pos2::new(x, r.bottom() - 8.0), Pos2::new(x, r.bottom())], Stroke::new(1.0, TEXT_DIM));
    }
    let xs: Vec<f32> = app.video.keyframes.iter().map(|k| r.left() + 8.0 + (r.width() - 16.0) * (k.time / dur)).collect();
    if xs.len() > 1 {
        p.line_segment([Pos2::new(xs[0], r.center().y), Pos2::new(*xs.last().unwrap(), r.center().y)], Stroke::new(2.0, Color32::from_gray(90)));
    }
    for (i, x) in xs.iter().enumerate() {
        let sel = app.selected_keyframe == Some(i);
        p.circle_filled(Pos2::new(*x, r.center().y), if sel { 7.0 } else { 5.5 }, if sel { ACCENT } else { TEXT });
    }
    if let Some(t) = app.playing {
        let x = r.left() + 8.0 + (r.width() - 16.0) * (t / dur).min(1.0);
        p.line_segment([Pos2::new(x, r.top()), Pos2::new(x, r.bottom())], Stroke::new(2.0, ACCENT));
    }
    if resp.clicked() {
        if let Some(pos) = resp.interact_pointer_pos() {
            app.selected_keyframe = xs.iter().enumerate().filter(|(_, x)| (**x - pos.x).abs() < 9.0).map(|(i, _)| i).next();
        }
    }
    let mut d = app.video.duration();
    ui.horizontal(|ui| {
        ui.label("Duration");
        if ui.add(egui::DragValue::new(&mut d).speed(0.1).range(0.5..=3600.0).suffix(" s")).changed() {
            app.video.set_duration(d);
        }
        ui.label(format!("{} keyframes", app.video.keyframes.len()));
    });
    ui.checkbox(&mut app.video.easing, "Ease in / ease out");
    ui.checkbox(&mut app.video.shaky, "Hand-held camera (shaky)");

    section(ui, "Keyframes");
    let mut remove = None;
    let mut goto = None;
    let n = app.video.keyframes.len();
    for i in 0..n {
        let sel = app.selected_keyframe == Some(i);
        egui::Frame::new().fill(if sel { Color32::from_rgb(0x3A, 0x2A, 0x1A) } else { BG_DARK }).corner_radius(4.0).inner_margin(egui::Margin::same(6)).show(ui, |ui| {
            ui.horizontal(|ui| {
                if ui.selectable_label(sel, format!("#{}", i + 1)).clicked() {
                    app.selected_keyframe = if sel { None } else { Some(i) };
                }
                let k = &mut app.video.keyframes[i];
                ui.add(egui::DragValue::new(&mut k.time).speed(0.05).range(0.0..=3600.0).suffix(" s"));
                if ui.small_button("Go").clicked() {
                    goto = Some(i);
                }
                if ui.small_button("Set to camera").clicked() {
                    k.pose = app.camera.pose;
                }
                if ui.small_button("🗑").clicked() {
                    remove = Some(i);
                }
            });
            if sel {
                let k = &mut app.video.keyframes[i];
                opt_slider(ui, "Time of day", &mut k.time_of_day, 0.0..=24.0, " h", app.sun.hour);
                opt_slider(ui, "Field of view", &mut k.fov, 10.0..=120.0, "°", app.vs.field_of_view);
                opt_slider(ui, "Focal point", &mut k.focal, 0.1..=100.0, " m", app.vs.dof_focal_point);
            }
        });
    }
    if let Some(i) = goto {
        let pose = app.video.keyframes[i].pose;
        app.camera.start_transition(pose, 0.6);
    }
    if let Some(i) = remove {
        app.video.remove(i);
        app.selected_keyframe = None;
    }
    // keep times ordered
    app.video.keyframes.sort_by(|a, b| a.time.partial_cmp(&b.time).unwrap_or(std::cmp::Ordering::Equal));
    ui.add_space(6.0);
    ui.label(RichText::new(format!("Output: {} fps, {} quality — set in Visual Settings › Output.", app.vs.fps, app.vs.bitrate.name())).small().color(TEXT_DIM));
}

fn opt_slider(ui: &mut Ui, label: &str, v: &mut Option<f32>, r: RangeInclusive<f32>, suffix: &str, current: f32) {
    ui.horizontal(|ui| {
        let mut on = v.is_some();
        if ui.checkbox(&mut on, label).changed() {
            *v = if on { Some(current) } else { None };
        }
        if let Some(x) = v.as_mut() {
            ui.add(egui::Slider::new(x, r).suffix(suffix));
        }
    });
}

fn assets_panel(app: &mut App, ui: &mut Ui, ctx: &egui::Context) {
    ui.horizontal_wrapped(|ui| {
        for c in scene::asset_categories() {
            if ui.selectable_label(app.asset_category == c, c).clicked() {
                app.asset_category = c.to_string();
            }
        }
    });
    ui.label(RichText::new("Pick an asset, then click in the 3D view to place it. Hold Shift to place several.").small().color(TEXT_DIM));
    ui.add_space(4.0);
    let cat = app.asset_category.clone();
    let tiles: Vec<&str> = scene::ASSETS.iter().filter(|a| a.category == cat).map(|a| a.name).collect();
    egui::Grid::new("asset_grid").num_columns(2).spacing([8.0, 8.0]).show(ui, |ui| {
        for (i, name) in tiles.iter().enumerate() {
            let active = app.asset_to_place.as_deref() == Some(*name);
            let (r, resp) = ui.allocate_exact_size(egui::vec2(135.0, 92.0), Sense::click());
            let p = ui.painter_at(r);
            p.rect_filled(r, 5.0, if active { Color32::from_rgb(0x4A, 0x30, 0x10) } else { BG_DARK });
            if active || resp.hovered() {
                p.rect_stroke(r, 5.0, Stroke::new(1.5, ACCENT), egui::StrokeKind::Inside);
            }
            asset_icon(&p, Rect::from_center_size(r.center() - egui::vec2(0.0, 10.0), egui::vec2(60.0, 50.0)), &cat);
            p.text(Pos2::new(r.center().x, r.bottom() - 12.0), Align2::CENTER_CENTER, *name, FontId::proportional(12.0), TEXT);
            if resp.clicked() {
                app.asset_to_place = if active { None } else { Some(name.to_string()) };
            }
            if i % 2 == 1 {
                ui.end_row();
            }
        }
    });
    ui.horizontal(|ui| {
        ui.label("Rotation");
        ui.add(egui::Slider::new(&mut app.asset_rotation, 0.0..=360.0).suffix("°"));
    });

    section(ui, "Selected");
    if let Some(g) = app.selected_group {
        let mut remove = false;
        if let Some(gr) = app.scene.group_mut(g) {
            ui.label(RichText::new(&gr.name).strong());
            grid(ui, "sel_asset", |ui| {
                ui.label("Rotation");
                ui.add(egui::Slider::new(&mut gr.rot_y, -180.0..=180.0).suffix("°"));
                ui.end_row();
                ui.label("Scale");
                ui.add(egui::Slider::new(&mut gr.scale, 0.1..=5.0));
                ui.end_row();
                ui.label("Position X / Y / Z");
                ui.horizontal(|ui| {
                    ui.add(egui::DragValue::new(&mut gr.offset.x).speed(0.05));
                    ui.add(egui::DragValue::new(&mut gr.offset.y).speed(0.05));
                    ui.add(egui::DragValue::new(&mut gr.offset.z).speed(0.05));
                });
                ui.end_row();
            });
            if ui.button("🗑 Delete (Del)").clicked() {
                remove = true;
            }
        } else {
            ui.label("Nothing selected.");
        }
        if remove {
            app.scene.remove_group(g);
            app.selected_group = None;
        }
    } else {
        ui.label(RichText::new("Click a placed asset or imported model in the view to select it.").color(TEXT_DIM));
    }

    section(ui, "Placed");
    let placed: Vec<(u32, String, GroupKind)> = app.scene.groups.iter().filter(|g| g.kind != GroupKind::Building).map(|g| (g.id, g.name.clone(), g.kind)).collect();
    if placed.is_empty() {
        ui.label(RichText::new("No assets placed yet.").color(TEXT_DIM));
    }
    for (id, name, kind) in placed {
        let sel = app.selected_group == Some(id);
        let tag = if kind == GroupKind::Imported { "  (model)" } else { "" };
        if ui.selectable_label(sel, format!("{name}{tag}")).clicked() {
            app.selected_group = Some(id);
        }
    }

    section(ui, "Import model");
    ui.horizontal(|ui| {
        ui.label("Units scale");
        ui.add(egui::DragValue::new(&mut app.import_scale).speed(0.01).range(0.001..=1000.0));
        if ui.button("Import OBJ...").clicked() {
            app.import_model(ctx);
        }
    });
    ui.checkbox(&mut app.live_updates, "Live updates: reload when the file changes");
}

fn asset_icon(p: &egui::Painter, r: Rect, cat: &str) {
    let c = r.center();
    match cat {
        "Vegetation" => {
            p.rect_filled(Rect::from_center_size(Pos2::new(c.x, r.bottom() - 9.0), egui::vec2(6.0, 18.0)), 1.0, Color32::from_rgb(110, 80, 50));
            p.circle_filled(Pos2::new(c.x, c.y - 4.0), 18.0, Color32::from_rgb(70, 130, 60));
        }
        "Furniture" => {
            p.rect_filled(Rect::from_center_size(c, egui::vec2(46.0, 8.0)), 2.0, Color32::from_rgb(170, 130, 90));
            p.rect_filled(Rect::from_min_size(Pos2::new(c.x - 20.0, c.y + 4.0), egui::vec2(4.0, 18.0)), 1.0, Color32::from_rgb(150, 110, 75));
            p.rect_filled(Rect::from_min_size(Pos2::new(c.x + 16.0, c.y + 4.0), egui::vec2(4.0, 18.0)), 1.0, Color32::from_rgb(150, 110, 75));
        }
        "People" => {
            p.circle_filled(Pos2::new(c.x, r.top() + 7.0), 6.0, Color32::from_rgb(210, 180, 150));
            p.rect_filled(Rect::from_min_max(Pos2::new(c.x - 8.0, r.top() + 15.0), Pos2::new(c.x + 8.0, r.bottom())), 4.0, Color32::from_rgb(70, 90, 140));
        }
        "Vehicles" => {
            p.rect_filled(Rect::from_center_size(Pos2::new(c.x, c.y + 4.0), egui::vec2(56.0, 16.0)), 5.0, Color32::from_rgb(180, 40, 40));
            p.rect_filled(Rect::from_center_size(Pos2::new(c.x - 4.0, c.y - 7.0), egui::vec2(30.0, 12.0)), 4.0, Color32::from_rgb(160, 35, 35));
            p.circle_filled(Pos2::new(c.x - 16.0, c.y + 13.0), 6.0, Color32::from_gray(30));
            p.circle_filled(Pos2::new(c.x + 16.0, c.y + 13.0), 6.0, Color32::from_gray(30));
        }
        _ => {
            p.line_segment([Pos2::new(c.x, r.top()), Pos2::new(c.x, r.bottom())], Stroke::new(3.0, Color32::from_gray(120)));
            p.circle_filled(Pos2::new(c.x, r.top() + 8.0), 10.0, Color32::from_rgb(255, 220, 140));
        }
    }
}

fn info_panel(app: &mut App, ui: &mut Ui) {
    ui.label(RichText::new("Click an object in the 3D view.").color(TEXT_DIM));
    if let Some(i) = app.selected_object {
        if let Some(o) = app.scene.objects.get(i) {
            let m = app.scene.object_matrix(o);
            let (a, b) = o.local_bounds;
            let size = (m.transform_vector3(b - a)).abs();
            let mat = app.scene.materials.get(o.material).map(|m| m.name.clone()).unwrap_or_default();
            let group = app.scene.group(o.group).map(|g| g.name.clone()).unwrap_or_default();
            section(ui, "Object");
            grid(ui, "obj_info", |ui| {
                ui.label("Name");
                ui.label(&o.name);
                ui.end_row();
                ui.label("Belongs to");
                ui.label(group);
                ui.end_row();
                ui.label("Material");
                ui.label(&mat);
                ui.end_row();
                ui.label("Size (W x D x H)");
                ui.label(format!("{:.2} x {:.2} x {:.2} m", size.x, size.z, size.y));
                ui.end_row();
                ui.label("Triangles");
                ui.label(format!("{}", o.mesh.indices.len() / 3));
                ui.end_row();
            });
            let mi = o.material;
            if ui.button("🎨 Edit this material").clicked() {
                app.selected_material = mi;
                app.show_material = true;
            }
        }
    }
    section(ui, "Scene");
    let tris: usize = app.scene.objects.iter().map(|o| o.mesh.indices.len() / 3).sum();
    grid(ui, "scene_info", |ui| {
        ui.label("Objects");
        ui.label(format!("{}", app.scene.objects.len()));
        ui.end_row();
        ui.label("Triangles");
        ui.label(format!("{tris}"));
        ui.end_row();
        ui.label("Materials");
        ui.label(format!("{}", app.scene.materials.len()));
        ui.end_row();
        ui.label("Lights");
        ui.label(format!("{}", app.scene.lights.len()));
        ui.end_row();
    });
}

fn issues_panel(app: &mut App, ui: &mut Ui, ctx: &egui::Context) {
    section(ui, "New issue");
    ui.add(egui::TextEdit::singleline(&mut app.issue_draft.title).hint_text("Title"));
    ui.add(egui::TextEdit::multiline(&mut app.issue_draft.description).hint_text("Description").desired_rows(3));
    ui.horizontal(|ui| {
        egui::ComboBox::from_id_salt("iss_prio").selected_text(app.issue_draft.priority.clone()).show_ui(ui, |ui| {
            for p in ["Low", "Normal", "High", "Critical"] {
                ui.selectable_value(&mut app.issue_draft.priority, p.to_string(), p);
            }
        });
        ui.label(if app.issue_draft.point.is_some() { "📍 point set" } else { "Click the view to pin a point" });
    });
    if ui.button("➕ Create issue at this view").clicked() {
        let mut iss = app.issue_draft.clone();
        if iss.title.trim().is_empty() {
            iss.title = format!("Issue {}", app.issues.len() + 1);
        }
        iss.pose = app.camera.pose;
        app.issues.push(iss);
        app.issue_draft = Issue { title: String::new(), description: String::new(), status: "Open".into(), priority: "Normal".into(), pose: app.camera.pose, point: None };
    }
    section(ui, "Issues");
    let mut goto = None;
    let mut del = None;
    for (i, iss) in app.issues.iter_mut().enumerate() {
        egui::Frame::new().fill(BG_DARK).corner_radius(4.0).inner_margin(egui::Margin::same(6)).show(ui, |ui| {
            ui.horizontal(|ui| {
                let col = match iss.priority.as_str() {
                    "Critical" => Color32::from_rgb(0xFF, 0x1D, 0x15),
                    "High" => ACCENT,
                    "Low" => Color32::from_rgb(0x00, 0xBF, 0x4C),
                    _ => Color32::from_rgb(0xF4, 0xE4, 0x09),
                };
                ui.label(RichText::new("●").color(col));
                ui.label(RichText::new(&iss.title).strong());
            });
            if !iss.description.is_empty() {
                ui.label(RichText::new(&iss.description).color(TEXT_DIM));
            }
            ui.horizontal(|ui| {
                egui::ComboBox::from_id_salt(("iss_status", i)).width(90.0).selected_text(iss.status.clone()).show_ui(ui, |ui| {
                    for s in ["Open", "In progress", "Resolved", "Closed"] {
                        ui.selectable_value(&mut iss.status, s.to_string(), s);
                    }
                });
                if ui.small_button("Go").clicked() {
                    goto = Some(iss.pose);
                }
                if ui.small_button("🗑").clicked() {
                    del = Some(i);
                }
            });
        });
    }
    if let Some(p) = goto {
        app.camera.start_transition(p, 0.8);
    }
    if let Some(i) = del {
        app.issues.remove(i);
    }
    ui.add_space(6.0);
    if ui.button("Export issues (JSON)...").clicked() {
        if let Some(p) = crate::export::dialog(&app.vs.default_folder).add_filter("Issues", &["json"]).set_file_name("issues.json").save_file() {
            let _ = std::fs::write(p, serde_json::to_string_pretty(&app.issues).unwrap_or_default());
            app.notify(ctx, "Issues exported");
        }
    }
}

// ------------------------------------------------------------------ windows

pub fn windows(app: &mut App, ctx: &egui::Context) {
    let mut open = app.show_visual;
    egui::Window::new("Visual Settings").open(&mut open).default_size([640.0, 560.0]).default_pos([300.0, 110.0]).collapsible(false).show(ctx, |ui| visual_settings(app, ui, ctx));
    app.show_visual &= open;

    let mut open = app.show_general;
    egui::Window::new("General Settings").open(&mut open).default_size([460.0, 420.0]).default_pos([420.0, 140.0]).collapsible(false).show(ctx, |ui| general_settings(app, ui));
    app.show_general &= open;

    let mut open = app.show_material;
    egui::Window::new("Material Editor").open(&mut open).default_size([620.0, 600.0]).default_pos([360.0, 100.0]).collapsible(false).show(ctx, |ui| material_editor(app, ui));
    app.show_material &= open;

    let mut open = app.show_batch;
    egui::Window::new("Batch Rendering").open(&mut open).default_size([360.0, 380.0]).collapsible(false).show(ctx, |ui| batch_window(app, ui, ctx));
    app.show_batch &= open;

    let mut open = app.show_sun;
    egui::Window::new("Sun & Location").open(&mut open).default_size([380.0, 360.0]).default_pos([900.0, 120.0]).collapsible(false).show(ctx, |ui| sun_window(app, ui));
    app.show_sun &= open;

    let mut open = app.show_about;
    egui::Window::new("About").open(&mut open).collapsible(false).resizable(false).show(ctx, |ui| {
        ui.heading(RichText::new("Galaxy Walk").color(ACCENT));
        ui.label("Real-time architectural walkthroughs and renderings.");
        ui.label("Version 0.1");
    });
    app.show_about &= open;
}

fn visual_settings(app: &mut App, ui: &mut Ui, ctx: &egui::Context) {
    ui.horizontal(|ui| {
        // ---- presets list (left)
        ui.vertical(|ui| {
            ui.set_width(160.0);
            ui.label(RichText::new("PRESETS").size(11.5).color(ACCENT).strong());
            let mut sel = None;
            for (i, p) in app.presets.iter().enumerate() {
                let label = if i == app.active_preset && app.preset_modified() { format!("{} *", p.name) } else { p.name.clone() };
                if ui.selectable_label(i == app.active_preset, label).clicked() {
                    sel = Some(i);
                }
            }
            if let Some(i) = sel {
                app.apply_preset(i);
            }
            ui.add_space(4.0);
            if app.preset_modified() && ui.button("💾 Save changes").clicked() {
                let a = app.active_preset;
                app.presets[a].settings = app.vs.clone();
            }
            if app.preset_modified() && ui.button("↺ Revert").clicked() {
                let a = app.active_preset;
                app.vs = app.presets[a].settings.clone();
            }
            ui.add(egui::TextEdit::singleline(&mut app.new_preset_name).hint_text("New preset name").desired_width(150.0));
            if ui.button("➕ New from current").clicked() {
                let name = if app.new_preset_name.trim().is_empty() { format!("Preset {}", app.presets.len() + 1) } else { app.new_preset_name.trim().to_string() };
                app.presets.push(Preset { name, settings: app.vs.clone() });
                app.active_preset = app.presets.len() - 1;
                app.new_preset_name.clear();
            }
            if ui.button("Rename").clicked() && !app.new_preset_name.trim().is_empty() {
                let a = app.active_preset;
                app.presets[a].name = app.new_preset_name.trim().to_string();
                app.new_preset_name.clear();
            }
            if app.presets.len() > 1 && ui.button("🗑 Delete").clicked() {
                app.presets.remove(app.active_preset);
                app.active_preset = 0;
                app.vs = app.presets[0].settings.clone();
            }
            if ui.button("Reset to defaults").clicked() {
                app.vs = VisualSettings::default();
            }
            ui.separator();
            if ui.button("Load from file...").clicked() {
                if let Some(p) = crate::export::dialog(&app.vs.default_folder).add_filter("Preset", &["json"]).pick_file() {
                    match std::fs::read_to_string(&p).ok().and_then(|t| serde_json::from_str::<Preset>(&t).ok()) {
                        Some(pr) => {
                            app.presets.push(pr);
                            app.apply_preset(app.presets.len() - 1);
                        }
                        None => app.notify(ctx, "That file is not a preset."),
                    }
                }
            }
            if ui.button("Save to file...").clicked() {
                if let Some(p) = crate::export::dialog(&app.vs.default_folder).add_filter("Preset", &["json"]).set_file_name("preset.json").save_file() {
                    let pr = Preset { name: app.presets[app.active_preset].name.clone(), settings: app.vs.clone() };
                    let _ = std::fs::write(p, serde_json::to_string_pretty(&pr).unwrap_or_default());
                }
            }
        });
        ui.separator();
        // ---- tabs (right)
        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                for (i, t) in ["Main", "Image", "Atmosphere", "Sky", "Output"].iter().enumerate() {
                    if ui.selectable_label(app.vis_tab == i, RichText::new(*t).size(14.0)).clicked() {
                        app.vis_tab = i;
                    }
                }
            });
            ui.separator();
            egui::ScrollArea::vertical().id_salt("vis_scroll").show(ui, |ui| {
                let vs = &mut app.vs;
                match app.vis_tab {
                    0 => tab_main(ui, vs),
                    1 => tab_image(ui, vs),
                    2 => tab_atmosphere(ui, vs),
                    3 => tab_sky(ui, vs),
                    _ => tab_output(ui, vs, app.viewport_px),
                }
            });
        });
    });
}

fn tab_main(ui: &mut Ui, vs: &mut VisualSettings) {
    section(ui, "Style");
    grid(ui, "vs_style", |ui| {
        ui.label("Mode");
        egui::ComboBox::from_id_salt("style").selected_text(match vs.rendering_style {
            RenderingStyle::None => "None",
            RenderingStyle::White => "White",
            RenderingStyle::Polystyrol => "Polystyrol",
            RenderingStyle::LightView => "Light View",
        })
        .show_ui(ui, |ui| {
            ui.selectable_value(&mut vs.rendering_style, RenderingStyle::None, "None");
            ui.selectable_value(&mut vs.rendering_style, RenderingStyle::White, "White");
            ui.selectable_value(&mut vs.rendering_style, RenderingStyle::Polystyrol, "Polystyrol");
            ui.selectable_value(&mut vs.rendering_style, RenderingStyle::LightView, "Light View");
        });
        ui.end_row();
        slider(ui, "Outline thickness", &mut vs.outline_thickness, 0.0..=100.0, " %");
        if vs.rendering_style == RenderingStyle::Polystyrol {
            slider(ui, "Translucency", &mut vs.polystyrol_transmission, 0.0..=100.0, " %");
        }
        if vs.rendering_style == RenderingStyle::LightView {
            check(ui, "Automatic scale", &mut vs.light_view_auto);
            if !vs.light_view_auto {
                // The two limits always stay at least 10 lx apart;
                // moving one pushes the other.
                if slider(ui, "Minimum", &mut vs.light_view_min, 0.0..=10000.0, " lx") {
                    vs.light_view_max = vs.light_view_max.max(vs.light_view_min + 10.0);
                }
                if slider(ui, "Maximum", &mut vs.light_view_max, 10.0..=120000.0, " lx") {
                    vs.light_view_min = vs.light_view_min.min(vs.light_view_max - 10.0).max(0.0);
                }
            }
        }
    });
    section(ui, "Camera");
    grid(ui, "vs_cam", |ui| {
        ui.label("Projection");
        ui.horizontal(|ui| {
            ui.selectable_value(&mut vs.projection, Projection::Perspective, "Perspective");
            ui.selectable_value(&mut vs.projection, Projection::TwoPoint, "Two-Point");
            ui.selectable_value(&mut vs.projection, Projection::Orthographic, "Orthographic");
        });
        ui.end_row();
        check(ui, "Auto exposure", &mut vs.auto_exposure);
        slider(ui, "Exposure brightness", &mut vs.exposure_brightness, 0.0..=100.0, " %");
        if vs.projection != Projection::Orthographic {
            slider(ui, "Field of view", &mut vs.field_of_view, 1.0..=179.0, "°");
        }
        slider(ui, "Depth of field", &mut vs.dof_strength, 0.0..=100.0, " %");
        check(ui, "Auto focus", &mut vs.dof_auto_focus);
        if !vs.dof_auto_focus {
            slider(ui, "Focal distance", &mut vs.dof_focal_point, 0.1..=100.0, " m");
        }
    });
    section(ui, "Rendering quality");
    grid(ui, "vs_q", |ui| {
        ui.label("Quality");
        ui.horizontal(|ui| {
            for q in Quality::ALL {
                ui.selectable_value(&mut vs.quality, q, q.name());
            }
        });
        ui.end_row();
        check(ui, "Auto resolution while moving", &mut vs.auto_resolution);
        check(ui, "Grass rendering", &mut vs.grass_rendering);
        check(ui, "Ground plane", &mut vs.ground_plane);
    });
}

fn tab_image(ui: &mut Ui, vs: &mut VisualSettings) {
    section(ui, "Image");
    grid(ui, "vs_img", |ui| {
        check(ui, "Auto contrast", &mut vs.auto_contrast);
        slider(ui, "Highlights", &mut vs.highlights, -100.0..=100.0, "");
        slider(ui, "Shadows", &mut vs.shadows, -100.0..=100.0, "");
        slider(ui, "Saturation", &mut vs.saturation, 0.0..=200.0, " %");
        slider(ui, "Color temperature", &mut vs.color_temperature, 1500.0..=15000.0, " K");
    });
    section(ui, "Effects");
    grid(ui, "vs_fx", |ui| {
        slider(ui, "Motion blur", &mut vs.motion_blur, 0.0..=100.0, " %");
        slider(ui, "Lens flare", &mut vs.lens_flare, 0.0..=100.0, " %");
        slider(ui, "Bloom", &mut vs.bloom, 0.0..=100.0, " %");
        slider(ui, "Vignette", &mut vs.vignette, 0.0..=100.0, " %");
        slider(ui, "Chromatic aberration", &mut vs.chromatic_aberration, 0.0..=100.0, " %");
    });
}

fn tab_atmosphere(ui: &mut Ui, vs: &mut VisualSettings) {
    section(ui, "Fog");
    grid(ui, "vs_fog", |ui| {
        slider(ui, "Intensity", &mut vs.fog_intensity, 0.0..=100.0, " %");
        slider(ui, "Height", &mut vs.fog_height, 0.1..=10000.0, " m");
    });
    section(ui, "Illumination");
    grid(ui, "vs_ill", |ui| {
        slider(ui, "Sun brightness", &mut vs.sun_brightness, 0.0..=500.0, " %");
        slider(ui, "Night sky brightness", &mut vs.night_sky_brightness, 0.0..=30000.0, " %");
        slider(ui, "Shadow sharpness", &mut vs.shadow_sharpness, 0.0..=100.0, " %");
        slider(ui, "Artificial light brightness", &mut vs.artificial_light_brightness, 0.0..=200.0, " %");
        slider(ui, "Ambient brightness", &mut vs.ambient_brightness, 0.0..=100.0, " %");
    });
    section(ui, "Wind");
    grid(ui, "vs_wind", |ui| {
        slider(ui, "Intensity", &mut vs.wind_intensity, 0.0..=100.0, " %");
        slider(ui, "Direction", &mut vs.wind_direction, 0.0..=360.0, "°");
    });
}

fn tab_sky(ui: &mut Ui, vs: &mut VisualSettings) {
    section(ui, "Horizon");
    grid(ui, "vs_hz", |ui| {
        check(ui, "White background", &mut vs.white_background);
        ui.label("Preset");
        egui::ComboBox::from_id_salt("horizon").selected_text(vs.horizon.name()).show_ui(ui, |ui| {
            for h in Horizon::ALL {
                ui.selectable_value(&mut vs.horizon, h, h.name());
            }
        });
        ui.end_row();
        slider(ui, "Rotation", &mut vs.horizon_rotation, 0.0..=360.0, "°");
        if vs.horizon == Horizon::Skybox {
            ui.label("Skybox image");
            ui.horizontal(|ui| {
                let name = std::path::Path::new(&vs.skybox_file).file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "(none)".into());
                ui.label(name);
                if ui.button("Load...").clicked() {
                    if let Some(f) = pick_file("Skybox (equirectangular)", "Images", &["hdr", "png", "jpg", "jpeg"]) {
                        vs.skybox_file = f;
                    }
                }
            });
            ui.end_row();
            check(ui, "Brightest point as sun direction", &mut vs.skybox_use_sun_spot);
            check(ui, "Normalize brightness", &mut vs.skybox_normalize);
            slider(ui, "Brightness", &mut vs.skybox_brightness, 100.0..=100000.0, " lx");
        }
    });
    section(ui, "Clouds");
    grid(ui, "vs_cl", |ui| {
        slider(ui, "Density", &mut vs.cloud_density, 0.0..=100.0, " %");
        slider(ui, "Variety", &mut vs.cloud_variety, 0.0..=100.0, " %");
        slider(ui, "Cirrus amount", &mut vs.cloud_cirrus, 0.0..=100.0, " %");
        ui.label("Contrails");
        ui.add(egui::Slider::new(&mut vs.cloud_contrails, 0..=10));
        ui.end_row();
        slider(ui, "Offset longitude", &mut vs.cloud_x, 0.0..=10000.0, "");
        slider(ui, "Offset latitude", &mut vs.cloud_y, 0.0..=10000.0, "");
    });
    section(ui, "Night");
    grid(ui, "vs_night", |ui| {
        slider(ui, "Moon size", &mut vs.moon_size, 0.0..=1000.0, " %");
    });
}

fn tab_output(ui: &mut Ui, vs: &mut VisualSettings, viewport: (u32, u32)) {
    section(ui, "Resolution");
    grid(ui, "vs_res", |ui| {
        ui.label("Resolution");
        egui::ComboBox::from_id_salt("res").selected_text(vs.capture_resolution.name()).show_ui(ui, |ui| {
            for r in CaptureResolution::ALL {
                ui.selectable_value(&mut vs.capture_resolution, r, r.name());
            }
        });
        ui.end_row();
        if vs.capture_resolution == CaptureResolution::Custom {
            ui.label("Width x height");
            ui.horizontal(|ui| {
                ui.add(egui::DragValue::new(&mut vs.custom_width).range(16..=8192).suffix(" px"));
                ui.add(egui::DragValue::new(&mut vs.custom_height).range(16..=8192).suffix(" px"));
            });
            ui.end_row();
        }
        check(ui, "Use viewport aspect ratio", &mut vs.use_viewport_aspect);
        check(ui, "Show safe frame", &mut vs.show_safe_frame);
        let (w, h) = vs.capture_size(viewport);
        ui.label("Output size");
        ui.label(RichText::new(format!("{w} x {h} px")).color(ACCENT));
        ui.end_row();
    });
    section(ui, "Image");
    grid(ui, "vs_img_out", |ui| {
        ui.label("File format");
        ui.horizontal(|ui| {
            ui.selectable_value(&mut vs.file_format, "png".to_string(), "PNG");
            ui.selectable_value(&mut vs.file_format, "jpg".to_string(), "JPG");
        });
        ui.end_row();
        check(ui, "Export alpha channel (PNG)", &mut vs.apply_alpha);
        check(ui, "Export depth, material and object ID", &mut vs.export_masks);
        if vs.export_masks {
            slider(ui, "Depth range", &mut vs.depth_range, 0.01..=1000.0, " m");
        }
        check(ui, "Automatic naming (save without asking)", &mut vs.auto_naming);
        {
            ui.label("Save folder");
            ui.horizontal(|ui| {
                ui.label(RichText::new(crate::export::output_dir(&vs.default_folder).display().to_string()).color(ACCENT));
                if ui.button("Open").clicked() {
                    crate::export::reveal(&crate::export::output_dir(&vs.default_folder));
                }
                if !vs.default_folder.is_empty() && ui.button("Reset").clicked() {
                    vs.default_folder.clear();
                }
                if ui.button("...").clicked() {
                    if let Some(d) = crate::export::dialog(&vs.default_folder).pick_folder() {
                        vs.default_folder = d.display().to_string();
                    }
                }
            });
            ui.end_row();
        }
    });
    section(ui, "Video");
    grid(ui, "vs_vid", |ui| {
        ui.label("Frame rate");
        ui.horizontal(|ui| {
            for f in [25u32, 30, 60, 120] {
                ui.selectable_value(&mut vs.fps, f, format!("{f}"));
            }
        });
        ui.end_row();
        ui.label("Compression");
        egui::ComboBox::from_id_salt("bitrate").selected_text(vs.bitrate.name()).show_ui(ui, |ui| {
            for b in Bitrate::ALL {
                ui.selectable_value(&mut vs.bitrate, b, b.name());
            }
        });
        ui.end_row();
    });
    section(ui, "Panorama");
    grid(ui, "vs_pano", |ui| {
        ui.label("Resolution (width)");
        ui.horizontal(|ui| {
            for (n, r) in [("Normal", 2048u32), ("High", 4096), ("Very high", 8192)] {
                ui.selectable_value(&mut vs.panorama_resolution, r, n);
            }
        });
        ui.end_row();
    });
}

fn general_settings(app: &mut App, ui: &mut Ui) {
    ui.horizontal(|ui| {
        for (i, t) in ["Input", "Customization", "Capture"].iter().enumerate() {
            if ui.selectable_label(app.gen_tab == i, RichText::new(*t).size(14.0)).clicked() {
                app.gen_tab = i;
            }
        }
    });
    ui.separator();
    let gs = &mut app.gs;
    match app.gen_tab {
        0 => {
            section(ui, "Mouse");
            grid(ui, "gs_mouse", |ui| {
                slider(ui, "Mouse speed", &mut gs.mouse_speed, 0.1..=3.0, "");
                slider(ui, "Mouse smoothing", &mut gs.mouse_smoothing, 0.0..=5.0, "");
                check(ui, "Invert X axis", &mut gs.invert_x);
                check(ui, "Invert Y axis", &mut gs.invert_y);
            });
            section(ui, "Movement");
            grid(ui, "gs_move", |ui| {
                slider(ui, "Movement speed", &mut gs.move_speed, 0.1..=100.0, "");
                slider(ui, "Movement smoothing", &mut gs.move_smoothing, 0.0..=5.0, "");
            });
            section(ui, "Walk mode body");
            grid(ui, "gs_body", |ui| {
                slider(ui, "Spectator height (body)", &mut gs.spectator_height, 0.5..=2.5, " m");
                slider(ui, "Body width", &mut gs.spectator_width, 0.1..=1.5, " m");
            });
        }
        1 => {
            section(ui, "Window");
            grid(ui, "gs_win", |ui| {
                ui.label("Window title");
                ui.add(egui::TextEdit::singleline(&mut gs.window_title).hint_text("Galaxy Walk"));
                ui.end_row();
                check(ui, "Show help panel", &mut gs.show_help);
                check(ui, "Show map", &mut gs.show_minimap);
                check(ui, "Sounds", &mut gs.sound);
            });
            section(ui, "Watermark");
            grid(ui, "gs_wm", |ui| {
                ui.label("Image");
                ui.horizontal(|ui| {
                    let n = std::path::Path::new(&gs.watermark_file).file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "(none)".into());
                    ui.label(n);
                    if ui.button("...").clicked() {
                        if let Some(f) = pick_file("Watermark", "PNG", &["png"]) {
                            gs.watermark_file = f;
                        }
                    }
                    if !gs.watermark_file.is_empty() && ui.button("✖").clicked() {
                        gs.watermark_file.clear();
                    }
                });
                ui.end_row();
                ui.label("Position");
                egui::Grid::new("wm_anchor").spacing([2.0, 2.0]).show(ui, |ui| {
                    for row in 0..3 {
                        for col in 0..3 {
                            let a = row * 3 + col;
                            if ui.selectable_label(gs.watermark_anchor == a, "■").clicked() {
                                gs.watermark_anchor = a;
                            }
                        }
                        ui.end_row();
                    }
                });
                ui.end_row();
            });
        }
        _ => {
            section(ui, "Capture");
            grid(ui, "gs_cap", |ui| {
                check(ui, "Screenshot key needs Shift (Shift+F11)", &mut gs.screenshot_hotkey_shift);
            });
        }
    }
}

fn material_editor(app: &mut App, ui: &mut Ui) {
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            ui.set_width(170.0);
            ui.label(RichText::new("MATERIALS").size(11.5).color(ACCENT).strong());
            ui.label(RichText::new("Tip: click an object in the view to pick its material.").small().color(TEXT_DIM));
            egui::ScrollArea::vertical().id_salt("mat_list").max_height(480.0).show(ui, |ui| {
                for (i, m) in app.scene.materials.iter().enumerate() {
                    ui.horizontal(|ui| {
                        let (r, _) = ui.allocate_exact_size(egui::vec2(14.0, 14.0), Sense::hover());
                        let c = m.color;
                        ui.painter().rect_filled(r, 3.0, Color32::from_rgb((c[0].powf(1.0 / 2.2) * 255.0) as u8, (c[1].powf(1.0 / 2.2) * 255.0) as u8, (c[2].powf(1.0 / 2.2) * 255.0) as u8));
                        if ui.selectable_label(app.selected_material == i, &m.name).clicked() {
                            app.selected_material = i;
                        }
                    });
                }
            });
        });
        ui.separator();
        ui.vertical(|ui| {
            let Some(m) = app.scene.materials.get_mut(app.selected_material) else { return };
            ui.heading(RichText::new(&m.name).strong());
            egui::ScrollArea::vertical().id_salt("mat_edit").show(ui, |ui| {
                grid(ui, "mat_type", |ui| {
                    ui.label("Type");
                    egui::ComboBox::from_id_salt("mtype").selected_text(m.mtype.name()).show_ui(ui, |ui| {
                        for t in MaterialType::ALL {
                            ui.selectable_value(&mut m.mtype, t, t.name());
                        }
                    });
                    ui.end_row();
                    check(ui, "Two sided", &mut m.double_sided);
                });
                section(ui, "Albedo");
                grid(ui, "mat_albedo", |ui| {
                    ui.label("Color");
                    ui.horizontal(|ui| {
                        ui.color_edit_button_rgb(&mut m.color);
                        egui::ComboBox::from_id_salt("albedo_ref").width(110.0).selected_text("Reference...").show_ui(ui, |ui| {
                            for (n, c) in ALBEDO_REFS {
                                if ui.selectable_label(false, *n).clicked() {
                                    m.color = c.map(|v| v.powf(2.2));
                                }
                            }
                        });
                    });
                    ui.end_row();
                    ui.label("Texture");
                    ui.horizontal(|ui| {
                        let n = std::path::Path::new(&m.texture).file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "(none)".into());
                        ui.label(n);
                        if ui.button("...").clicked() {
                            if let Some(f) = pick_file("Texture", "Images", &["png", "jpg", "jpeg"]) {
                                m.texture = f;
                            }
                        }
                        if !m.texture.is_empty() && ui.button("✖").clicked() {
                            m.texture.clear();
                        }
                    });
                    ui.end_row();
                    if !m.texture.is_empty() {
                        slider(ui, "Image fade", &mut m.image_fade, 0.0..=1.0, "");
                        ui.label("Tint");
                        ui.color_edit_button_rgb(&mut m.tint);
                        ui.end_row();
                        slider(ui, "Width", &mut m.tex_width, 0.01..=20.0, " m");
                        slider(ui, "Height", &mut m.tex_height, 0.01..=20.0, " m");
                        slider(ui, "Rotation", &mut m.tex_rotation, 0.0..=360.0, "°");
                    }
                });
                section(ui, "Bump");
                grid(ui, "mat_bump", |ui| {
                    ui.label("Map type");
                    ui.horizontal(|ui| {
                        ui.selectable_value(&mut m.bump_type, BumpType::Bump, "Bump");
                        ui.selectable_value(&mut m.bump_type, BumpType::Normal, "Normal");
                        ui.selectable_value(&mut m.bump_type, BumpType::Displacement, "Displacement");
                    });
                    ui.end_row();
                    slider(ui, "Amount", &mut m.bump_amount, -1.0..=1.0, "");
                });
                section(ui, "Reflections");
                grid(ui, "mat_refl", |ui| {
                    slider(ui, "Roughness", &mut m.roughness, 0.0..=1.0, "");
                    slider(ui, "Metallic", &mut m.metallic, 0.0..=1.0, "");
                    slider(ui, "Specular", &mut m.specular, 0.0..=1.0, "");
                });
                section(ui, "Transparency");
                grid(ui, "mat_tr", |ui| {
                    ui.label("Mode");
                    ui.horizontal(|ui| {
                        ui.selectable_value(&mut m.transparency, TransparencyType::Transmittance, "Transmittance");
                        ui.selectable_value(&mut m.transparency, TransparencyType::Cutout, "Cutout");
                    });
                    ui.end_row();
                    slider(ui, "Opacity", &mut m.opacity, 0.0..=1.0, "");
                    if m.transparency == TransparencyType::Transmittance && m.opacity < 1.0 {
                        slider(ui, "Refractive index", &mut m.refractive_index, 1.0..=2.5, "");
                        ui.label("Tint color");
                        ui.color_edit_button_rgb(&mut m.glass_tint);
                        ui.end_row();
                        check(ui, "Frosted glass", &mut m.frosted);
                    }
                });
                section(ui, "Self-illumination");
                grid(ui, "mat_em", |ui| {
                    ui.label("Color");
                    ui.color_edit_button_rgb(&mut m.emissive_color);
                    ui.end_row();
                    slider(ui, "Luminance", &mut m.luminance, 0.0..=10000.0, " cd/m²");
                });
                match m.mtype {
                    MaterialType::Water => {
                        section(ui, "Water");
                        grid(ui, "mat_water", |ui| {
                            slider(ui, "Wave height", &mut m.wave_height, 0.0..=1.0, "");
                            slider(ui, "Wave scale", &mut m.wave_scale, 0.0..=1.0, "");
                            slider(ui, "Caustics", &mut m.caustics, 0.0..=1.0, "");
                        });
                    }
                    MaterialType::Grass | MaterialType::Carpet => {
                        section(ui, if m.mtype == MaterialType::Grass { "Grass" } else { "Carpet" });
                        grid(ui, "mat_grass", |ui| {
                            slider(ui, "Height", &mut m.grass_height, 0.01..=1.0, " m");
                            slider(ui, "Height variation", &mut m.grass_variation, 0.0..=1.0, "");
                        });
                    }
                    _ => {}
                }
            });
        });
    });
}

fn batch_window(app: &mut App, ui: &mut Ui, ctx: &egui::Context) {
    app.batch.selected.resize(app.views.len(), true);
    ui.label("Render each selected view with its own sun, field of view and preset.");
    ui.horizontal(|ui| {
        if ui.small_button("All").clicked() {
            app.batch.selected.iter_mut().for_each(|s| *s = true);
        }
        if ui.small_button("None").clicked() {
            app.batch.selected.iter_mut().for_each(|s| *s = false);
        }
    });
    egui::ScrollArea::vertical().max_height(220.0).show(ui, |ui| {
        for (i, v) in app.views.iter().enumerate() {
            ui.checkbox(&mut app.batch.selected[i], &v.name);
        }
    });
    ui.separator();
    ui.checkbox(&mut app.batch.panorama, "Render as 360° panoramas");
    let (w, h) = app.vs.capture_size(app.viewport_px);
    ui.label(RichText::new(if app.batch.panorama { format!("Panorama width {} px", app.vs.panorama_resolution) } else { format!("Image size {w} x {h} px ({})", app.vs.file_format.to_uppercase()) }).color(TEXT_DIM));
    if ui.add(egui::Button::new(RichText::new("Start batch rendering").color(Color32::BLACK)).fill(ACCENT)).clicked() {
        app.batch_render(ctx);
        app.show_batch = false;
    }
}

fn sun_window(app: &mut App, ui: &mut Ui) {
    let s = &mut app.sun;
    let (az, alt) = s.azimuth_altitude();
    section(ui, "Time");
    grid(ui, "sun_t", |ui| {
        slider(ui, "Time of day", &mut s.hour, 0.0..=23.99, " h");
        ui.label("Date (day / month / year)");
        ui.horizontal(|ui| {
            ui.add(egui::DragValue::new(&mut s.day).range(1..=31));
            ui.add(egui::DragValue::new(&mut s.month).range(1..=12));
            ui.add(egui::DragValue::new(&mut s.year).range(1900..=2200));
        });
        ui.end_row();
    });
    section(ui, "Location");
    grid(ui, "sun_loc", |ui| {
        ui.label("Place");
        egui::ComboBox::from_id_salt("place").selected_text("Choose...").show_ui(ui, |ui| {
            for (n, lat, lon, gmt) in PLACES {
                if ui.selectable_label(false, *n).clicked() {
                    s.latitude = *lat;
                    s.longitude = *lon;
                    s.gmt_offset = *gmt;
                }
            }
        });
        ui.end_row();
        slider(ui, "Latitude", &mut s.latitude, -90.0..=90.0, "°");
        slider(ui, "Longitude", &mut s.longitude, -180.0..=180.0, "°");
        slider(ui, "Time zone (GMT)", &mut s.gmt_offset, -12.0..=14.0, " h");
        slider(ui, "North rotation", &mut s.rotation, 0.0..=360.0, "°");
        slider(ui, "Extra sun height", &mut s.angle_offset, -60.0..=60.0, "°");
    });
    ui.separator();
    ui.label(RichText::new(format!("Sun azimuth {az:.1}°, altitude {alt:.1}°")).color(TEXT_DIM));
    ui.label(RichText::new("Shift + left drag in the view changes the time; Shift + right drag turns the sun. Home resets.").small().color(TEXT_DIM));
    if ui.button("Use as starting sun (Home)").clicked() {
        app.sun_start = app.sun;
    }
}

// ------------------------------------------------------------------ overlays on the 3D view

pub fn viewport_overlay(app: &mut App, ui: &mut Ui, ctx: &egui::Context, rect: Rect) {
    let p = ui.painter_at(rect);

    // safe frame
    if app.vs.show_safe_frame {
        let (w, h) = app.vs.capture_size(app.viewport_px);
        let aspect = w as f32 / h as f32;
        let mut fr = rect;
        if rect.width() / rect.height() > aspect {
            let nw = rect.height() * aspect;
            fr = Rect::from_center_size(rect.center(), egui::vec2(nw, rect.height()));
        } else {
            let nh = rect.width() / aspect;
            fr = Rect::from_center_size(rect.center(), egui::vec2(rect.width(), nh));
        }
        let shade = Color32::from_black_alpha(130);
        p.rect_filled(Rect::from_min_max(rect.min, Pos2::new(fr.left(), rect.bottom())), 0.0, shade);
        p.rect_filled(Rect::from_min_max(Pos2::new(fr.right(), rect.top()), rect.max), 0.0, shade);
        p.rect_filled(Rect::from_min_max(Pos2::new(fr.left(), rect.top()), Pos2::new(fr.right(), fr.top())), 0.0, shade);
        p.rect_filled(Rect::from_min_max(Pos2::new(fr.left(), fr.bottom()), Pos2::new(fr.right(), rect.bottom())), 0.0, shade);
        p.rect_stroke(fr, 0.0, Stroke::new(1.0, ACCENT), egui::StrokeKind::Inside);
    }

    // crosshair while placing assets
    if app.asset_to_place.is_some() {
        p.text(rect.center_top() + egui::vec2(0.0, 16.0), Align2::CENTER_TOP, format!("Click to place: {}  (Esc to cancel)", app.asset_to_place.clone().unwrap_or_default()), FontId::proportional(14.0), ACCENT);
    }
    if app.mode == Mode::Issues {
        if let Some(pt) = app.issue_draft.point {
            if let Some(s) = project(app, rect, pt) {
                p.circle_filled(s, 7.0, ACCENT);
                p.circle_stroke(s, 10.0, Stroke::new(2.0, Color32::WHITE));
            }
        }
        for iss in &app.issues {
            if let Some(pt) = iss.point {
                if let Some(s) = project(app, rect, pt) {
                    p.circle_filled(s, 6.0, Color32::from_rgb(0xFF, 0x1D, 0x15));
                    p.text(s + egui::vec2(10.0, -10.0), Align2::LEFT_BOTTOM, &iss.title, FontId::proportional(12.0), Color32::WHITE);
                }
            }
        }
    }

    // compass (top right)
    let cc = Pos2::new(rect.right() - 44.0, rect.top() + 44.0);
    p.circle_filled(cc, 26.0, Color32::from_black_alpha(140));
    let yaw = (app.camera.pose.yaw + app.sun.rotation * 0.0).to_radians();
    let north = egui::vec2(-yaw.sin(), -yaw.cos());
    p.line_segment([cc, cc + north * 20.0], Stroke::new(3.0, ACCENT));
    p.text(cc + north * 15.0 + egui::vec2(0.0, 0.0), Align2::CENTER_CENTER, "N", FontId::proportional(11.0), Color32::WHITE);
    p.line_segment([cc, cc - north * 16.0], Stroke::new(2.0, Color32::from_gray(160)));

    // time of day bar (bottom centre)
    let bar = Rect::from_center_size(Pos2::new(rect.center().x, rect.bottom() - 26.0), egui::vec2(300.0_f32.min(rect.width() - 40.0), 26.0));
    p.rect_filled(bar, 13.0, Color32::from_black_alpha(150));
    let x = bar.left() + 14.0 + (bar.width() - 28.0) * (app.sun.hour / 24.0);
    let inner = Rect::from_min_max(Pos2::new(bar.left() + 14.0, bar.center().y - 2.0), Pos2::new(bar.right() - 14.0, bar.center().y + 2.0));
    p.rect_filled(inner, 2.0, Color32::from_gray(80));
    p.circle_filled(Pos2::new(x, bar.center().y), 6.0, ACCENT);
    p.text(Pos2::new(bar.right() - 10.0, bar.top() - 4.0), Align2::RIGHT_BOTTOM, format!("☀ {}", app.sun.time_string()), FontId::proportional(13.0), Color32::WHITE);
    let resp = ui.interact(bar, egui::Id::new("tod_bar"), Sense::click_and_drag());
    if resp.dragged() || resp.clicked() {
        if let Some(pos) = resp.interact_pointer_pos() {
            app.sun.hour = ((pos.x - bar.left() - 14.0) / (bar.width() - 28.0) * 24.0).clamp(0.0, 23.99);
        }
    }

    // light view legend
    if app.vs.rendering_style == RenderingStyle::LightView {
        let lg = Rect::from_min_size(Pos2::new(rect.right() - 60.0, rect.top() + 90.0), egui::vec2(18.0, 200.0));
        let n = 20;
        for i in 0..n {
            let t = 1.0 - i as f32 / n as f32;
            let c = heat(t);
            let r = Rect::from_min_size(Pos2::new(lg.left(), lg.top() + lg.height() * i as f32 / n as f32), egui::vec2(lg.width(), lg.height() / n as f32 + 0.5));
            p.rect_filled(r, 0.0, c);
        }
        let (mn, mx) = if app.vs.light_view_auto { (0.0, 0.0) } else { (app.vs.light_view_min, app.vs.light_view_max) };
        let top = if app.vs.light_view_auto { "auto".to_string() } else { format!("{mx:.0} lx") };
        let bot = if app.vs.light_view_auto { "".to_string() } else { format!("{mn:.0} lx") };
        p.text(lg.left_top() - egui::vec2(4.0, 0.0), Align2::RIGHT_TOP, top, FontId::proportional(11.0), Color32::WHITE);
        p.text(lg.left_bottom() - egui::vec2(4.0, 0.0), Align2::RIGHT_BOTTOM, bot, FontId::proportional(11.0), Color32::WHITE);
    }

    // help panel
    if app.gs.show_help {
        let lines = [
            ("W A S D / arrows", "Move"),
            ("E / Q", "Up / down (fly)"),
            ("Shift / Ctrl", "Faster x3 / x10 (fly x6 / x20)"),
            ("Left drag", "Look around"),
            ("Right drag", "Orbit around point"),
            ("Middle drag / L+R", "Pan"),
            ("Mouse wheel", "Fly forward / back"),
            ("Double-click", "Teleport to point"),
            ("Space", "Walk / fly mode"),
            ("Shift + right drag", "Time of day"),
            ("Shift + left drag", "Sun direction"),
            ("Home", "Reset sun"),
            ("1 - 5", "Standard views"),
            ("Page Up / Down", "Favourite views"),
            ("Shift + F11", "Screenshot"),
            ("H", "Hide this help"),
        ];
        let h = 26.0 + lines.len() as f32 * 17.0;
        let hr = Rect::from_min_size(Pos2::new(rect.right() - 290.0, rect.bottom() - h - 60.0), egui::vec2(274.0, h));
        p.rect_filled(hr, 6.0, Color32::from_black_alpha(170));
        p.text(hr.left_top() + egui::vec2(12.0, 8.0), Align2::LEFT_TOP, "CONTROLS", FontId::proportional(12.0), ACCENT);
        for (i, (k, d)) in lines.iter().enumerate() {
            let y = hr.top() + 28.0 + i as f32 * 17.0;
            p.text(Pos2::new(hr.left() + 12.0, y), Align2::LEFT_TOP, *k, FontId::proportional(12.0), Color32::WHITE);
            p.text(Pos2::new(hr.left() + 138.0, y), Align2::LEFT_TOP, *d, FontId::proportional(12.0), TEXT_DIM);
        }
    }

    // minimap
    if app.gs.show_minimap {
        minimap(app, ui, rect);
    }

    // video playback banner
    if let Some(t) = app.playing {
        p.text(rect.left_top() + egui::vec2(16.0, 14.0), Align2::LEFT_TOP, format!("▶ Preview {:.1} / {:.1} s", t, app.video.duration()), FontId::proportional(15.0), ACCENT);
    }

    // toast
    if let Some((text, until)) = app.toast.clone() {
        let now = ctx.input(|i| i.time);
        if now < until {
            let galley = p.layout_no_wrap(text.clone(), FontId::proportional(14.0), Color32::WHITE);
            let size = galley.size() + egui::vec2(28.0, 16.0);
            let tr = Rect::from_center_size(Pos2::new(rect.center().x, rect.top() + 50.0), size);
            p.rect_filled(tr, 6.0, Color32::from_black_alpha(200));
            p.galley(tr.min + egui::vec2(14.0, 8.0), galley, Color32::WHITE);
        } else {
            app.toast = None;
        }
    }
}

fn heat(t: f32) -> Color32 {
    // blue -> cyan -> green -> yellow -> red
    let stops = [(0.0, [0, 0, 255]), (0.25, [0, 255, 255]), (0.5, [0, 255, 0]), (0.75, [255, 255, 0]), (1.0, [255, 0, 0])];
    for w in stops.windows(2) {
        let (a, ca) = w[0];
        let (b, cb) = w[1];
        if t <= b {
            let k = ((t - a) / (b - a)).clamp(0.0, 1.0);
            let l = |x: i32, y: i32| (x as f32 + (y - x) as f32 * k) as u8;
            return Color32::from_rgb(l(ca[0], cb[0]), l(ca[1], cb[1]), l(ca[2], cb[2]));
        }
    }
    Color32::RED
}

fn project(app: &App, rect: Rect, p: Vec3) -> Option<Pos2> {
    let aspect = rect.width() / rect.height().max(1.0);
    let vp = app.camera.proj_matrix(app.vs.projection, app.vs.field_of_view, aspect) * app.camera.view_matrix(app.vs.projection);
    let c = vp * p.extend(1.0);
    if c.w <= 0.0 {
        return None;
    }
    let n = c.truncate() / c.w;
    if n.x.abs() > 1.2 || n.y.abs() > 1.2 {
        return None;
    }
    Some(Pos2::new(rect.left() + (n.x + 1.0) * 0.5 * rect.width(), rect.top() + (1.0 - n.y) * 0.5 * rect.height()))
}

fn minimap(app: &mut App, ui: &mut Ui, rect: Rect) {
    let size = 220.0;
    let mr = Rect::from_min_size(Pos2::new(rect.left() + 14.0, rect.bottom() - size - 60.0), Vec2::splat(size));
    let p = ui.painter_at(mr);
    p.rect_filled(mr, 6.0, Color32::from_black_alpha(185));
    let (mn, mx) = app.scene.world_bounds();
    let c = (mn + mx) * 0.5;
    let half = ((mx.x - mn.x).max(mx.z - mn.z) * 0.5 + 6.0).max(10.0);
    let inner = mr.shrink(10.0);
    let to_map = |x: f32, z: f32| Pos2::new(inner.center().x + (x - c.x) / half * inner.width() * 0.5, inner.center().y + (z - c.z) / half * inner.height() * 0.5);
    for o in &app.scene.objects {
        if o.name == "Ground" {
            continue;
        }
        let m = app.scene.object_matrix(o);
        let (a, b) = o.local_bounds;
        let wa = m.transform_point3(a);
        let wb = m.transform_point3(b);
        let r = Rect::from_two_pos(to_map(wa.x, wa.z), to_map(wb.x, wb.z));
        if r.width() * r.height() < 0.5 {
            continue;
        }
        let kind = app.scene.group(o.group).map(|g| g.kind).unwrap_or(GroupKind::Building);
        let col = match kind {
            GroupKind::Building => Color32::from_white_alpha(28),
            GroupKind::Asset => Color32::from_rgba_unmultiplied(110, 180, 90, 90),
            GroupKind::Imported => Color32::from_rgba_unmultiplied(120, 160, 230, 90),
        };
        p.rect_filled(r.intersect(inner), 0.0, col);
    }
    for v in &app.views {
        p.circle_filled(to_map(v.pose.pos.x, v.pose.pos.z), 3.0, if v.favorite { ACCENT } else { TEXT_DIM });
    }
    if app.video.keyframes.len() > 1 {
        let pts: Vec<Pos2> = app.video.keyframes.iter().map(|k| to_map(k.pose.pos.x, k.pose.pos.z)).collect();
        p.add(egui::Shape::line(pts, Stroke::new(1.5, Color32::from_rgb(0xCA, 0xE6, 0xFE))));
    }
    let cp = app.camera.pose;
    let pos = to_map(cp.pos.x, cp.pos.z);
    let f = cp.flat_forward();
    let dir = egui::vec2(f.x, f.z);
    let side = egui::vec2(-dir.y, dir.x);
    p.add(egui::Shape::convex_polygon(vec![pos + dir * 11.0, pos - dir * 6.0 + side * 6.0, pos - dir * 6.0 - side * 6.0], ACCENT, Stroke::new(1.0, Color32::BLACK)));
    p.text(mr.left_top() + egui::vec2(8.0, 6.0), Align2::LEFT_TOP, "MAP — click to move", FontId::proportional(10.5), TEXT_DIM);
    let resp = ui.interact(mr, egui::Id::new("minimap"), Sense::click());
    if resp.clicked() {
        if let Some(cl) = resp.interact_pointer_pos() {
            let wx = c.x + (cl.x - inner.center().x) / (inner.width() * 0.5) * half;
            let wz = c.z + (cl.y - inner.center().y) / (inner.height() * 0.5) * half;
            let mut to = CameraPose { pos: Vec3::new(wx, cp.pos.y, wz), ..cp };
            if app.camera.walk_mode {
                if let Some(h) = app.scene.raycast(Vec3::new(wx, 200.0, wz), -Vec3::Y, 400.0, true) {
                    to.pos.y = h.point.y + crate::camera::Camera::eye_height(&app.gs);
                }
            }
            app.camera.start_transition(to, 0.8);
        }
    }
}
