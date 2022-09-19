// Bevy code commonly triggers these lints and they may be important signals
// about code quality. They are sometimes hard to avoid though, and the CI
// workflow treats them as errors, so this allows them throughout the project.
// Feel free to delete this line.
#![allow(clippy::too_many_arguments, clippy::type_complexity)]

use std::{collections::HashMap, marker, time::Duration};

use bevy::prelude::*;
use bevy_flycam::{FlyCam, NoCameraPlayerPlugin};
use bevy_inspector_egui::{
    widgets::ResourceInspector, Inspectable, InspectorPlugin, RegisterInspectable,
};
use bevy_mod_picking::{DefaultPickingPlugins, PickableBundle, PickingCameraBundle, PickingEvent};

#[derive(Inspectable, Component)]
pub struct Name {
    pub name: String,
}

#[derive(Inspectable, Default)]
pub struct InspectTarget {
    target: Option<Entity>,
}

#[derive(Component)]
pub struct DebugMarker;

#[derive(Inspectable, Component)]
pub struct Celestial {
    mass: f32,
    velocity: Vec3,
}

#[derive(Inspectable)]
pub struct Universe {
    active: bool,
    gravitational_constant: f32,
    update_frequency_ms: u64,
    debug_steps: u32,
}

#[derive(Inspectable, Default)]
struct UniverseInspector {
    universe: ResourceInspector<Universe>,
}

impl Default for Universe {
    fn default() -> Self {
        Self {
            gravitational_constant: 0.05,
            active: false,
            update_frequency_ms: 34,
            debug_steps: 1000,
        }
    }
}

pub struct ReflectionPlugin;

impl Plugin for ReflectionPlugin {
    fn build(&self, app: &mut App) {
        app.register_inspectable::<Name>()
            .register_inspectable::<Celestial>()
            .register_inspectable::<Universe>();
    }
}

pub struct UniverseTimer {
    timer: Timer,
}

#[derive(Copy, Clone)]
pub struct UniverseTickEvent(f32);

fn handle_delta(
    mut universe_timer: ResMut<UniverseTimer>,
    constants: Res<Universe>,
    time: Res<Time>,
    mut universe_tick_writer: EventWriter<UniverseTickEvent>,
) {
    if universe_timer.timer.tick(time.delta()).finished() {
        universe_timer.timer.reset();
        universe_tick_writer.send(UniverseTickEvent(
            1.0 / constants.update_frequency_ms as f32,
        ))
    }
}

fn update_celestial_bodies_event_reader(
    mut universe_tick_reader: EventReader<UniverseTickEvent>,
    constants: Res<Universe>,
    keys: Res<Input<KeyCode>>,
    query: Query<(Entity, &mut Celestial, &mut Transform), Without<DebugMarker>>,
) {
    let tick = if let Some(tick) = universe_tick_reader.iter().last() {
        if constants.active {
            Some(*tick)
        } else {
            None
        }
    } else if keys.just_pressed(KeyCode::T) {
        // Hack to force tick
        let event = UniverseTickEvent(1.0 / constants.update_frequency_ms as f32);
        Some(event)
    } else {
        None
    };
    if let Some(tick) = tick {
        update_celestial_bodies(tick, constants, query);
    }
}

fn update_celestial_bodies(
    tick: UniverseTickEvent,
    constants: Res<Universe>,
    mut query: Query<(Entity, &mut Celestial, &mut Transform), Without<DebugMarker>>,
) {
    let celstial_map = build_celestial_maps(&query);
    let mut velocity_map = calculate_celestial_velocities(&tick, &constants, &celstial_map);
    for (this, mut body, mut transform) in query.iter_mut() {
        body.velocity = velocity_map
            .remove(&this)
            .expect("could not find velocity for entity");

        transform.translation += body.velocity * tick.0;
    }
}

struct CelestialBundle {
    pos: Vec3,
    vel: Vec3,
    mass: f32,
}

struct CelestialMap {
    map: HashMap<Entity, CelestialBundle>,
}

fn build_celestial_maps(
    celestial_bodies: &Query<(Entity, &mut Celestial, &mut Transform), Without<DebugMarker>>,
) -> CelestialMap {
    let mut map = HashMap::new();

    for (entity, body, transform) in celestial_bodies.iter() {
        map.insert(
            entity,
            CelestialBundle {
                pos: transform.translation,
                vel: body.velocity,
                mass: body.mass,
            },
        );
    }

    CelestialMap { map }
}

fn calculate_celestial_velocities(
    tick: &UniverseTickEvent,
    constants: &Res<Universe>,
    celestial_map: &CelestialMap,
) -> HashMap<Entity, Vec3> {
    let mut velocity_map = HashMap::new();
    for (this, bundle) in celestial_map.map.iter() {
        let mut current_velocity = bundle.vel;
        for (that, other_bundle) in celestial_map.map.iter() {
            if this == that {
                continue;
            }

            current_velocity += calculate_dt_velocity(
                constants.gravitational_constant,
                bundle.pos,
                other_bundle.pos,
                other_bundle.mass,
            ) * tick.0;
        }
        velocity_map.insert(*this, current_velocity);
    }
    velocity_map
}

fn calculate_dt_velocity(
    gravitational_constant: f32,
    this_translation: Vec3,
    that_translation: Vec3,
    that_mass: f32,
) -> Vec3 {
    let square_distance = this_translation.distance_squared(that_translation);
    let force_direction = (that_translation - this_translation).normalize();
    force_direction * gravitational_constant * that_mass / square_distance
}

fn generate_debug_points(
    key: Res<Input<KeyCode>>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    constants: Res<Universe>,
    celestial_bodies: Query<(Entity, &mut Celestial, &mut Transform), Without<DebugMarker>>,
    mut old_debug_markers: Query<(Entity, &mut Transform), With<DebugMarker>>,
    material: Query<&Handle<StandardMaterial>>,
) {
    if !key.just_pressed(KeyCode::D) {
        return;
    }
    if key.just_pressed(KeyCode::C) {
        for (entity, _) in old_debug_markers.iter() {
            commands.entity(entity).despawn();
        }
        return;
    }
    let mut celestial_map = build_celestial_maps(&celestial_bodies);
    let mut positions = Vec::new();
    let tick = UniverseTickEvent(1.0 / constants.update_frequency_ms as f32);
    for _ in 0..constants.debug_steps {
        let velocities = calculate_celestial_velocities(&tick, &constants, &celestial_map);
        for (entity, bundle) in celestial_map.map.iter_mut() {
            let velocity = velocities.get(entity).unwrap();
            bundle.vel = *velocity;
            bundle.pos += bundle.vel * tick.0;
            positions.push((*entity, bundle.pos));
        }
    }

    for (marker, mut marker_transform) in old_debug_markers.iter_mut() {
        let pos = positions.pop();
        if let Some((entity, pos)) = pos {
            let material = material.get(entity).unwrap().clone();
            marker_transform.translation.x = pos.x;
            marker_transform.translation.y = pos.y;
            marker_transform.translation.z = pos.z;
            commands.entity(marker).insert(material.clone());
        } else {
            commands.entity(marker).despawn();
        }
    }

    for (entity, position) in positions {
        generate_debug_marker(
            &mut commands,
            &mut meshes,
            material.get(entity).unwrap().clone(),
            position,
        );
    }
}

fn generate_debug_marker(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    material: Handle<StandardMaterial>,
    position: Vec3,
) {
    commands
        .spawn_bundle(PbrBundle {
            mesh: meshes.add(Mesh::from(shape::Icosphere {
                radius: 0.01,
                subdivisions: 1,
            })),
            material,
            transform: Transform::from_xyz(position.x, position.y, position.z),
            ..Default::default()
        })
        .insert(DebugMarker);
}

fn universe_toggle(key: Res<Input<KeyCode>>, mut universe: ResMut<Universe>) {
    if key.just_pressed(KeyCode::U) {
        universe.active = !universe.active;
    }
}

pub struct UniversePlugin;

impl Plugin for UniversePlugin {
    fn build(&self, app: &mut App) {
        let universe = Universe::default();

        let timer = UniverseTimer {
            timer: Timer::new(Duration::from_millis(universe.update_frequency_ms), true),
        };
        app.insert_resource(Universe::default())
            .insert_resource(timer)
            .add_event::<UniverseTickEvent>()
            .add_system(handle_delta)
            .add_system(universe_toggle)
            .add_system(update_celestial_bodies_event_reader);
    }
}

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugin(InspectorPlugin::<InspectTarget>::new())
        .add_plugin(InspectorPlugin::<UniverseInspector>::new())
        .add_plugin(ReflectionPlugin)
        .add_plugin(NoCameraPlayerPlugin)
        .add_plugin(UniversePlugin)
        .add_plugins(DefaultPickingPlugins)
        .add_startup_system(setup)
        .add_startup_system(setup_universe)
        .add_system(handle_input)
        .add_system(pick_active)
        .add_system(generate_debug_points)
        .run();
}

fn handle_input(
    mut commands: Commands,
    query: Query<Entity, With<Celestial>>,
    meshes: ResMut<Assets<Mesh>>,
    materials: ResMut<Assets<StandardMaterial>>,
    inspector: ResMut<InspectTarget>,
    keys: Res<Input<KeyCode>>,
) {
    if keys.just_pressed(KeyCode::R) {
        for entity in query.iter() {
            commands.entity(entity).despawn();
        }
        setup_universe(commands, meshes, materials, inspector);
    }
}

fn setup_universe(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut inspector: ResMut<InspectTarget>,
) {
    commands
        .spawn_bundle(PbrBundle {
            mesh: meshes.add(Mesh::from(shape::Icosphere {
                radius: 0.1,
                subdivisions: 3,
            })),
            material: materials.add(Color::RED.into()),
            transform: Transform::from_xyz(0.0, 0.0, 0.0),
            ..Default::default()
        })
        .insert(Name {
            name: "Left".to_string(),
        })
        .insert(Celestial {
            mass: 100.0,
            velocity: Vec3::ZERO,
        })
        .insert_bundle(PickableBundle::default());

    commands
        .spawn_bundle(PbrBundle {
            mesh: meshes.add(Mesh::from(shape::Icosphere {
                radius: 0.1,
                subdivisions: 3,
            })),
            material: materials.add(Color::CYAN.into()),
            transform: Transform::from_xyz(10.0, 0.1, 0.0),
            ..Default::default()
        })
        .insert(Name {
            name: "Right".to_string(),
        })
        .insert(Celestial {
            mass: 1.0,
            velocity: Vec3::new(0.0, 0.0, 100.0),
        })
        .insert_bundle(PickableBundle::default());

    commands
        .spawn_bundle(PbrBundle {
            mesh: meshes.add(Mesh::from(shape::Icosphere {
                radius: 0.1,
                subdivisions: 3,
            })),
            material: materials.add(Color::GREEN.into()),
            transform: Transform::from_xyz(0.0, 0.3, -10.0),
            ..Default::default()
        })
        .insert(Name {
            name: "Right".to_string(),
        })
        .insert(Celestial {
            mass: 1.0,
            velocity: Vec3::new(100.0, 0.0, 0.0),
        })
        .insert_bundle(PickableBundle::default());
}

fn setup(mut commands: Commands) {
    commands
        .spawn_bundle(Camera3dBundle {
            transform: Transform::from_xyz(0.0, 5.0, 0.0).looking_at(Vec3::ZERO, Vec3::Y),
            ..default()
        })
        .insert(FlyCam)
        .insert_bundle(PickingCameraBundle::default());
    commands.insert_resource(AmbientLight {
        color: Color::WHITE,
        brightness: 100.0,
    });
}

pub fn pick_active(mut events: EventReader<PickingEvent>, mut inspector: ResMut<InspectTarget>) {
    for event in events.iter() {
        match event {
            PickingEvent::Clicked(e) => {
                inspector.target = Some(*e);
            }
            _ => {}
        }
    }
}
