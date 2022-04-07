use wgpu::util::DeviceExt;

use glam::f32::Mat4;

pub use pipeline::{ PipelineLayout, RenderPipeline, RenderPipelineBuilder };

pub mod pipeline;

pub struct WindowSurface {
    surface: wgpu::Surface,
    config: Option<wgpu::SurfaceConfiguration>,
}

impl WindowSurface {
    fn new(surface: wgpu::Surface) -> Self {
        Self {
            surface,
            config: None,
        }
    }

    fn configure(&mut self,
                 device: &wgpu::Device,
                 adapter: &wgpu::Adapter,
                 width: u32, height: u32)
    {
        let surface_format =
            self.surface.get_preferred_format(&adapter).unwrap();

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width,
            height,
            present_mode: wgpu::PresentMode::Fifo,
        };

        self.surface.configure(&device, &config);
        self.config = Some(config);
    }

    pub fn get_render_target(&self)
        -> Result<wgpu::SurfaceTexture, wgpu::SurfaceError>
    {
        self.surface.get_current_texture()
    }

    pub fn config(&self) -> &wgpu::SurfaceConfiguration {
        self.config.as_ref().expect("Surface not configured")
    }
}

pub struct GpuDevice {
    pub instance: wgpu::Instance,
    pub adapter: wgpu::Adapter,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,

    /*
    pub depth_texture: Texture,
    pub render_pipeline: wgpu::RenderPipeline,

    pub uniform_buffer: wgpu::Buffer,
    pub uniform_buffer_bind_group: wgpu::BindGroup,
    */
}

impl GpuDevice {
    pub async fn new_for_window(window: &glfw::Window)
        -> Option<(Self, WindowSurface)>
    {
        let instance = wgpu::Instance::new(wgpu::Backends::all());

        let surface = unsafe { instance.create_surface(window) };
        let mut surface = WindowSurface::new(surface);

        let adapter = instance.request_adapter(
            &wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::default(),
                compatible_surface: None,
                force_fallback_adapter: false,
            },
        ).await.unwrap();

        let desc = wgpu::DeviceDescriptor {
            features: wgpu::Features::default(),
            limits: wgpu::Limits::default(),
            label: None,
        };

        let (device, queue) = adapter.request_device(&desc, None,)
            .await
            .expect("Failed to request device");


        let (width, height) = window.get_framebuffer_size();
        surface.configure(&device, &adapter,
                          width.try_into().ok()?,
                          height.try_into().ok()?);

        /*
        let shader = device.create_shader_module(&wgpu::include_wgsl!("../shader.wgsl"));


        let width = surface.config().width;
        let height = surface.config().height;
        let depth_texture = Texture::create_depth_texture(&device, width, height, "Depth Texture");


    */

        let gpu_device = Self {
            instance,
            adapter,
            device,
            queue,
        };

        Some((gpu_device, surface))
    }
}

pub struct Texture {
    pub texture: wgpu::Texture,
    pub view: wgpu::TextureView,
}

impl Texture {
    const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

    pub fn create_depth_texture(gpu_device: &GpuDevice, width: u32, height: u32)
        -> Self
    {
        let size = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };

        let desc = wgpu::TextureDescriptor {
            label: None,
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: Self::DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT |
                   wgpu::TextureUsages::TEXTURE_BINDING,
        };

        let texture = gpu_device.device.create_texture(&desc);

        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        Self { texture, view }
    }
}

pub struct Mesh {
    pub vertex_buffer: wgpu::Buffer,
    pub index_buffer: wgpu::Buffer,
    pub index_count: u32,
}

impl Mesh {
    pub fn from_data(gpu_device: &GpuDevice,
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

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex {
    pub position: [f32; 3],
    pub color: [f32; 3],
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

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct UniformBuffer {
    projection_matrix: [f32; 4 * 4],
    view_matrix: [f32; 4 * 4],
    model_matrix: [f32; 4 * 4],
}

impl UniformBuffer {
    pub fn new(projection_matrix: Mat4, view_matrix: Mat4, model_matrix: Mat4) -> Self {
        let mut result = Self {
            projection_matrix: [0.0; 4 * 4],
            view_matrix: [0.0; 4 * 4],
            model_matrix: [0.0; 4 * 4],
        };

        result.update(projection_matrix, view_matrix, model_matrix);

        result
    }

    pub fn update(&mut self, projection_matrix: Mat4, view_matrix: Mat4, model_matrix: Mat4) {
        projection_matrix.write_cols_to_slice(&mut self.projection_matrix);
        view_matrix.write_cols_to_slice(&mut self.view_matrix);
        model_matrix.write_cols_to_slice(&mut self.model_matrix);
    }

    pub fn update_view(&mut self, view: Mat4) {
        view.write_cols_to_slice(&mut self.view_matrix);
    }

    pub fn update_model(&mut self, model: Mat4) {
        model.write_cols_to_slice(&mut self.model_matrix);
    }
}
