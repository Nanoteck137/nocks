use std::path::Path;
use std::fs::File;
use std::io::Read;
use std::time::Instant;

use glfw::{Action, Context, Key};
use glam::f32::{ Mat4, Vec2 };
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
}

impl UniformBuffer {
    fn new(projection_matrix: Mat4, view_matrix: Mat4) -> Self {
        let mut result = Self {
            projection_matrix: [0.0; 4 * 4],
            view_matrix: [0.0; 4 * 4],
        };

        result.update(projection_matrix, view_matrix);

        result
    }

    fn update(&mut self, projection_matrix: Mat4, view_matrix: Mat4) {
        projection_matrix.write_cols_to_slice(&mut self.projection_matrix);
        view_matrix.write_cols_to_slice(&mut self.view_matrix);
    }

    fn update_view(&mut self, view: Mat4) {
        view.write_cols_to_slice(&mut self.view_matrix);
    }
}

struct Camera {
    pos: Vec2,
}

impl Camera {
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

struct GameState {
    up: bool,
    down: bool,
    left: bool,
    right: bool,
}

impl GameState {
    fn new() -> Self {
        Self {
            up: false,
            down: false,
            left: false,
            right: false,
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
    vertex_buffer: Vec<Vertex>,
    index_buffer: Vec<u32>,
}

struct GpuDevice {
    surface: wgpu::Surface,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,

    width: u32,
    height: u32,

    render_pipeline: wgpu::RenderPipeline,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,

    uniform_buffer: wgpu::Buffer,
    uniform_buffer_bind_group: wgpu::BindGroup,
}

impl GpuDevice {
    async fn new(window: &glfw::Window, width: u32, height: u32, initial_uniform_buffer: UniformBuffer, map: &Map)
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
                features: wgpu::Features::POLYGON_MODE_LINE | wgpu::Features::POLYGON_MODE_POINT,
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
            depth_stencil: None, // 1.
            multisample: wgpu::MultisampleState {
                count: 1, // 2.
                mask: !0, // 3.
                alpha_to_coverage_enabled: false, // 4.
            },
            multiview: None, // 5.
        });

        let vertex_buffer = device.create_buffer_init(
            &wgpu::util::BufferInitDescriptor {
                label: Some("Vertex Buffer"),
                contents: bytemuck::cast_slice(&map.vertex_buffer),
                usage: wgpu::BufferUsages::VERTEX,
            }
        );

        let index_buffer = device.create_buffer_init(
            &wgpu::util::BufferInitDescriptor {
                label: Some("Index Buffer"),
                contents: bytemuck::cast_slice(&map.index_buffer),
                usage: wgpu::BufferUsages::INDEX,
            }
        );

        Some(Self {
            surface,
            device,
            queue,
            config,

            width,
            height,

            render_pipeline,
            vertex_buffer,
            index_buffer,

            uniform_buffer,
            uniform_buffer_bind_group,
        })
    }
}

fn load_map<P>(filename: P) -> Option<Map>
    where P: AsRef<Path>
{
    let mut file = File::open(filename).ok()?;
    let mut data = Vec::new();
    file.read_to_end(&mut data).ok()?;

    let magic = &data[0..4];
    println!("Magic: {:?}", std::str::from_utf8(&magic));

    let version = u32::from_le_bytes(data[4..8].try_into().ok()?);
    println!("Version: {}", version);

    let vertex_data_offset = u64::from_le_bytes(data[8..16].try_into().ok()?);
    let vertex_count = u64::from_le_bytes(data[16..24].try_into().ok()?);

    // Convert the numbers to usize
    let vertex_data_offset: usize = vertex_data_offset.try_into().ok()?;
    let vertex_count: usize = vertex_count.try_into().ok()?;

    println!("Vertex Offset: {:#x} Count: {}", vertex_data_offset, vertex_count);

    let index_data_offset = u64::from_le_bytes(data[24..32].try_into().ok()?);
    let index_count = u64::from_le_bytes(data[32..40].try_into().ok()?);

    let index_data_offset: usize = index_data_offset.try_into().ok()?;
    let index_count: usize = index_count.try_into().ok()?;

    println!("Index Offset: {:#x} Count: {}", index_data_offset, index_count);

    let mut vertices = Vec::new();

    for vertex_index in 0..vertex_count {
        let index = vertex_index * (8 * 2 + 4 * 4);
        let offset = vertex_data_offset + index;
        let data = &data[offset..offset + (8 * 2 + 4 * 4)];

        let x = f64::from_le_bytes(data[0..8].try_into().ok()?);
        let y = f64::from_le_bytes(data[8..16].try_into().ok()?);

        let r = f32::from_le_bytes(data[16..20].try_into().ok()?);
        let g = f32::from_le_bytes(data[20..24].try_into().ok()?);
        let b = f32::from_le_bytes(data[24..28].try_into().ok()?);
        let _a = f32::from_le_bytes(data[28..32].try_into().ok()?);

        vertices.push(Vertex {
            position: [x as f32, y as f32, 0.0],
            color: [r, g, b],
        });
    }

    let mut indices = Vec::new();

    for offset in 0..index_count {
        let offset = offset * std::mem::size_of::<u32>();
        let offset = index_data_offset + offset;
        let data = &data[offset..offset + 4];

        let index = u32::from_le_bytes(data.try_into().ok()?);
        indices.push(index);
    }

    Some(Map {
        vertex_buffer: vertices,
        index_buffer: indices
    })
}

fn main() {
    env_logger::init();

    let map = load_map("/home/nanoteck137/wad_reader/map.mup")
        .expect("Failed to load map");

    let vert = map.vertex_buffer[0];

    let mut glfw = glfw::init(glfw::FAIL_ON_ERRORS).unwrap();
    glfw.window_hint(glfw::WindowHint::ClientApi(glfw::ClientApiHint::NoApi));

    let (mut window, events) =
        glfw.create_window(1280, 720,
                           "Hello this is window",
                           glfw::WindowMode::Windowed)
            .expect("Failed to create GLFW window.");

    window.set_key_polling(true);

    let (window_width, window_height) = window.get_framebuffer_size();

    let projection_matrix = Mat4::orthographic_lh(0.0, window_width as f32, 0.0, window_height as f32, -1.0, 1.0);
    let view_matrix = Mat4::IDENTITY;

    let mut uniform_buffer = UniformBuffer::new(projection_matrix, view_matrix);
    let mut camera = Camera::new(Vec2::new(vert.position[0], vert.position[1]));
    println!("Setting camera: {}, {}", camera.pos.x, camera.pos.y);
    let mut game_state = GameState::new();

    let gpu_device = pollster::block_on(GpuDevice::new(&window, window_width.try_into().unwrap(), window_height.try_into().unwrap(), uniform_buffer, &map)).unwrap();

    let time = Instant::now();
    let mut past = 0.0;

    while !window.should_close() {
        let now = time.elapsed().as_secs_f32();
        let dt = now - past;
        // println!("Delta Time: {}", dt);
        past = now;

        glfw.poll_events();
        for (_, event) in glfw::flush_messages(&events) {
            handle_window_event(&mut window, &mut game_state, event);
        }

        const CAMERA_SPEED: f32 = 1000.0;

        if game_state.up {
            camera.pos.y += CAMERA_SPEED * dt;
        }

        if game_state.down {
            camera.pos.y -= CAMERA_SPEED * dt;
        }

        if game_state.right {
            camera.pos.x += CAMERA_SPEED * dt;
        }

        if game_state.left {
            camera.pos.x -= CAMERA_SPEED * dt;
        }

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
                    depth_stencil_attachment: None,
                });

            render_pass.set_pipeline(&gpu_device.render_pipeline); // 2.
            render_pass.set_vertex_buffer(0, gpu_device.vertex_buffer.slice(..));
            render_pass.set_index_buffer(gpu_device.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
            render_pass.set_bind_group(0, &gpu_device.uniform_buffer_bind_group, &[]);

            render_pass.draw_indexed(0..map.index_buffer.len().try_into().unwrap(), 0, 0..1);
            //render_pass.draw(0..VERTICES.len().try_into().unwrap(), 0..1); // 3.
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

        _ => {}
    }
}
