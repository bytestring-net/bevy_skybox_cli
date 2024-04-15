use bevy::{core_pipeline::Skybox,prelude::*};

// Create Bevy instance
fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_systems(Update,(rotate_camera, zoom_camera))
        .add_systems(Startup, setup)
        .run();
}

// Setup our scene
fn setup(mut commands: Commands, asset_server: Res<AssetServer>, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>) {
    
    // Spawn the showcase material
    commands.spawn(PbrBundle {
        mesh: meshes.add(Sphere::new(1.0)),
        material: materials.add(StandardMaterial {
            base_color: Color::WHITE,
            metallic: 0.8,
            perceptual_roughness: 0.1,
            ..default()
        }),
        ..default()
    });

    // Load the skybox
    let skybox_handle = asset_server.load("skybox.ktx2");

    // Spawn the camera
    commands.spawn((
        // Camera controls
        OrbitCamera { orbit: Vec3::ZERO, distance: 10.0 },
        Camera3dBundle::default(),

        // This is the skybox texture that camera sees
        Skybox {
            image: skybox_handle.clone(),
            brightness: 1000.0,
        },

        // This is used to cast light and reflections from skybox
        EnvironmentMapLight {
            diffuse_map: asset_server.load("diffuse_map.ktx2"),
            specular_map: asset_server.load("specular_map.ktx2"),
            //specular_map: skybox_handle.clone(), //Here should be "radiance.ktx2" but it's broken in 'skylight'. Using the "skybox.ktx2" works jsut fine
            intensity: 900.0,
        },
    ));

    // This should have effect on skybox too
    commands.insert_resource(AmbientLight {
        color: Color::rgb_u8(210, 220, 240),
        brightness: 1.0,
    });
}


// #===================================#
// #=== CAMERA LOGIC TO LOOK AROUND ===#

use bevy::input::mouse::{MouseMotion, MouseWheel};
#[derive(Component)]
struct OrbitCamera {
    pub orbit: Vec3,
    pub distance: f32,
}
fn rotate_camera(mut mouse_motion_events: EventReader<MouseMotion>, mouse_input: Res<ButtonInput<MouseButton>>, mut query: Query<(&OrbitCamera, &mut Transform)>) {
    let mut delta = Vec2::ZERO;
    if mouse_input.pressed(MouseButton::Left) {
        delta = mouse_motion_events.read().map(|e| e.delta).sum();
    }
    if mouse_input.just_pressed(MouseButton::Left) {
        delta = Vec2::ZERO;
    }
    for (camera, mut transform) in &mut query {

        // ROTATION 
        let (mut rx, mut ry, rz) = transform.rotation.to_euler(EulerRot::YXZ);
        rx += (-delta.x * 0.1).to_radians();
        ry += (-delta.y * 0.1).to_radians();
        ry = ry.clamp(-90_f32.to_radians(), 90_f32.to_radians());
        transform.rotation = Quat::from_euler(EulerRot::YXZ, rx, ry, rz);


        // ORBIT TRANSFORM
        let tx = camera.distance * rx.sin();
        let ty = camera.distance * rx.cos();
        let tz = camera.distance * ry.sin();

        let diff = camera.distance * ry.cos();
        let plane_ratio_decrease = (camera.distance - diff)/camera.distance;

        transform.translation = camera.orbit;
        transform.translation.x += tx * (1.0 - plane_ratio_decrease);
        transform.translation.z += ty * (1.0 - plane_ratio_decrease);
        transform.translation.y += -tz;
    }
}
fn zoom_camera(mut mouse_wheel_events: EventReader<MouseWheel>, mut query: Query<&mut OrbitCamera>) {
    let delta: f32 = mouse_wheel_events.read().map(|e| e.y).sum();
    for mut camera in &mut query {
        if delta != 0.0 { camera.distance -= (camera.distance*0.1)*delta }
    }
}
