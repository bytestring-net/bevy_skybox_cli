use std::borrow::Cow;

use bytemuck::{Pod, Zeroable};
use wgpu::{util::{BufferInitDescriptor, DeviceExt}, TextureDescriptor, TextureUsages};

use crate::shader_src::{set_constants, set_texture_format};


pub struct BakeParameters {
    pub num_samples: u16,
    pub strength: f32,
    pub contrast_correction: f32,
    pub brightness_correction: f32,
    pub saturation_correction: f32,
    pub hue_correction: f32,
}

impl BakeParameters {
    fn to_name_value(&self) -> [(&str, Cow<str>); 7] {
        [
            ("NUM_SAMPLES", Cow::Owned(format!("{}u", self.num_samples))),
            ("STRENGTH", Cow::Owned(format!("{:?}", self.strength))),
            ("CONTRAST_CORRECTION", Cow::Owned(format!("{:?}", self.contrast_correction))),
            ("BRIGHTNESS_CORRECTION", Cow::Owned(format!("{:?}", self.brightness_correction))),
            ("SATURATION_CORRECTION", Cow::Owned(format!("{:?}", self.saturation_correction))),
            ("HUE_CORRECTION", Cow::Owned(format!("{:?}", self.hue_correction))),
            ("FLIP_Y", true.to_string().into())
        ]
    }
}


// Bakes the IBL radiance map from an environment map. The input environment map and the output
// radiance map are cubemaps
pub async fn radiance(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    env_map: &wgpu::Texture,
    cubemap_side: u32,
    parameters: &BakeParameters,
) -> Option<wgpu::Texture> {
    static RADIANCE_SRC: &str = include_str!("ibl_bake.wgsl");
    let radiance_src = set_constants(RADIANCE_SRC, &parameters.to_name_value());
    let radiance_src = set_texture_format(&radiance_src, &[
        ("envmap", env_map.format()),
        ("output_faces", env_map.format())
    ]);
    // Loads the shader from WGSL
    let cs_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: None,
        source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(&radiance_src)),
    });

    let env_map_view = env_map.create_view(&wgpu::TextureViewDescriptor {
        label: None,
        dimension: Some(wgpu::TextureViewDimension::Cube),
        mip_level_count: Some(env_map.mip_level_count()),
        ..wgpu::TextureViewDescriptor::default()
    });

    let max_mip = (cubemap_side as f32).log2().floor() as u32 + 1;
    let output = device.create_texture(
        &TextureDescriptor {
            label: Some("Radiance"),
            size: wgpu::Extent3d{ width: cubemap_side, height: cubemap_side, depth_or_array_layers: 6},
            mip_level_count: max_mip,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: env_map.format(),
            usage: wgpu::TextureUsages::STORAGE_BINDING | TextureUsages::COPY_SRC,
            view_formats: &[]
        },
    );

    // A bind group defines how buffers are accessed by shaders.
    // It is to WebGPU what a descriptor set is to Vulkan.
    // `binding` here refers to the `binding` of a buffer in the shader (`layout(set = 0, binding = 0) buffer`).
    let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: None,
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::Cube,
                    multisampled: false
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::StorageTexture {
                    access: wgpu::StorageTextureAccess::WriteOnly,
                    format: env_map.format(),
                    view_dimension: wgpu::TextureViewDimension::D2Array
                },
                count: None,
            },
        ],
    });
    let bind_group2_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: None,
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
                count: None
            }
        ]
    });

    // A pipeline specifies the operation of a shader
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("Radiance Layout"),
        bind_group_layouts: &[&bind_group_layout, &bind_group2_layout],
        push_constant_ranges: &[],
    });


    // Instantiates the pipeline.
    let compute_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: None,
        layout: Some(&pipeline_layout),
        module: &cs_module,
        entry_point: "radiance",
    });

    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: None,
        address_mode_u: wgpu::AddressMode::ClampToEdge,
        address_mode_v: wgpu::AddressMode::ClampToEdge,
        address_mode_w: wgpu::AddressMode::ClampToEdge,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        mipmap_filter: wgpu::FilterMode::Linear,
        ..Default::default()
    });

    #[repr(C)]
    #[derive(Pod, Copy, Clone, Zeroable)]
    struct RadianceData {
        mip_level: u32,
        max_mip: u32,
    }

    for mip_level in 0..max_mip {
        // TODO: doing all the cycles in one command encoder hangs the OS and outputs black
        let level_side = cubemap_side >> mip_level;

        // A command encoder executes one or many pipelines.
        // It is to WebGPU what a command buffer is to Vulkan.
        let mut encoder =
            device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

        let output_level_view = output.create_view(&wgpu::TextureViewDescriptor {
            label: None,
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            array_layer_count: Some(6),
            base_mip_level: mip_level,
            mip_level_count: Some(1),
            ..wgpu::TextureViewDescriptor::default()
        });

        // Instantiates the bind group, once again specifying the binding of buffers.
        let bind_group_layout = compute_pipeline.get_bind_group_layout(0);
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Radiance BindGroup"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&env_map_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&output_level_view),
                },
            ],
        });

        // TODO: Create the buffer once and rewrite
        let uniforms = device.create_buffer_init(&BufferInitDescriptor {
            label: Some("Buffer"),
            contents: bytemuck::cast_slice(&[RadianceData {
                mip_level,
                max_mip,
            }]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let bind_group_uniforms = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Radiance uniforms bind group"),
            layout: &bind_group2_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniforms.as_entire_binding()
                }
            ]
        });

        {
            let mut cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some(&format!("Compute radiance level {}", mip_level)),
                ..Default::default()
            });
            cpass.set_pipeline(&compute_pipeline);
            cpass.set_bind_group(0, &bind_group, &[]);
            cpass.set_bind_group(1, &bind_group_uniforms, &[]);
            cpass.insert_debug_marker(&format!("Compute radiance level {}", mip_level));
            cpass.dispatch_workgroups(level_side, level_side, 6); // Number of cells to run, the (x,y,z) size of item being processed
        }

        // Submits command encoder for processing
        queue.submit(Some(encoder.finish()));

        // Poll the device in a blocking manner so that our future resolves.
        device.poll(wgpu::Maintain::Wait);
    }

    Some(output)
}

// Bakes the IBL irradiance map from an environment map. The input environment map and the output
// radiance map are cubemaps
pub async fn irradiance(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    env_map: &wgpu::Texture,
    cubemap_side: u32,
    parameters: &BakeParameters,
) -> Option<wgpu::Texture> {
    static IRRADIANCE_SRC: &str = include_str!("ibl_bake.wgsl");
    let irradiance_src = set_constants(&IRRADIANCE_SRC, &parameters.to_name_value());
    let irradiance_src = set_texture_format(&irradiance_src, &[
        ("envmap", env_map.format()),
        ("output_faces", env_map.format())
    ]);
    // Loads the shader from WGSL
    let cs_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: None,
        source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(&irradiance_src)),
    });

    let env_map_view = env_map.create_view(&wgpu::TextureViewDescriptor {
        label: None,
        dimension: Some(wgpu::TextureViewDimension::Cube),
        mip_level_count: Some(env_map.mip_level_count()),
        ..wgpu::TextureViewDescriptor::default()
    });

    let output = device.create_texture(
        &TextureDescriptor {
            label: Some("Irradiance"),
            size: wgpu::Extent3d{ width: cubemap_side, height: cubemap_side, depth_or_array_layers: 6},
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: env_map.format(),
            usage: wgpu::TextureUsages::STORAGE_BINDING | TextureUsages::COPY_SRC,
            view_formats: &[]
        },
    );

    let output_view = output.create_view(&wgpu::TextureViewDescriptor {
        label: None,
        dimension: Some(wgpu::TextureViewDimension::D2Array),
        array_layer_count: Some(6),
        ..wgpu::TextureViewDescriptor::default()
    });

    // A bind group defines how buffers are accessed by shaders.
    // It is to WebGPU what a descriptor set is to Vulkan.
    // `binding` here refers to the `binding` of a buffer in the shader (`layout(set = 0, binding = 0) buffer`).
    let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: None,
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::Cube,
                    multisampled: false
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::StorageTexture {
                    access: wgpu::StorageTextureAccess::WriteOnly,
                    format: env_map.format(),
                    view_dimension: wgpu::TextureViewDimension::D2Array
                },
                count: None,
            },
        ],
    });

    // A pipeline specifies the operation of a shader
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("Irradiance Layout"),
        bind_group_layouts: &[&bind_group_layout],
        push_constant_ranges: &[],
    });


    // Instantiates the pipeline.
    let compute_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: None,
        layout: Some(&pipeline_layout),
        module: &cs_module,
        entry_point: "irradiance",
    });

    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: None,
        address_mode_u: wgpu::AddressMode::ClampToEdge,
        address_mode_v: wgpu::AddressMode::ClampToEdge,
        address_mode_w: wgpu::AddressMode::ClampToEdge,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        mipmap_filter: wgpu::FilterMode::Linear,
        ..Default::default()
    });

    // A command encoder executes one or many pipelines.
    // It is to WebGPU what a command buffer is to Vulkan.
    let mut encoder =
        device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

    // Instantiates the bind group, once again specifying the binding of buffers.
    let bind_group_layout = compute_pipeline.get_bind_group_layout(0);
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("Irradiance BindGroup"),
        layout: &bind_group_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&env_map_view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(&sampler),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::TextureView(&output_view),
            },
        ],
    });

    {
        let mut cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("Compute irradiance"),
            ..Default::default()
        });
        cpass.set_pipeline(&compute_pipeline);
        cpass.set_bind_group(0, &bind_group, &[]);
        cpass.insert_debug_marker("Compute irradiance");
        cpass.dispatch_workgroups(cubemap_side, cubemap_side, 6); // Number of cells to run, the (x,y,z) size of item being processed
    }

    // Submits command encoder for processing
    queue.submit(Some(encoder.finish()));

    // Poll the device in a blocking manner so that our future resolves.
    device.poll(wgpu::Maintain::Wait);

    Some(output)
}
