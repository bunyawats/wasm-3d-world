//! The heads-up display, drawn with egui: header stats, the control panel, the
//! followed creature's telemetry, the simulation inspector, hints and toasts.
//!
//! Port of the original's Tailwind HUD. egui is an *immediate-mode* GUI: there are
//! no widget objects to keep in sync. Every frame, `draw_hud` describes the whole UI
//! again from the current state, and each widget call returns what the user did
//! with it this frame (`if ui.button("X").clicked() { ... }`).

use std::time::Duration;

use bevy::{ecs::system::SystemParam, prelude::*};
use bevy_egui::{EguiContexts, EguiPlugin, EguiPrimaryContextPass, egui};
use sim::{Behaviour, Bird, Deer, Flight, MAX_BIRDS, MAX_DEER, daylight::Phase};

use crate::{
    birds::{FLOCK_SIZE, FlockState, bird_name},
    camera::CameraFocus,
    deer::{HerdState, deer_name},
    environment::WorldClock,
    terrain::{IslandEdit, IslandScenery, IslandTerrain, TreeCount},
};

/// How long a toast stays on screen.
const TOAST_SECONDS: f32 = 2.5;
/// The original's lighting presets.
const DAWN: f32 = 6.8;
const NOON: f32 = 13.5;
const NIGHT: f32 = 21.5;

pub struct HudPlugin;

impl Plugin for HudPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(EguiPlugin::default())
            .init_resource::<SimTimings>()
            .init_resource::<HudState>()
            .add_message::<Toast>()
            .add_systems(Update, collect_toasts)
            // egui UI systems run in their own schedule, after input is processed.
            .add_systems(EguiPrimaryContextPass, draw_hud);
    }
}

/// A short message shown at the bottom of the screen. Any system can send one
/// with a `MessageWriter<Toast>`.
#[derive(Message)]
pub struct Toast(pub String);

/// How long the simulation steps take, smoothed so the numbers are readable.
#[derive(Resource, Default)]
pub struct SimTimings {
    herd_us: f32,
    flocks_us: f32,
}

impl SimTimings {
    pub fn record_herd(&mut self, took: Duration) {
        smooth(&mut self.herd_us, took);
    }

    pub fn record_flocks(&mut self, took: Duration) {
        smooth(&mut self.flocks_us, took);
    }
}

/// Exponential moving average: each new sample moves the value 5% of the way.
fn smooth(average_us: &mut f32, sample: Duration) {
    let us = sample.as_secs_f32() * 1_000_000.0;
    *average_us += (us - *average_us) * 0.05;
}

/// The HUD's own state, which isn't simulation state.
#[derive(Resource)]
struct HudState {
    show_inspector: bool,
    /// The current toast and how many seconds it has left.
    toast: Option<(String, f32)>,
    /// Smoothed frames per second.
    fps: f32,
}

impl Default for HudState {
    fn default() -> Self {
        Self {
            show_inspector: false,
            toast: None,
            fps: 60.0,
        }
    }
}

/// Keeps the newest toast and counts its time down.
fn collect_toasts(mut toasts: MessageReader<Toast>, mut hud: ResMut<HudState>, time: Res<Time>) {
    // `last()` consumes the iterator and keeps only the final message.
    if let Some(Toast(text)) = toasts.read().last() {
        hud.toast = Some((text.clone(), TOAST_SECONDS));
    }
    let dt = time.delta_secs();
    // `as_mut()` turns `&mut Option<T>` into `Option<&mut T>`, so the tuple inside
    // can be edited in place.
    if let Some((_, left)) = hud.toast.as_mut() {
        *left -= dt;
    }
    if hud.toast.as_ref().is_some_and(|(_, left)| *left <= 0.0) {
        hud.toast = None;
    }
    if dt > 0.0 {
        hud.fps += (1.0 / dt - hud.fps) * 0.05;
    }
}

/// Everything the HUD reads or changes, bundled into one parameter.
///
/// A system function can only take so many parameters (and many are hard to read).
/// `#[derive(SystemParam)]` makes a struct that Bevy fills in exactly like separate
/// parameters. The `'w` (world) and `'s` (system state) lifetimes are the ones
/// `Res`, `ResMut` and friends carry.
#[derive(SystemParam)]
struct HudWorld<'w> {
    herd: ResMut<'w, HerdState>,
    flocks: ResMut<'w, FlockState>,
    clock: ResMut<'w, WorldClock>,
    focus: ResMut<'w, CameraFocus>,
    island: ResMut<'w, IslandTerrain>,
    tree_count: ResMut<'w, TreeCount>,
    scenery: Res<'w, IslandScenery>,
    timings: Res<'w, SimTimings>,
    edits: MessageWriter<'w, IslandEdit>,
    toasts: MessageWriter<'w, Toast>,
}

fn draw_hud(
    mut contexts: EguiContexts,
    mut hud: ResMut<HudState>,
    mut world: HudWorld,
    mut styled: Local<bool>,
) -> Result {
    // `?` returns early with an error if there's no egui context yet; Bevy logs it.
    let ctx = contexts.ctx_mut()?;
    if !*styled {
        apply_style(ctx);
        *styled = true;
    }

    header(ctx, &mut hud, &world);
    controls(ctx, &mut world);
    telemetry(ctx, &mut world);
    if hud.show_inspector {
        inspector(ctx, &mut hud, &world);
    }
    hints(ctx, &hud);
    Ok(())
}

/// Dark, translucent "glass" panels with cyan accents, like the original.
fn apply_style(ctx: &egui::Context) {
    ctx.all_styles_mut(|style| {
        let visuals = &mut style.visuals;
        *visuals = egui::Visuals::dark();
        visuals.window_fill = egui::Color32::from_rgba_unmultiplied(15, 23, 42, 220);
        visuals.panel_fill = visuals.window_fill;
        visuals.window_stroke = egui::Stroke::new(1.0, egui::Color32::from_rgb(51, 65, 85));
        visuals.window_corner_radius = egui::CornerRadius::same(10);
        visuals.selection.bg_fill = egui::Color32::from_rgb(8, 145, 178);
        visuals.hyperlink_color = egui::Color32::from_rgb(34, 211, 238);
        style.spacing.item_spacing = egui::vec2(6.0, 5.0);
    });
}

/// A floating card with no title bar, pinned to a corner.
fn card(id: &str, anchor: egui::Align2, offset: [f32; 2]) -> egui::Window<'_> {
    egui::Window::new(id)
        .title_bar(false)
        .resizable(false)
        .anchor(anchor, offset)
}

fn heading(ui: &mut egui::Ui, text: &str) {
    ui.add_space(4.0);
    ui.label(
        egui::RichText::new(text.to_uppercase())
            .small()
            .strong()
            .color(egui::Color32::from_rgb(34, 211, 238)),
    );
}

fn header(ctx: &egui::Context, hud: &mut HudState, world: &HudWorld) {
    card("brand", egui::Align2::LEFT_TOP, [12.0, 12.0]).show(ctx, |ui| {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("WILDVERSE").strong());
            ui.label(egui::RichText::new("Rust + Bevy").small().weak());
        });
    });

    card("stats", egui::Align2::RIGHT_TOP, [-12.0, 12.0]).show(ctx, |ui| {
        ui.horizontal(|ui| {
            ui.monospace(format!("{:>3.0} FPS", hud.fps));
            ui.separator();
            let sim_us = world.timings.herd_us + world.timings.flocks_us;
            ui.monospace(format!("sim {sim_us:>6.1} µs"));
            ui.separator();
            // Plain words: egui's built-in font has no deer emoji.
            ui.label(format!("{} deer", world.herd.0.deer.len()));
            ui.label(format!("{} birds", world.flocks.0.birds.len()));
            ui.separator();
            ui.toggle_value(&mut hud.show_inspector, "Inspector");
        });
    });
}

fn controls(ctx: &egui::Context, world: &mut HudWorld) {
    card("controls", egui::Align2::LEFT_TOP, [12.0, 56.0]).show(ctx, |ui| {
        ui.set_width(230.0);

        heading(ui, "Camera");
        let focus = *world.focus;
        ui.horizontal(|ui| {
            // Each button decides on a new focus; it's applied once, below, so the
            // focus resource is only marked changed when it really changes.
            let mut chosen = None;
            if ui
                .selectable_label(focus == CameraFocus::Free, "Free orbit")
                .clicked()
            {
                chosen = Some(CameraFocus::Free);
            }
            if ui
                .selectable_label(matches!(focus, CameraFocus::Deer(_)), "Deer")
                .clicked()
            {
                chosen = Some(CameraFocus::Deer(0));
            }
            if ui
                .selectable_label(matches!(focus, CameraFocus::Bird(_)), "Bird")
                .clicked()
            {
                chosen = Some(CameraFocus::Bird(0));
            }
            if ui.button("Next ▶").clicked() {
                chosen = Some(next_focus(
                    focus,
                    world.herd.0.deer.len(),
                    world.flocks.0.birds.len(),
                ));
            }
            if let Some(new) = chosen.filter(|new| *new != focus) {
                *world.focus = new;
            }
        });
        if ui.button("Startle the herd").clicked() {
            world.herd.0.startle();
            world
                .toasts
                .write(Toast("The herd bolts into a trot!".into()));
        }

        heading(ui, "Time of day");
        let clock = world.clock.0;
        ui.horizontal(|ui| {
            for (label, hours) in [("Dawn", DAWN), ("Noon", NOON), ("Night", NIGHT)] {
                if ui.button(label).clicked() {
                    world.clock.0.set(hours);
                }
            }
            ui.label(egui::RichText::new(clock_text(clock.hours)).monospace());
        });
        // Edit a copy, and write back only if the slider moved. Handing egui
        // `&mut world.clock.0.hours` directly would mark the clock as changed every
        // frame (see the change-detection note in `environment.rs`).
        let mut hours = clock.hours;
        if ui
            .add(egui::Slider::new(&mut hours, 0.0..=24.0).show_value(false))
            .changed()
        {
            world.clock.0.set(hours);
        }
        let mut running = clock.running;
        if ui
            .checkbox(&mut running, "Run the day/night cycle")
            .changed()
        {
            world.clock.0.running = running;
        }

        heading(ui, "Add wildlife & flora");
        ui.horizontal(|ui| {
            if ui.button("+ Deer").clicked() {
                let island = &world.island.0;
                let message = match world.herd.0.spawn(island, None) {
                    Some(id) => format!("{} joins the herd", deer_name(&world.herd.0.deer[id], id)),
                    None => format!("The herd is full ({MAX_DEER} deer)"),
                };
                world.toasts.write(Toast(message));
            }
            if ui.button("+ Bird flock").clicked() {
                let added = world.flocks.0.spawn_flock(FLOCK_SIZE);
                let message = if added == 0 {
                    format!("The sky is full ({MAX_BIRDS} birds)")
                } else {
                    format!("{added} birds take off")
                };
                world.toasts.write(Toast(message));
            }
        });
        ui.horizontal(|ui| {
            if ui.button("+ Tree").clicked() {
                world.edits.write(IslandEdit::PlantTree);
                world.toasts.write(Toast("Planted a tree".into()));
            }
            if ui.button("+ Crystal").clicked() {
                world.edits.write(IslandEdit::GrowCrystal);
                world
                    .toasts
                    .write(Toast("A crystal grows on the hillside".into()));
            }
        });

        heading(ui, "Terrain");
        // Rebuilding the island is heavy (a new mesh and ~4,000 scenery entities),
        // so sliders only trigger it when released, not on every step of a drag.
        let mut height = world.island.0.height_scale;
        let response = ui.add(
            egui::Slider::new(&mut height, 1.5..=5.0)
                .step_by(0.5)
                .text("Elevation"),
        );
        if response.changed() {
            world.island.0.height_scale = height;
        }
        let height_done = response.drag_stopped() || (response.changed() && !response.dragged());

        let mut trees = world.tree_count.0;
        let response = ui.add(
            egui::Slider::new(&mut trees, 60..=240)
                .step_by(20.0)
                .text("Forest"),
        );
        if response.changed() {
            world.tree_count.0 = trees;
        }
        let trees_done = response.drag_stopped() || (response.changed() && !response.dragged());

        let reseed = ui.button("Reseed the island").clicked();
        if reseed {
            world.island.0.seed_offset = world.herd.0.rng.range(0.0..100.0);
        }
        if height_done || trees_done || reseed {
            world.edits.write(IslandEdit::Rebuild);
            world.toasts.write(Toast("Island rebuilt".into()));
        }

        let scenery = &world.scenery.0;
        ui.add_space(4.0);
        ui.label(
            egui::RichText::new(format!(
                "{} trees · {} rocks · {} crystals",
                scenery.trees.len(),
                scenery.rocks.len(),
                scenery.crystals.len()
            ))
            .small()
            .weak(),
        );
    });
}

/// Cycles Free → each deer → each bird → Free.
fn next_focus(focus: CameraFocus, deer: usize, birds: usize) -> CameraFocus {
    match focus {
        CameraFocus::Free if deer > 0 => CameraFocus::Deer(0),
        CameraFocus::Deer(i) if i + 1 < deer => CameraFocus::Deer(i + 1),
        CameraFocus::Free | CameraFocus::Deer(_) if birds > 0 => CameraFocus::Bird(0),
        CameraFocus::Bird(i) if i + 1 < birds => CameraFocus::Bird(i + 1),
        _ => CameraFocus::Free,
    }
}

/// "1:30 PM"-style text for an hour value.
fn clock_text(hours: f32) -> String {
    let total_minutes = (hours * 60.0) as u32;
    let (h, m) = (total_minutes / 60 % 24, total_minutes % 60);
    let suffix = if h >= 12 { "PM" } else { "AM" };
    let h12 = if h % 12 == 0 { 12 } else { h % 12 };
    format!("{h12}:{m:02} {suffix}")
}

/// The card describing the creature the camera follows.
fn telemetry(ctx: &egui::Context, world: &mut HudWorld) {
    let (title, rows) = match *world.focus {
        CameraFocus::Free => return,
        CameraFocus::Deer(i) => match world.herd.0.deer.get(i) {
            Some(deer) => (deer_name(deer, i), deer_rows(deer)),
            None => return,
        },
        CameraFocus::Bird(i) => match world.flocks.0.birds.get(i) {
            Some(bird) => (bird_name(i), bird_rows(bird)),
            None => return,
        },
    };

    let mut stop = false;
    card("telemetry", egui::Align2::RIGHT_TOP, [-12.0, 56.0]).show(ctx, |ui| {
        ui.set_width(220.0);
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(title).strong());
            // Push the button to the right edge.
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                stop = ui
                    .small_button("Stop")
                    .on_hover_text("Stop following")
                    .clicked();
            });
        });
        egui::Grid::new("telemetry-grid")
            .num_columns(2)
            .show(ui, |ui| {
                for (label, value) in rows {
                    ui.label(egui::RichText::new(label).weak());
                    ui.monospace(value);
                    ui.end_row();
                }
            });
    });
    if stop {
        *world.focus = CameraFocus::Free;
    }
}

fn deer_rows(deer: &Deer) -> Vec<(&'static str, String)> {
    let slope = deer.ground_normal.y.clamp(-1.0, 1.0).acos().to_degrees();
    vec![
        ("Behaviour", behaviour_name(deer.behaviour).to_string()),
        ("Speed", format!("{:.2} m/s", deer.speed)),
        (
            "Altitude",
            format!("{:.1} m ({slope:.0}° slope)", deer.position.y),
        ),
        (
            "Stride",
            format!("{:.2} rad", deer.walk_cycle % std::f32::consts::TAU),
        ),
        (
            "Hips F/B",
            format!(
                "{:+.2} {:+.2}",
                deer.legs.front_left.hip, deer.legs.back_left.hip
            ),
        ),
        ("Neck", format!("{:+.2} rad", deer.pose.neck_pitch)),
    ]
}

fn bird_rows(bird: &Bird) -> Vec<(&'static str, String)> {
    vec![
        ("Flight", flight_name(bird.flight).to_string()),
        ("Speed", format!("{:.2} m/s", bird.velocity.length())),
        (
            "Altitude",
            format!(
                "{:.1} m ({:+.0}° bank)",
                bird.position.y,
                bird.bank.to_degrees()
            ),
        ),
        (
            "Wingbeat",
            format!("{:.2} rad", bird.flap_phase % std::f32::consts::TAU),
        ),
        (
            "Wings",
            format!("{:+.2} {:+.2}", bird.inner_wing, bird.outer_wing),
        ),
    ]
}

fn behaviour_name(behaviour: Behaviour) -> &'static str {
    match behaviour {
        Behaviour::Idle => "Idle",
        Behaviour::Grazing => "Grazing",
        Behaviour::Walking => "Walking",
        Behaviour::Trotting => "Trotting",
    }
}

fn flight_name(flight: Flight) -> &'static str {
    match flight {
        Flight::Flapping => "Flapping",
        Flight::Gliding => "Gliding",
    }
}

/// The simulation inspector: the successor to the original's "WASM memory
/// inspector". There's no hand-assembled linear memory any more, so it shows the
/// same idea directly: every creature's live state, and how big that state is.
fn inspector(ctx: &egui::Context, hud: &mut HudState, world: &HudWorld) {
    let herd = &world.herd.0;
    let flocks = &world.flocks.0;
    egui::Window::new("Simulation inspector")
        .open(&mut hud.show_inspector)
        // Lower right, below the telemetry card, so it doesn't cover the view.
        .default_pos([
            ctx.content_rect().right() - 512.0,
            ctx.content_rect().bottom() - 420.0,
        ])
        .default_size([500.0, 360.0])
        .show(ctx, |ui| {
            let deer_bytes = std::mem::size_of::<Deer>();
            let bird_bytes = std::mem::size_of::<Bird>();
            ui.label(format!(
                "Herd step {:.1} µs · flock step {:.1} µs · {} — sun {:?}",
                world.timings.herd_us,
                world.timings.flocks_us,
                clock_text(world.clock.0.hours),
                Phase::at(world.clock.0.hours),
            ));
            // `size_of` is the in-memory size of one value, known at compile time.
            // The original packed each deer into 96 bytes of WASM memory by hand.
            ui.label(
                egui::RichText::new(format!(
                    "Deer: {} × {deer_bytes} B = {} B   Birds: {} × {bird_bytes} B = {} B",
                    herd.deer.len(),
                    herd.deer.len() * deer_bytes,
                    flocks.birds.len(),
                    flocks.birds.len() * bird_bytes,
                ))
                .small()
                .weak(),
            );
            ui.separator();

            egui::ScrollArea::vertical().show(ui, |ui| {
                egui::CollapsingHeader::new("Deer")
                    .default_open(true)
                    .show(ui, |ui| {
                        egui::Grid::new("deer-grid").striped(true).show(ui, |ui| {
                            for header in ["#", "Kind", "Behaviour", "Position", "Speed", "Heading"]
                            {
                                ui.strong(header);
                            }
                            ui.end_row();
                            for (i, deer) in herd.deer.iter().enumerate() {
                                ui.monospace(format!("{}", i + 1));
                                ui.label(format!("{:?}", deer.kind));
                                ui.label(behaviour_name(deer.behaviour));
                                ui.monospace(vec3_text(deer.position));
                                ui.monospace(format!("{:.2}", deer.speed));
                                ui.monospace(format!(
                                    "{:+.2}",
                                    deer.heading.sin().atan2(deer.heading.cos())
                                ));
                                ui.end_row();
                            }
                        });
                    });
                ui.collapsing("Birds", |ui| {
                    egui::Grid::new("bird-grid").striped(true).show(ui, |ui| {
                        for header in ["#", "Flight", "Position", "Speed", "Bank", "Wings"] {
                            ui.strong(header);
                        }
                        ui.end_row();
                        for (i, bird) in flocks.birds.iter().enumerate() {
                            ui.monospace(format!("{}", i + 1));
                            ui.label(flight_name(bird.flight));
                            ui.monospace(vec3_text(bird.position));
                            ui.monospace(format!("{:.1}", bird.velocity.length()));
                            ui.monospace(format!("{:+.2}", bird.bank));
                            ui.monospace(format!(
                                "{:+.2} {:+.2}",
                                bird.inner_wing, bird.outer_wing
                            ));
                            ui.end_row();
                        }
                    });
                });
            });
        });
}

fn vec3_text(v: Vec3) -> String {
    format!("{:6.1} {:5.1} {:6.1}", v.x, v.y, v.z)
}

fn hints(ctx: &egui::Context, hud: &HudState) {
    egui::Area::new(egui::Id::new("hints"))
        .anchor(egui::Align2::CENTER_BOTTOM, [0.0, -12.0])
        .show(ctx, |ui| {
            egui::Frame::window(ui.style()).show(ui, |ui| {
                ui.label(
                    egui::RichText::new(
                        "Left-drag orbit · Right-drag pan · Scroll zoom · Click a deer or bird to follow · \
                         Keys: S startle, B birds, T clock, [ ] hour",
                    )
                    .small(),
                );
            });
        });

    if let Some((text, _)) = &hud.toast {
        egui::Area::new(egui::Id::new("toast"))
            .anchor(egui::Align2::CENTER_BOTTOM, [0.0, -52.0])
            .show(ctx, |ui| {
                egui::Frame::window(ui.style())
                    .fill(egui::Color32::from_rgba_unmultiplied(8, 145, 178, 230))
                    .show(ui, |ui| {
                        ui.label(
                            egui::RichText::new(text)
                                .strong()
                                .color(egui::Color32::WHITE),
                        );
                    });
            });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clock_text_reads_like_a_wall_clock() {
        assert_eq!(clock_text(0.0), "12:00 AM");
        assert_eq!(clock_text(13.5), "1:30 PM");
        assert_eq!(clock_text(12.0), "12:00 PM");
        assert_eq!(clock_text(23.99), "11:59 PM");
    }

    #[test]
    fn next_focus_cycles_through_deer_then_birds_then_free() {
        let mut focus = CameraFocus::Free;
        let mut seen = Vec::new();
        for _ in 0..6 {
            focus = next_focus(focus, 2, 2);
            seen.push(focus);
        }
        assert_eq!(
            seen,
            [
                CameraFocus::Deer(0),
                CameraFocus::Deer(1),
                CameraFocus::Bird(0),
                CameraFocus::Bird(1),
                CameraFocus::Free,
                CameraFocus::Deer(0),
            ]
        );
    }
}
