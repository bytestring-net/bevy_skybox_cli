use std::borrow::Cow;

use wgpu::{ImageCopyTexture, Origin3d, TextureDescriptor, TextureUsages};

use crate::shader_src::set_texture_format;


pub fn generate_mipmaps(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
) -> wgpu::Texture {
    static GENERATE_MIPMAPS_SRC: &str = include_str!("generate_mipmaps.wgsl");
    let generate_mipmaps_src = set_texture_format(GENERATE_MIPMAPS_SRC, &[
        ("input", texture.format()),
        ("output", texture.format())
    ]);
    // Loads the shader from WGSL
    let cs_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: None,
        source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(&generate_mipmaps_src)),
    });

    let max_side = texture.width().max(texture.height());
    let mip_level_count = (max_side as f32).log2().floor() as u32 + 1;
    let output = device.create_texture(
        &TextureDescriptor {
            label: Some("GenerateMipmapsOutput"),
            size: texture.size(),
            mip_level_count,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: texture.format(),
            usage: wgpu::TextureUsages::STORAGE_BINDING
                | wgpu::TextureUsages::TEXTURE_BINDING
                | TextureUsages::COPY_SRC
                | TextureUsages::COPY_DST,
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
                ty: wgpu::BindingType::StorageTexture {
                    access: wgpu::StorageTextureAccess::ReadOnly,
                    format: texture.format(),
                    view_dimension: wgpu::TextureViewDimension::D2Array
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::StorageTexture {
                    access: wgpu::StorageTextureAccess::WriteOnly,
                    format: texture.format(),
                    view_dimension: wgpu::TextureViewDimension::D2Array
                },
                count: None,
            },
        ],
    });



    // A pipeline specifies the operation of a shader
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("generate mipmaps Layout"),
        bind_group_layouts: &[&bind_group_layout],
        push_constant_ranges: &[],
    });


    // Instantiates the pipeline.
    let compute_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: None,
        layout: Some(&pipeline_layout),
        module: &cs_module,
        entry_point: "generate_mipmaps",
    });

    // A command encoder executes one or many pipelines.
    // It is to WebGPU what a command buffer is to Vulkan.
    let mut encoder =
        device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

    encoder.copy_texture_to_texture(
        ImageCopyTexture{texture, mip_level: 0, origin: Origin3d::ZERO, aspect: wgpu::TextureAspect::All},
        ImageCopyTexture{texture: &output, mip_level: 0, origin: Origin3d::ZERO, aspect: wgpu::TextureAspect::All},
        wgpu::Extent3d { width: texture.width(), height: texture.height(), depth_or_array_layers: texture.depth_or_array_layers() }
    );

    let mut side = max_side >> 1;
    let mut level = 0;
    while side > 0 {
        let output_view = output.create_view(&wgpu::TextureViewDescriptor {
            label: None,
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            array_layer_count: Some(6),
            base_mip_level: level + 1,
            mip_level_count: Some(1),
            ..wgpu::TextureViewDescriptor::default()
        });
        let input_view = output.create_view(&wgpu::TextureViewDescriptor {
            label: None,
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            array_layer_count: Some(6),
            base_mip_level: level,
            mip_level_count: Some(1),
            ..wgpu::TextureViewDescriptor::default()
        });

        // Instantiates the bind group, once again specifying the binding of buffers.
        let bind_group_layout = compute_pipeline.get_bind_group_layout(0);
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Generate mipmaps BindGroup"),
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&input_view),
            },wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(&output_view),
            }],
        });

        {
            let mut cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: None,
                ..Default::default()
            });
            cpass.set_pipeline(&compute_pipeline);
            cpass.set_bind_group(0, &bind_group, &[]);
            cpass.insert_debug_marker("Generate mipmaps");
            cpass.dispatch_workgroups(side, side, 6); // Number of cells to run, the (x,y,z) size of item being processed
        }

        side >>= 1;
        level += 1;
    }

    // Submits command encoder for processing
    queue.submit(Some(encoder.finish()));

    // Poll the device in a blocking manner so that our future resolves.
    device.poll(wgpu::Maintain::Wait);

    output
}
