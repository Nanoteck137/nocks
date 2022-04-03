use std::path::Path;
use std::fs::File;
use std::io::Read;
use std::time::Instant;

use glfw::{Action, Context, Key};
use glam::f32::{ Mat4, Vec2, Vec3 };
use wgpu::util::DeviceExt;

extern crate glfw;

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct Vertex {
    position: [f32; 3],
    color: [f32; 3],
}

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct UniformBuffer {
    projection_matrix: [f32; 4 * 4],
    view_matrix: [f32; 4 * 4],
    model_matrix: [f32; 4 * 4],
}

impl UniformBuffer {
    fn new(projection_matrix: Mat4, view_matrix: Mat4, model_matrix: Mat4) -> Self {
        let mut result = Self {
            projection_matrix: [0.0; 4 * 4],
            view_matrix: [0.0; 4 * 4],
            model_matrix: [0.0; 4 * 4],
        };

        result.update(projection_matrix, view_matrix, model_matrix);

        result
    }

    fn update(&mut self, projection_matrix: Mat4, view_matrix: Mat4, model_matrix: Mat4) {
        projection_matrix.write_cols_to_slice(&mut self.projection_matrix);
        view_matrix.write_cols_to_slice(&mut self.view_matrix);
        model_matrix.write_cols_to_slice(&mut self.model_matrix);
    }

    fn update_view(&mut self, view: Mat4) {
        view.write_cols_to_slice(&mut self.view_matrix);
    }

    fn update_model(&mut self, model: Mat4) {
        model.write_cols_to_slice(&mut self.model_matrix);
    }
}

struct Texture {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
}


impl Texture {
    const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

    fn create_depth_texture(device: &wgpu::Device, width: u32, height: u32, label: &str)
        -> Self
    {
        let size = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };

        let desc = wgpu::TextureDescriptor {
            label: Some(label),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: Self::DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT |
                    wgpu::TextureUsages::TEXTURE_BINDING,
        };

        let texture = device.create_texture(&desc);

        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        Self { texture, view }
    }
}

struct Mesh {
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    index_count: u32,
}

impl Mesh {
    fn from_data(gpu_device: &GpuDevice,
                 vertex_buffer: &Vec<Vertex>,
                 index_buffer: &Vec<u32>)
        -> Self
    {
        let index_count = index_buffer.len();

        let vertex_buffer = gpu_device.device.create_buffer_init(
            &wgpu::util::BufferInitDescriptor {
                label: Some("Vertex Buffer"),
                contents: bytemuck::cast_slice(vertex_buffer),
                usage: wgpu::BufferUsages::VERTEX,
            }
        );

        let index_buffer = gpu_device.device.create_buffer_init(
            &wgpu::util::BufferInitDescriptor {
                label: Some("Index Buffer"),
                contents: bytemuck::cast_slice(index_buffer),
                usage: wgpu::BufferUsages::INDEX,
            }
        );


        Self {
            vertex_buffer,
            index_buffer,
            index_count: index_count.try_into().unwrap(),
        }
    }
}

struct Camera2D {
    pos: Vec2,
}

impl Camera2D {
    fn new(pos: Vec2) -> Self {
        Self {
            pos
        }
    }

    fn create_view_matrix(&self) -> Mat4 {
        let pos = (self.pos, 0.0);
        let view = Mat4::from_translation(pos.try_into().unwrap());

        view.inverse()
    }
}

struct Camera3D {
    pos: Vec3,
    front: Vec3,
    up: Vec3,
}

impl Camera3D {
    fn new(pos: Vec3, front: Vec3) -> Self {
        let up = Vec3::new(0.0, 1.0, 0.0);

        Self {
            pos,
            front,
            up
        }
    }

    fn create_view_matrix(&self) -> Mat4 {
        let view = Mat4::look_at_lh(self.pos, self.pos + self.front, self.up);

        view
    }
}

struct GameState {
    up: bool,
    down: bool,
    left: bool,
    right: bool,

    first_mouse: bool,
    last_mouse_x: f32,
    last_mouse_y: f32,

    yaw: f32,
    pitch: f32,
}

impl GameState {
    fn new() -> Self {
        Self {
            up: false,
            down: false,
            left: false,
            right: false,

            first_mouse: true,
            last_mouse_x: 0.0,
            last_mouse_y: 0.0,

            yaw: 90.0,
            pitch: 0.0,
        }
    }
}

impl Vertex {
    fn desc<'a>() -> wgpu::VertexBufferLayout<'a> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x3,
                },
                wgpu::VertexAttribute {
                    offset: std::mem::size_of::<[f32; 3]>() as wgpu::BufferAddress,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x3,
                }
            ]
        }
    }
}

struct Map {
    meshes: Vec<Mesh>,
}

struct GpuDevice {
    surface: wgpu::Surface,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,

    width: u32,
    height: u32,

    depth_texture: Texture,
    render_pipeline: wgpu::RenderPipeline,

    uniform_buffer: wgpu::Buffer,
    uniform_buffer_bind_group: wgpu::BindGroup,
}

impl GpuDevice {
    async fn new(window: &glfw::Window,
                 width: u32, height: u32,
                 initial_uniform_buffer: UniformBuffer)
        -> Option<Self>
    {
        let instance = wgpu::Instance::new(wgpu::Backends::all());
        let surface = unsafe { instance.create_surface(window) };
        let adapter = instance.request_adapter(
            &wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::default(),
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            },
        ).await.unwrap();
        println!("Adapter: {:#?}", adapter);

        let (device, queue) = adapter.request_device(
            &wgpu::DeviceDescriptor {
                features: wgpu::Features::POLYGON_MODE_LINE, // wgpu::Features::default(),
                limits: wgpu::Limits::default(),
                label: None,
            },
            None, // Trace path
        ).await.unwrap();

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface.get_preferred_format(&adapter).unwrap(),
            width,
            height,
            present_mode: wgpu::PresentMode::Fifo,
        };

        surface.configure(&device, &config);

        let shader = device.create_shader_module(&wgpu::include_wgsl!("shader.wgsl"));

        let uniform_buffer = device.create_buffer_init(
            &wgpu::util::BufferInitDescriptor {
                label: Some("Uniform Buffer"),
                contents: bytemuck::cast_slice(&[initial_uniform_buffer]),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            }
        );

        let uniform_buffer_bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
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

        let uniform_buffer_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &uniform_buffer_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform_buffer.as_entire_binding(),
                }
            ],
            label: Some("uniform_buffer_bind_group"),
        });

        let depth_texture = Texture::create_depth_texture(&device, width, height, "Depth Texture");

        let render_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Render Pipeline Layout"),
                bind_group_layouts: &[&uniform_buffer_bind_group_layout],
                push_constant_ranges: &[],
            });

        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Render Pipeline"),
            layout: Some(&render_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main", // 1.
                buffers: &[Vertex::desc()], // 2.
            },
            fragment: Some(wgpu::FragmentState { // 3.
                module: &shader,
                entry_point: "fs_main",
                targets: &[wgpu::ColorTargetState { // 4.
                    format: config.format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                }],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList, // 1.
                strip_index_format: None,
                front_face: wgpu::FrontFace::Cw, // 2.
                cull_mode: Some(wgpu::Face::Back),
                // Setting this to anything other than Fill requires Features::NON_FILL_POLYGON_MODE
                polygon_mode: wgpu::PolygonMode::Fill,
                // Requires Features::DEPTH_CLIP_CONTROL
                unclipped_depth: false,
                // Requires Features::CONSERVATIVE_RASTERIZATION
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: Texture::DEPTH_FORMAT,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState {
                count: 1, // 2.
                mask: !0, // 3.
                alpha_to_coverage_enabled: false, // 4.
            },
            multiview: None, // 5.
        });

        Some(Self {
            surface,
            device,
            queue,
            config,

            width,
            height,

            depth_texture,

            render_pipeline,

            uniform_buffer,
            uniform_buffer_bind_group,
        })
    }
}

fn load_map<P>(filename: P, gpu_device: &GpuDevice) -> Option<Map>
    where P: AsRef<Path>
{
    let mut file = File::open(filename).ok()?;
    let mut data = Vec::new();
    file.read_to_end(&mut data).ok()?;

    let mime_map = mime::Map::deserialize(&data).unwrap();

    let mut meshes = Vec::new();

    let sector = &mime_map.sectors[38];

    for sector in &mime_map.sectors {
        let mut vertex_buffer = Vec::new();
        for v in &sector.vertex_buffer {
            vertex_buffer.push(Vertex {
                position: [v.x, v.y, v.z],
                color: [v.color[0], v.color[1], v.color[2]],
            });
        }

        let index_buffer = &sector.index_buffer;
        meshes.push(Mesh::from_data(gpu_device,
                                    &vertex_buffer,
                                    index_buffer));
    }


    let map = Map {
        meshes
    };

    Some(map)
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
    let mut camera = Camera3D::new(Vec3::new(1077.0, 460.0, -3600.0), Vec3::new(0.0, 0.0, 1.0));
    // let mut camera = Camera2D::new(Vec2::new(vert.position[0], vert.position[1]));
    //println!("Setting camera: {}, {}", camera.pos.x, camera.pos.y);
    let mut game_state = GameState::new();

    let gpu_device = pollster::block_on(GpuDevice::new(&window,
                                                       window_width.try_into().unwrap(),
                                                       window_height.try_into().unwrap(),
                                                       uniform_buffer)).unwrap();

    let mut map = load_map("/Users/patrikrosenstrom/wad_reader/map.mup",
                           &gpu_device)
        .expect("Failed to load map");

    let time = Instant::now();
    let mut past = 0.0;

    while !window.should_close() {
        let now = time.elapsed().as_secs_f32();
        let dt = now - past;
        past = now;

        glfw.poll_events();
        for (_, event) in glfw::flush_messages(&events) {
            handle_window_event(&mut window, &mut game_state, event);
        }

        const CAMERA_SPEED: f32 = 100.0;

        if game_state.up {
            camera.pos += (CAMERA_SPEED * camera.front) * dt;
        }

        if game_state.down {
            camera.pos -= (CAMERA_SPEED * camera.front) * dt;
        }

        if game_state.right {
            camera.pos -= (camera.front.cross(camera.up).normalize() * CAMERA_SPEED) * dt
        }

        if game_state.left {
            camera.pos += (camera.front.cross(camera.up).normalize() * CAMERA_SPEED) * dt
        }

        let pitch = game_state.pitch;
        let yaw = game_state.yaw;

        let direction = Vec3::new(
            yaw.to_radians().cos() * pitch.to_radians().cos(),
            pitch.to_radians().sin(),
            yaw.to_radians().sin() * pitch.to_radians().cos());
        camera.front = direction.normalize();

        let output = gpu_device.surface.get_current_texture().unwrap();
        let view = output.texture.create_view(&wgpu::TextureViewDescriptor::default());

        uniform_buffer.update_view(camera.create_view_matrix());
        gpu_device.queue.write_buffer(&gpu_device.uniform_buffer, 0, bytemuck::cast_slice(&[uniform_buffer]));

        let mut encoder = gpu_device.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Render Encoder"),
        });

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
                        view: &gpu_device.depth_texture.view,
                        depth_ops: Some(wgpu::Operations {
                            load: wgpu::LoadOp::Clear(1.0),
                            store: true,
                        }),
                        stencil_ops: None,
                    }),
                });

            render_pass.set_pipeline(&gpu_device.render_pipeline);
            render_pass.set_bind_group(0, &gpu_device.uniform_buffer_bind_group, &[]);

            for mesh in &map.meshes {
                render_pass.set_vertex_buffer(0, mesh.vertex_buffer.slice(..));
                render_pass.set_index_buffer(mesh.index_buffer.slice(..), wgpu::IndexFormat::Uint32);

                render_pass.draw_indexed(0..mesh.index_count, 0, 0..1);
            }
        }

        gpu_device.queue.submit(std::iter::once(encoder.finish()));
        output.present();
    }
}

fn handle_window_event(window: &mut glfw::Window, game_state: &mut GameState, event: glfw::WindowEvent) {
    match event {
        glfw::WindowEvent::Key(Key::Escape, _, Action::Press, _) => {
            window.set_should_close(true)
        }

        glfw::WindowEvent::Key(key, _, Action::Press, _) => {
            match key {
                Key::W => game_state.up = true,
                Key::S => game_state.down = true,
                Key::A => game_state.left = true,
                Key::D => game_state.right = true,

                _ => {},
            }
        }

        glfw::WindowEvent::Key(key, _, Action::Release, _) => {
            match key {
                Key::W => game_state.up = false,
                Key::S => game_state.down = false,
                Key::A => game_state.left = false,
                Key::D => game_state.right = false,

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
