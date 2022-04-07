use bevy_ecs::prelude::*;
use rapier3d::prelude::*;

use rapier3d::na::*;

use std::path::Path;
use std::fs::File;
use std::io::Read;
use std::time::Instant;

use glfw::{Action, Context, Key};
use glam::f32::{ Mat4, Vec2, Vec3 };
use wgpu::util::DeviceExt;

use bevy_ecs::world::EntityRef;

use render::{ GpuDevice, Mesh, Vertex, UniformBuffer, Texture };

extern crate glfw;

const UNIT_TO_METERS: f32 = 4.0;

mod render;

#[derive(Debug)]
struct GameState {
    /// Set to true if the game should close
    close: bool,

    up: bool,
    down: bool,
    left: bool,
    right: bool,
    jump: bool,

    first_mouse: bool,
    last_mouse_x: f32,
    last_mouse_y: f32,

    yaw: f32,
    pitch: f32,
}

impl GameState {
    fn new() -> Self {
        Self {
            close: false,

            up: false,
            down: false,
            left: false,
            right: false,
            jump: false,

            first_mouse: true,
            last_mouse_x: 0.0,
            last_mouse_y: 0.0,

            yaw: 90.0,
            pitch: 0.0,
        }
    }
}

struct Sector {
    floor_mesh: Mesh,
    ceiling_mesh: Mesh,
    wall_mesh: Mesh,

    floor_collider: Option<Collider>,
    wall_collider: Option<Collider>,
}

struct Map {
    sectors: Vec<Sector>,
}

fn load_map<P>(filename: P, gpu_device: &GpuDevice) -> Option<Map>
    where P: AsRef<Path>
{
    let mut file = File::open(filename).ok()?;
    let mut data = Vec::new();
    file.read_to_end(&mut data).ok()?;

    let mime_map = mime::Map::deserialize(&data).unwrap();

    let mut sectors = Vec::new();

    let mut index = 0;
    for sector in &mime_map.sectors {
        let generate_mesh = |m: &mime::Mesh| {
            let mut vertex_buffer = Vec::new();

            for v in &m.vertex_buffer {
                vertex_buffer.push(Vertex {
                    position: [v.x, v.y, v.z],
                    color: [v.color[0], v.color[1], v.color[2]],
                });
            }

            let index_buffer = &m.index_buffer;

            Mesh::from_data(gpu_device, &vertex_buffer, index_buffer)
        };

        let generate_collider = |m: &mime::Mesh| {
            let mut points = Vec::new();
            let mut indices = Vec::new();

            for v in &m.vertex_buffer {
                points.push(Point3::new(v.x / UNIT_TO_METERS,
                                        v.y / UNIT_TO_METERS,
                                        v.z / UNIT_TO_METERS));
            }

            for i in 0..m.index_buffer.len() / 3 {
                let p1 = m.index_buffer[i * 3 + 0];
                let p2 = m.index_buffer[i * 3 + 1];
                let p3 = m.index_buffer[i * 3 + 2];

                indices.push([p1, p2, p3]);
            }

            ColliderBuilder::trimesh(points, indices).build()
        };

        let floor_collider = generate_collider(&sector.floor_mesh);
        let wall_collider = generate_collider(&sector.wall_mesh);

        let floor_mesh = generate_mesh(&sector.floor_mesh);
        let ceiling_mesh = generate_mesh(&sector.ceiling_mesh);
        let wall_mesh = generate_mesh(&sector.wall_mesh);

        sectors.push(Sector {
            floor_mesh,
            ceiling_mesh,
            wall_mesh,

            floor_collider: Some(floor_collider),
            wall_collider: Some(wall_collider),
        });

        index += 1;
    }

    let map = Map {
        sectors,
    };

    Some(map)
}


struct DeltaTime(f32);

#[derive(Component, Debug)]
#[repr(transparent)]
struct Position(Vec3);

#[derive(Component, Debug)]
struct Camera {
    direction: Vec3,
    up: Vec3,
}

#[derive(Component)]
struct Player {
    collider_handle: ColliderHandle,
    body_handle: RigidBodyHandle,
    speed: f32,
}

fn update_camera(mut query: Query<(&mut Position, &mut Camera, &Player)>,
                 game_state: Res<GameState>,
                 mut bodies: ResMut<RigidBodySet>,
                 dt: Res<DeltaTime>)
{
    for (mut position, mut camera, player) in query.iter_mut() {
        let mut body = bodies.get_mut(player.body_handle).unwrap();

        let pitch = game_state.pitch;
        let yaw = game_state.yaw;

        let direction = Vec3::new(
            yaw.to_radians().cos() * pitch.to_radians().cos(),
            pitch.to_radians().sin(),
            yaw.to_radians().sin() * pitch.to_radians().cos());

        camera.direction = direction.normalize();

        if game_state.up {
            let dir = camera.direction;
            let force = dir * 20.0;
            let force = vector![force.x, 0.0, force.z];

            body.set_linvel(force, true);
        }

        if game_state.jump {
            let dir = Vec3::new(0.0, 1.0, 0.0);
            let force = dir * 20.0;
            let force = vector![force.x, force.y, force.z];

            body.set_linvel(force, true);
        }
        /*
        let speed = player.speed;

        if game_state.up {
            position.0 += (speed * camera.direction) * dt.0;
        }

        if game_state.down {
            position.0 -= (speed * camera.direction) * dt.0;
        }

        if game_state.right {
            position.0 -= (camera.direction.cross(camera.up).normalize() * speed) * dt.0;
        }

        if game_state.left {
            position.0 += (camera.direction.cross(camera.up).normalize() * speed) * dt.0;
        }
        */
    }
}

fn update_player_physics(mut query: Query<(&mut Position, &Player)>,
                         bodies: Res<RigidBodySet>)
{
    for (mut position, player) in query.iter_mut() {
        let body = bodies.get(player.body_handle).unwrap();
        let x = body.translation().x;
        let y = body.translation().y;
        let z = body.translation().z;
        let new_pos = Vec3::new(x, y, z);

        position.0 = new_pos * UNIT_TO_METERS;
    }
}

fn generate_view_matrix(camera: EntityRef) -> Mat4 {
    let pos = camera.get::<Position>()
        .expect("Camera dosen't have Position Component");
    let controller = camera.get::<Camera>()
        .expect("Camera dosen't have Camera Controller Component");

    let pos = pos.0 + Vec3::new(0.0, 20.0, 0.0);

    Mat4::look_at_lh(pos, pos+ controller.direction, controller.up)
}

fn main() {
    env_logger::init();

    let mut glfw = glfw::init(glfw::FAIL_ON_ERRORS).unwrap();
    glfw.window_hint(glfw::WindowHint::ClientApi(glfw::ClientApiHint::NoApi));

    let (mut window, events) =
        glfw.create_window(1280, 720,
                           "Hello this is window",
                           glfw::WindowMode::Windowed)
            .expect("Failed to create GLFW window.");

    window.set_key_polling(true);
    window.set_cursor_pos_polling(true);
    window.set_cursor_mode(glfw::CursorMode::Disabled);

    let (window_width, window_height) = window.get_framebuffer_size();

    let aspect_ratio = window_width as f32 / window_height as f32;
    let projection_matrix = Mat4::perspective_lh(90.0f32.to_radians(), aspect_ratio, 0.1, 2000.0);
    let view_matrix = Mat4::IDENTITY;
    let model_matrix = Mat4::from_scale(Vec3::new(1.0, 1.0, 1.0));

    let mut uniform_buffer = UniformBuffer::new(projection_matrix, view_matrix, model_matrix);

    let (gpu_device, surface) = pollster::block_on(GpuDevice::new_for_window(&window)).unwrap();

    let mut map = load_map("/home/nanoteck137/doom1.mup", &gpu_device)
        .expect("Failed to load map");

    let shader = gpu_device.device.create_shader_module(&wgpu::include_wgsl!("shader.wgsl"));

    let uniform_buffer_handle = gpu_device.device.create_buffer_init(
        &wgpu::util::BufferInitDescriptor {
            label: Some("Uniform Buffer"),
            contents: bytemuck::cast_slice(&[uniform_buffer]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        }
    );

    let uniform_buffer_bind_group_layout = gpu_device.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }
        ],
        label: Some("uniform_buffer_bind_group_layout"),
    });

    let uniform_buffer_bind_group = gpu_device.device.create_bind_group(&wgpu::BindGroupDescriptor {
        layout: &uniform_buffer_bind_group_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer_handle.as_entire_binding(),
            }
        ],
        label: Some("uniform_buffer_bind_group"),
    });

    let pipeline_layout = render::PipelineLayout::builder()
        .bind_group_layout(&uniform_buffer_bind_group_layout)
        .build(&gpu_device);

    let pipeline = render::RenderPipeline::builder()
        .fragment_shader(&shader)
        .vertex_shader(&shader)
        .depth_stencil(true)
        .cull_mode(wgpu::Face::Back)
        .build(&gpu_device, &surface, &pipeline_layout);


    let depth_texture = Texture::create_depth_texture(&gpu_device, surface.config().width, surface.config().height);

    let mut rigid_body_set = RigidBodySet::new();
    let mut collider_set = ColliderSet::new();

    //let sector = &mut map.sectors[38]; {
    for sector in map.sectors.iter_mut() {
        collider_set.insert(sector.floor_collider.take().unwrap());
        collider_set.insert(sector.wall_collider.take().unwrap());
    }

    let player_rigidbody = RigidBodyBuilder::new_dynamic()
        .translation(vector![1077.0 / UNIT_TO_METERS, 20.0 / UNIT_TO_METERS, -3600.0 / UNIT_TO_METERS])
        .build();
    let player_rigidbody = rigid_body_set.insert(player_rigidbody);

    let player_collider = ColliderBuilder::cuboid(1.0, 4.0, 1.0)
        .build();
    let player_collider =
        collider_set.insert_with_parent(player_collider,
                                        player_rigidbody,
                                        &mut rigid_body_set);

    let mut world = World::default();

    world.insert_resource(GameState::new());
    world.insert_resource(DeltaTime(0.0));
    world.insert_resource(map);
    world.insert_resource(rigid_body_set);

    let player_id = world.spawn()
        .insert(Position(Vec3::new(1077.0, 460.0, -3600.0)))
        .insert(Camera {
            direction: Vec3::new(0.0, 0.0, 1.0),
            up: Vec3::new(0.0, 1.0, 0.0),
        })
        .insert(Player {
            speed: 100.0,
            collider_handle: player_collider,
            body_handle: player_rigidbody,
        })
        .id();

    let mut schedule = Schedule::default();

    let stage = SystemStage::single_threaded()
        .with_system(update_player_physics)
        .with_system(update_camera);
    schedule.add_stage("update", stage);

    let gravity = vector![0.0, -9.81, 0.0];
    let integration_parameters = IntegrationParameters::default();
    let mut physics_pipeline = PhysicsPipeline::new();
    let mut island_manager = IslandManager::new();
    let mut broad_phase = BroadPhase::new();
    let mut narrow_phase = NarrowPhase::new();
    let mut joint_set = JointSet::new();
    let mut ccd_solver = CCDSolver::new();
    let physics_hooks = ();
    let event_handler = ();

    let time = Instant::now();
    let mut past = 0.0;

    let mut close_game = false;
    while !close_game {
        let now = time.elapsed().as_secs_f32();
        let dt = now - past;
        past = now;

        {
            let mut dtr = world.get_resource_mut::<DeltaTime>().unwrap();
            dtr.0 = dt;
        }

        {
            let mut game_state = world.get_resource_mut::<GameState>().unwrap();
            glfw.poll_events();
            for (_, event) in glfw::flush_messages(&events) {
                handle_window_event(&mut game_state, event);
            }
        }

        {
            let game_state = world.get_resource_mut::<GameState>().unwrap();
            if game_state.close {
                close_game = true;
            }
        }

        let mut rigid_body_set = world.get_resource_mut::<RigidBodySet>()
            .unwrap();

        physics_pipeline.step(
            &gravity,
            &integration_parameters,
            &mut island_manager,
            &mut broad_phase,
            &mut narrow_phase,
            &mut rigid_body_set,
            &mut collider_set,
            &mut joint_set,
            &mut ccd_solver,
            &physics_hooks,
            &event_handler,
        );

        schedule.run(&mut world);

        let player = world.entity(player_id);
        let view_matrix = generate_view_matrix(player);

        let player = world.entity(player_id);
        let player_pos = player.get::<Position>().unwrap().0;

        // TODO(patrik): Check error
        let output = surface.get_render_target().unwrap();
        let view = output.texture.create_view(&wgpu::TextureViewDescriptor::default());

        uniform_buffer.update_view(view_matrix);
        gpu_device.queue.write_buffer(&uniform_buffer_handle,
                                      0,
                                      bytemuck::cast_slice(&[uniform_buffer]));

        let mut encoder = gpu_device.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Render Encoder"),
        });

        // window.get_render_target();

        // renderer.begin_render(&render_target);
        // renderer.end_render();

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("Render Pass"),
                    color_attachments: &[
                        wgpu::RenderPassColorAttachment {
                            view: &view,
                            resolve_target: None,
                            ops: wgpu::Operations {
                                load: wgpu::LoadOp::Clear(
                                    wgpu::Color {
                                        r: 0.1,
                                        g: 0.2,
                                        b: 0.3,
                                        a: 1.0,
                                    }
                                ),
                                store: true,
                            }
                        }
                    ],
                    depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                        view: &depth_texture.view,
                        depth_ops: Some(wgpu::Operations {
                            load: wgpu::LoadOp::Clear(1.0),
                            store: true,
                        }),
                        stencil_ops: None,
                    }),
                });

            render_pass.set_bind_group(0, &uniform_buffer_bind_group, &[]);

            let map = world.get_resource::<Map>().unwrap();
            // let sector = &map.sectors[38]; {
            for sector in &map.sectors {

                render_pass.set_pipeline(&pipeline.handle());

                let m = &sector.floor_mesh;
                render_pass.set_vertex_buffer(0, m.vertex_buffer.slice(..));
                render_pass.set_index_buffer(m.index_buffer.slice(..),
                                             wgpu::IndexFormat::Uint32);

                render_pass.draw_indexed(0..m.index_count, 0, 0..1);

                let m = &sector.ceiling_mesh;
                render_pass.set_vertex_buffer(0, m.vertex_buffer.slice(..));
                render_pass.set_index_buffer(m.index_buffer.slice(..),
                                             wgpu::IndexFormat::Uint32);

                render_pass.draw_indexed(0..m.index_count, 0, 0..1);

                let m = &sector.wall_mesh;
                render_pass.set_vertex_buffer(0, m.vertex_buffer.slice(..));
                render_pass.set_index_buffer(m.index_buffer.slice(..),
                                             wgpu::IndexFormat::Uint32);

                render_pass.draw_indexed(0..m.index_count, 0, 0..1);
            }
        }

        gpu_device.queue.submit(std::iter::once(encoder.finish()));
        output.present();

        if window.should_close() {
            let mut game_state = world.get_resource_mut::<GameState>().unwrap();
            game_state.close = true;
        }
    }
}

fn handle_window_event(game_state: &mut GameState,
                       event: glfw::WindowEvent)
{
    match event {
        glfw::WindowEvent::Key(Key::Escape, _, Action::Press, _) => {
            game_state.close = true;
        }

        glfw::WindowEvent::Key(key, _, Action::Press, _) => {
            match key {
                Key::W => game_state.up = true,
                Key::S => game_state.down = true,
                Key::A => game_state.left = true,
                Key::D => game_state.right = true,
                Key::Space => game_state.jump = true,

                _ => {},
            }
        }

        glfw::WindowEvent::Key(key, _, Action::Release, _) => {
            match key {
                Key::W => game_state.up = false,
                Key::S => game_state.down = false,
                Key::A => game_state.left = false,
                Key::D => game_state.right = false,
                Key::Space => game_state.jump = false,

                _ => {},
            }
        }

        glfw::WindowEvent::CursorPos(mx, my) => {
            let mx = mx as f32;
            let my = my as f32;

            if game_state.first_mouse {
                game_state.last_mouse_x = mx;
                game_state.last_mouse_y = my;
                game_state.first_mouse = false;
            }

            let mut x_offset = mx - game_state.last_mouse_x;
            let mut y_offset = game_state.last_mouse_y - my;
            game_state.last_mouse_x = mx;
            game_state.last_mouse_y = my;

            const SENSITIVITY: f32 = 0.1;
            x_offset *= SENSITIVITY;
            y_offset *= SENSITIVITY;

            game_state.yaw   -= x_offset;
            game_state.pitch += y_offset;
        }

        _ => {}
    }
}
