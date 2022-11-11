#![allow(clippy::too_many_arguments)]
#![warn(
    clippy::wildcard_imports,
    clippy::string_add,
    clippy::string_add_assign,
    clippy::manual_ok_or,
    unused_lifetimes
)]

mod input;

use std::f32::consts::PI;

use bevy::log::{Level, LogSettings};
use bevy::prelude::{
    shape, App, Assets, Bundle, Camera2dBundle, ClearColor, Color, Commands, Component,
    ComputedVisibility, Entity, GlobalTransform, Handle, Input, KeyCode, Mesh, Quat, Query, Res,
    ResMut, SystemSet, Transform, Vec3, Visibility,
};
use bevy::sprite::{ColorMaterial, Mesh2dHandle};
use bevy::time::FixedTimestep;
use bevy::window::WindowDescriptor;
use bevy::DefaultPlugins;
use bevy_egui::{egui, EguiContext, EguiPlugin};
#[cfg(debug_assertions)]
use bevy_inspector_egui::WorldInspectorPlugin;
use bevy_spatial::{KDTreeAccess2D, KDTreePlugin2D, SpatialAccess};
use libm::sqrt;
use num::clamp;

use crate::input::{Camera, CursorPanState, CursorPlugin};

type BoidNNTree = KDTreeAccess2D<Boid>;

fn main() {
    #[cfg(target_arch = "wasm32")]
    {
        // Setting a panic hook specific for WASM builds
        std::panic::set_hook(Box::new(console_error_panic_hook::hook));
    }

    let mut app = App::new();

    // Constructing our app
    app.insert_resource(WindowDescriptor {
        title: "Boids".to_owned(),
        width: 1280.0,
        height: 720.0,
        scale_factor_override: Some(1.0),
        ..Default::default()
    })
    .insert_resource(LogSettings {
        level: Level::INFO,
        ..Default::default()
    })
    .insert_resource(ClearColor(Color::BLACK))
    .insert_resource(CursorPanState::default())
    .insert_resource(Options::default())
    .insert_resource(State::default())
    .add_plugins(DefaultPlugins)
    .add_plugin(KDTreePlugin2D::<Boid>::default())
    .add_plugin(CursorPlugin)
    .add_plugin(EguiPlugin)
    .add_startup_system(init_world)
    .add_system(input::handle_keyboard_pan_and_zoom)
    .add_system(input::handle_mouse_pan_and_zoom)
    .add_system(handle_play_pause)
    .add_system(cgol_gui)
    .add_system_set(
        SystemSet::new()
            .with_run_criteria(FixedTimestep::steps_per_second(15.0))
            .with_system(calculate_boid_color)
            .with_system(calculate_boid_rotation)
            .with_system(update_stats),
    )
    .add_system_set(
        SystemSet::new()
            .with_run_criteria(FixedTimestep::steps_per_second(60.0))
            .with_system(tick_boids),
    );

    #[cfg(debug_assertions)]
    {
        // Adding the world inspector if debug mode is enabled
        app.add_plugin(WorldInspectorPlugin::new());
    }

    #[cfg(target_arch = "wasm32")]
    {
        // If the target is WASM adding the resizer plugin to scale with browser window
        app.add_plugin(bevy_web_resizer::Plugin);
    }

    // Running our app
    app.run();
}

fn init_world(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    options: Res<Options>,
) {
    commands
        .spawn_bundle(Camera2dBundle {
            transform: Transform::default().with_scale(Vec3 {
                x: 0.1,
                y: 0.1,
                z: 1.0,
            }),
            ..Default::default()
        })
        .insert(Camera);

    let mesh: Mesh2dHandle = meshes
        .add(Mesh::from(shape::RegularPolygon::new(0.5, 3)))
        .into();

    for _ in 0..100 {
        spawn_boid(&mut commands, &mut materials, &options, mesh.clone());
    }
}

fn cgol_gui(
    mut egui_ctx: ResMut<EguiContext>,
    mut options: ResMut<Options>,
    mut state: ResMut<State>,
    mut background: ResMut<ClearColor>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    egui::Window::new("Options")
        .vscroll(true)
        .default_width(175.0)
        .resizable(false)
        .show(egui_ctx.ctx_mut(), |ui| {
            if ui
                .add_sized(
                    [175.0, 20.0],
                    egui::Button::new(if options.paused { "Play" } else { "Pause" }),
                )
                .clicked()
            {
                options.paused = !options.paused;
            };

            ui.separator();

            ui.horizontal(|ui| {
                ui.label("Visibility Range");
                ui.add(
                    egui::DragValue::new(&mut options.visibility_range).clamp_range(1.0..=120.0),
                );
            });

            ui.horizontal(|ui| {
                ui.label("Accuracy");
                ui.add(egui::DragValue::new(&mut options.accuracy).clamp_range(1..=120));
            });

            ui.separator();
            ui.checkbox(&mut options.separation, "Separation");

            ui.horizontal(|ui| {
                let max = options.visibility_range;
                ui.label("Separation Range");
                ui.add(egui::DragValue::new(&mut options.separation_range).clamp_range(1.0..=max));
            });

            ui.horizontal(|ui| {
                ui.label("Separation Impact");
                ui.add(
                    egui::DragValue::new(&mut options.separation_impact).clamp_range(0.001..=5.0),
                );
            });

            ui.separator();
            ui.checkbox(&mut options.alignment, "Alignment");

            ui.horizontal(|ui| {
                ui.label("Alignment Impact");
                ui.add(
                    egui::DragValue::new(&mut options.alignment_impact).clamp_range(0.001..=5.0),
                );
            });

            ui.separator();
            ui.checkbox(&mut options.cohesion, "Cohesion");

            ui.horizontal(|ui| {
                ui.label("Cohesion Impact");
                ui.add(
                    egui::DragValue::new(&mut options.cohesion_impact)
                        .fixed_decimals(4)
                        .clamp_range(0.0001..=5.0),
                );
            });

            ui.separator();
            ui.checkbox(&mut options.border, "Border");

            ui.horizontal(|ui| {
                ui.label("Border Size");
                ui.add(egui::DragValue::new(&mut options.border_size).clamp_range(10..=1000));
            });

            ui.horizontal(|ui| {
                ui.label("Border Impact");
                ui.add(egui::DragValue::new(&mut options.border_impact).clamp_range(0.05..=5.0));
            });

            ui.separator();
            ui.checkbox(&mut options.speed_limit, "Speed Limit");

            ui.horizontal(|ui| {
                ui.label("Minimum Speed");
                ui.add(
                    egui::DragValue::new(&mut options.min_speed)
                        .fixed_decimals(2)
                        .clamp_range(0.05..=5.0),
                );
            });

            ui.horizontal(|ui| {
                ui.label("Maximum Speed");
                ui.add(
                    egui::DragValue::new(&mut options.max_speed)
                        .fixed_decimals(2)
                        .clamp_range(0.05..=5.0),
                );
            });

            ui.separator();
            ui.label("Spawn more boids");

            ui.horizontal(|ui| {
                ui.add(egui::DragValue::new(&mut options.spawn_amount).clamp_range(1..=1000));

                if ui.button("Spawn").clicked() {
                    let mesh: Mesh2dHandle = meshes
                        .add(Mesh::from(shape::RegularPolygon::new(0.5, 3)))
                        .into();

                    for _ in 0..options.spawn_amount {
                        spawn_boid(&mut commands, &mut materials, &options, mesh.clone());
                    }
                }
            });

            ui.label(format!("Boid Count: {}", state.boid_count));

            ui.separator();
            ui.label("Visual Options");
            ui.checkbox(&mut options.calculate_rotation, "Calculate Rotation");
            ui.checkbox(&mut options.calculate_color, "Calculate Color");

            ui.horizontal(|ui| {
                if ui
                    .color_edit_button_rgb(&mut options.foreground_color)
                    .changed()
                {
                    state.prev_calculating_color = true;
                }

                if ui
                    .color_edit_button_rgb(&mut options.background_color)
                    .changed()
                {
                    let [r, g, b] = options.background_color;
                    background.0 = Color::rgb(r, g, b);
                }
            });

            ui.separator();
            ui.hyperlink_to("source code", "https://github.com/CatDevz/BadBoids");
        });
}

fn handle_play_pause(keyboard_input: Res<Input<KeyCode>>, mut options: ResMut<Options>) {
    if keyboard_input.just_pressed(KeyCode::Space) {
        options.paused = !options.paused
    }
}

struct Options {
    paused: bool,
    visibility_range: f32,
    accuracy: u32,

    separation: bool,
    separation_range: f32,
    separation_impact: f32,

    alignment: bool,
    alignment_impact: f32,

    cohesion: bool,
    cohesion_impact: f32,

    border: bool,
    border_size: i32,
    border_impact: f32,

    speed_limit: bool,
    min_speed: f32,
    max_speed: f32,

    spawn_amount: i32,

    calculate_rotation: bool,
    calculate_color: bool,
    foreground_color: [f32; 3],
    background_color: [f32; 3],
}

struct State {
    boid_count: u32,
    prev_calculating_color: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            paused: true,
            visibility_range: 10.0,
            accuracy: 100,
            separation: true,
            separation_range: 2.0,
            separation_impact: 0.05,
            alignment: true,
            alignment_impact: 0.05,
            cohesion: true,
            cohesion_impact: 0.0005,
            border: true,
            border_size: 50,
            border_impact: 0.02,
            speed_limit: true,
            min_speed: 0.3,
            max_speed: 0.2,
            spawn_amount: 100,
            calculate_rotation: true,
            calculate_color: true,
            foreground_color: [0.0, 1.0, 0.0915],
            background_color: [0.0, 0.0, 0.0],
        }
    }
}

impl Default for State {
    fn default() -> Self {
        Self {
            boid_count: 0,
            prev_calculating_color: true,
        }
    }
}

#[derive(Debug, Bundle, Default)]
struct BoidBundle {
    boid: Boid,

    // Will actually be used
    material: Handle<ColorMaterial>,
    transform: Transform,

    // Required for rendering
    mesh: Mesh2dHandle,
    visibility: Visibility,
    global_transform: GlobalTransform,
    computed_visibility: ComputedVisibility,
}

#[derive(Debug, Component, Default, Clone)]
struct Boid {
    flock_size: u32,
    vx: f32,
    vy: f32,
}

fn spawn_boid(
    commands: &mut Commands,
    materials: &mut ResMut<Assets<ColorMaterial>>,
    options: &Options,
    mesh: Mesh2dHandle,
) {
    let border_size = options.border_size as f32;
    commands.spawn_bundle(BoidBundle {
        mesh,
        transform: Transform::default()
            .with_translation(Vec3 {
                x: rand::random::<f32>() * border_size * 2.0 - border_size,
                y: rand::random::<f32>() * border_size * 2.0 - border_size,
                ..Default::default()
            })
            .with_scale(Vec3 {
                x: 0.7,
                y: 1.1,
                z: 1.0,
            }),
        material: {
            let [r, g, b] = options.foreground_color;
            materials.add(ColorMaterial::from(Color::rgb(r, g, b)))
        },
        ..Default::default()
    });
}

fn calculate_boid_color(
    query: Query<(&Boid, &Handle<ColorMaterial>)>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    mut state: ResMut<State>,
    options: Res<Options>,
) {
    if !options.calculate_color {
        if state.prev_calculating_color {
            state.prev_calculating_color = false;

            let [r, g, b] = options.foreground_color;
            for (_, mat_handle) in query.iter() {
                let Some(mut material) = materials.get_mut(mat_handle) else {
                    continue;
                };

                material.color = Color::rgb(r, g, b);
            }
        }

        return;
    }

    if !state.prev_calculating_color {
        state.prev_calculating_color = true;
    }

    for (boid, mat_handle) in query.iter() {
        // Get the material using the handle
        let Some(mut material) = materials.get_mut(mat_handle) else {
            continue;
        };

        // Set the material color based on the number of boids in its flock
        material.color = Color::hsl(clamp(boid.flock_size * 5, 0, 140) as f32, 1.0, 0.5);
    }
}

fn calculate_boid_rotation(mut query: Query<(&Boid, &mut Transform)>, options: Res<Options>) {
    if !options.calculate_rotation {
        return;
    }

    for (boid, mut transform) in query.iter_mut() {
        let angle = libm::atan2f(boid.vy, boid.vx);
        transform.rotation = Quat::from_rotation_z(angle - 90.0 * (PI / 180.0));
    }
}

fn update_stats(mut state: ResMut<State>, query: Query<&Boid>) {
    state.boid_count = query.iter().len() as u32;
}

fn tick_boids(
    mut query: Query<(Entity, &mut Boid, &mut Transform)>,
    options: Res<Options>,
    tree: Res<BoidNNTree>,
) {
    if options.paused {
        return;
    }

    let boid_iter = query.iter();
    let mut updated_boids = Vec::<(Entity, Boid, Transform)>::with_capacity(boid_iter.len());

    for (entity, boid, transform) in boid_iter {
        let mut boid = boid.clone();
        let mut transform = *transform;

        // Setting some basic variables
        let pos = transform.translation;

        let mut close_dx = 0.0;
        let mut close_dy = 0.0;
        let mut flock_vx_sum = 0.0;
        let mut flock_vy_sum = 0.0;
        let mut flock_x_sum = 0.0;
        let mut flock_y_sum = 0.0;

        // Getting the flock
        let flock = tree.within_distance(pos, options.visibility_range);
        let flock_size = flock.len() as u32;

        // Copying some debug info
        boid.flock_size = flock_size;

        // Looping through every other boid in the flock
        let flock = flock
            .into_iter()
            .filter_map(|it| Some((it.0, query.get(it.1).ok()?.1)))
            .collect::<Vec<_>>();

        for (i, (other_pos, other_boid)) in flock.into_iter().enumerate() {
            if i as u32 > options.accuracy {
                break;
            }

            // Getting the distance between our boid and the other
            let Vec3 { x: dx, y: dy, z: _ } = pos - other_pos;

            // Applying separation if boids are close enough and cohesion if they are far
            // enough
            if (dx * dx + dy * dy) < options.separation_range && options.separation {
                close_dx += dx;
                close_dy += dy;
            } else if options.cohesion {
                flock_x_sum += other_pos.x;
                flock_y_sum += other_pos.y;
            }

            // Applying alignment if enabled
            if options.alignment {
                flock_vx_sum += other_boid.vx;
                flock_vy_sum += other_boid.vy;
            }
        }

        if flock_size > 0 {
            let flock_vx_avrg = flock_vx_sum / flock_size as f32;
            let flock_vy_avrg = flock_vy_sum / flock_size as f32;
            boid.vx += (flock_vx_avrg - boid.vx) * options.alignment_impact;
            boid.vy += (flock_vy_avrg - boid.vy) * options.alignment_impact;

            let flock_x_avrg = flock_x_sum / flock_size as f32;
            let flock_y_avrg = flock_y_sum / flock_size as f32;
            boid.vx += (flock_x_avrg - pos.x) * 0.0005;
            boid.vy += (flock_y_avrg - pos.y) * 0.0005;
        }

        boid.vx += close_dx * options.separation_impact;
        boid.vy += close_dy * options.separation_impact;

        // Bounding boxes
        if options.border {
            let size = options.border_size as f32;
            if transform.translation.x > size {
                boid.vx -= options.border_impact;
            }

            if transform.translation.x < -size {
                boid.vx += options.border_impact;
            }

            if transform.translation.y > size {
                boid.vy -= options.border_impact;
            }

            if transform.translation.y < -size {
                boid.vy += options.border_impact;
            }
        }

        // Speed limits
        if options.speed_limit {
            let speed = sqrt((boid.vx * boid.vx + boid.vy * boid.vy) as f64) as f32;

            if speed < options.min_speed {
                boid.vx = (boid.vx / speed) * options.min_speed;
                boid.vy = (boid.vy / speed) * options.min_speed;
            }

            if speed > options.max_speed {
                boid.vx = (boid.vx / speed) * options.max_speed;
                boid.vy = (boid.vy / speed) * options.max_speed;
            }
        }

        // Calculating the new position based on the velocity of the boid
        transform.translation.x += boid.vx;
        transform.translation.y += boid.vy;

        // Adding the updated boid
        updated_boids.push((entity, boid, transform));
    }

    // Looping through every boid and applying it to its actual entity
    for (entity, updated_boid, updated_transform) in updated_boids.into_iter() {
        let Ok((_, mut boid, mut transform)) = query.get_mut(entity) else { continue; };

        // Updating the boid itself
        boid.flock_size = updated_boid.flock_size;
        boid.vx = updated_boid.vx;
        boid.vy = updated_boid.vy;

        // Updating the transform
        transform.translation = updated_transform.translation;
        transform.rotation = updated_transform.rotation;
        transform.scale = updated_transform.scale;
    }
}
