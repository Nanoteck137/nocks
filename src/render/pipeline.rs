use super::{ Texture, Vertex, WindowSurface, GpuDevice };

pub struct PipelineLayoutBuilder<'a> {
    bind_group_layouts: Vec<&'a wgpu::BindGroupLayout>,
}

impl<'a> PipelineLayoutBuilder<'a> {
    fn new() -> Self {
        Self {
            bind_group_layouts: Vec::new(),
        }
    }

    pub fn bind_group_layout(mut self,
                             bind_group_layout: &'a wgpu::BindGroupLayout)
        -> Self
    {
        self.bind_group_layouts.push(bind_group_layout);
        self
    }

    pub fn build(&self, gpu_device: &GpuDevice) -> PipelineLayout {
        let handle = gpu_device.device.create_pipeline_layout(
            &wgpu::PipelineLayoutDescriptor {
                label: None,
                bind_group_layouts: &self.bind_group_layouts,
                push_constant_ranges: &[],
            }
        );

        PipelineLayout::new(handle)
    }
}

pub struct PipelineLayout {
    handle: wgpu::PipelineLayout,
}

impl PipelineLayout {
    fn new(handle: wgpu::PipelineLayout) -> Self {
        Self { handle }
    }

    pub fn handle(&self) -> &wgpu::PipelineLayout {
        &self.handle
    }

    pub fn builder<'a>() -> PipelineLayoutBuilder<'a> {
        PipelineLayoutBuilder::new()
    }
}

pub struct RenderPipelineBuilder<'a> {
    vertex_shader: Option<&'a wgpu::ShaderModule>,
    fragment_shader: Option<&'a wgpu::ShaderModule>,
    use_depth_stencil: bool,

    topology: wgpu::PrimitiveTopology,
    front_face: wgpu::FrontFace,
    cull_mode: Option<wgpu::Face>,
    polygon_mode: wgpu::PolygonMode,
}

impl<'a> RenderPipelineBuilder<'a> {
    fn new() -> Self {
        Self {
            vertex_shader: None,
            fragment_shader: None,
            use_depth_stencil: false,

            topology: wgpu::PrimitiveTopology::TriangleList,
            front_face: wgpu::FrontFace::Cw,
            cull_mode: None,
            polygon_mode: wgpu::PolygonMode::Fill,
        }
    }

    pub fn vertex_shader(mut self, shader: &'a wgpu::ShaderModule) -> Self {
        self.vertex_shader = Some(shader);
        self
    }

    pub fn fragment_shader(mut self, shader: &'a wgpu::ShaderModule) -> Self {
        self.fragment_shader = Some(shader);
        self
    }

    pub fn depth_stencil(mut self, depth_stencil: bool) -> Self {
        self.use_depth_stencil = depth_stencil;
        self
    }

    pub fn topology(mut self, topology: wgpu::PrimitiveTopology) -> Self {
        self.topology = topology;
        self
    }

    pub fn front_face(mut self, front_face: wgpu::FrontFace) -> Self {
        self.front_face = front_face;
        self
    }

    pub fn cull_mode(mut self, cull_mode: wgpu::Face) -> Self {
        self.cull_mode = Some(cull_mode);
        self
    }

    pub fn polygon_mode(mut self, polygon_mode: wgpu::PolygonMode) -> Self {
        self.polygon_mode = polygon_mode;
        self
    }

    pub fn build(&self,
                 gpu_device: &GpuDevice,
                 surface: &WindowSurface,
                 pipeline_layout: &PipelineLayout)
        -> RenderPipeline
    {
        let depth_stencil = if self.use_depth_stencil {
            Some(wgpu::DepthStencilState {
                format: Texture::DEPTH_FORMAT,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            })
        } else {
            None
        };

        let handle = gpu_device.device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: None,
            layout: Some(pipeline_layout.handle()),

            vertex: wgpu::VertexState {
                module: self.vertex_shader.expect("No vertex shader selected"),
                entry_point: "vs_main",
                buffers: &[Vertex::desc()],
            },

            fragment: Some(wgpu::FragmentState {
                module: self.fragment_shader
                    .expect("No fragment shader selected"),
                entry_point: "fs_main",
                targets: &[wgpu::ColorTargetState {
                    format: surface.config().format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                }],
            }),

            primitive: wgpu::PrimitiveState {
                topology: self.topology,
                strip_index_format: None,
                front_face: self.front_face,
                cull_mode: self.cull_mode,
                polygon_mode: self.polygon_mode,
                unclipped_depth: false,
                conservative: false,
            },

            depth_stencil,

            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },

            multiview: None,
        });
        RenderPipeline::new(handle)
    }
}

pub struct RenderPipeline {
    handle: wgpu::RenderPipeline,
}

impl RenderPipeline {
    fn new(handle: wgpu::RenderPipeline) -> Self {
        Self {
            handle
        }
    }

    pub fn handle(&self) -> &wgpu::RenderPipeline {
        &self.handle
    }

    pub fn builder<'a>() -> RenderPipelineBuilder<'a> {
        RenderPipelineBuilder::new()
    }
}

// TODO(patrik): Add compute
pub struct ComputePipeline {}
