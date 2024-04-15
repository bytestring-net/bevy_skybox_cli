use std::borrow::Cow;

use image::DynamicImage;
use wgpu::{util::{DeviceExt, TextureDataOrder}, TextureDescriptor, TextureFormat, TextureUsages};

use crate::shader_src::{set_constants, set_texture_format};



// Runs a compute shader that converts an equirectangular input image into a cubemap
pub async fn equirectangular_to_cubemap(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    env_map: &DynamicImage,
    cubemap_side: u32,
    pixel_format: wgpu::TextureFormat,
    flip_y: bool,
) -> Option<wgpu::Texture> {
    // TODO: check if input is different
    let env_map_format = wgpu::TextureFormat::Rgba32Float;

    static EQUI_TO_CUBEMAP_SRC: &str = include_str!("equirectangular_to_cubemap.wgsl");
    let equi_to_cubemap_src = set_constants(EQUI_TO_CUBEMAP_SRC, &[
        ("FLIP_Y", flip_y.to_string().into())
    ]);
    let equi_to_cubemap_src = set_texture_format(&equi_to_cubemap_src, &[
        ("equirectangular", env_map_format),
        ("cubemap_faces", pixel_format),
    ]);

    // Loads the shader from WGSL
    let cs_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: None,
        source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(&equi_to_cubemap_src)),
    });

    let env_map = device.create_texture_with_data(
        queue,
        &TextureDescriptor {
            label: Some("Envmap"),
            size: wgpu::Extent3d{ width: env_map.width(), height: env_map.height(), depth_or_array_layers: 1},
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: env_map_format,
            usage: wgpu::TextureUsages::STORAGE_BINDING
                | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[]
        },
        TextureDataOrder::default(),
        env_map.as_bytes()
    );

    let env_map_view = env_map.create_view(&wgpu::TextureViewDescriptor {
        label: None,
        dimension: Some(wgpu::TextureViewDimension::D2),
        ..wgpu::TextureViewDescriptor::default()
    });

    let cubemap = device.create_texture(
        &TextureDescriptor {
            label: Some("Cubemap"),
            size: wgpu::Extent3d{ width: cubemap_side, height: cubemap_side, depth_or_array_layers: 6},
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: pixel_format,
            usage: wgpu::TextureUsages::STORAGE_BINDING
                | wgpu::TextureUsages::TEXTURE_BINDING
                | TextureUsages::COPY_SRC,
            view_formats: &[]
        },
    );

    let cubemap_view = cubemap.create_view(&wgpu::TextureViewDescriptor {
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
                ty: wgpu::BindingType::StorageTexture {
                    access: wgpu::StorageTextureAccess::ReadOnly,
                    format: TextureFormat::Rgba32Float,
                    view_dimension: wgpu::TextureViewDimension::D2
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::StorageTexture {
                    access: wgpu::StorageTextureAccess::WriteOnly,
                    format: pixel_format,
                    view_dimension: wgpu::TextureViewDimension::D2Array
                },
                count: None,
            },
        ],
    });

    // A pipeline specifies the operation of a shader
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("Equirectangular To Cubemap Layout"),
        bind_group_layouts: &[&bind_group_layout],
        push_constant_ranges: &[],
    });


    // Instantiates the pipeline.
    let compute_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: None,
        layout: Some(&pipeline_layout),
        module: &cs_module,
        entry_point: "equirectangular_to_cubemap",
    });


    // Instantiates the bind group, once again specifying the binding of buffers.
    let bind_group_layout = compute_pipeline.get_bind_group_layout(0);
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("Equirectangular to Cubemap BindGroup"),
        layout: &bind_group_layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: wgpu::BindingResource::TextureView(&env_map_view),
        },wgpu::BindGroupEntry {
            binding: 1,
            resource: wgpu::BindingResource::TextureView(&cubemap_view),
        }],
    });

    // A command encoder executes one or many pipelines.
    // It is to WebGPU what a command buffer is to Vulkan.
    let mut encoder =
        device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
    {
        let mut cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: None,
            ..Default::default()
        });
        cpass.set_pipeline(&compute_pipeline);
        cpass.set_bind_group(0, &bind_group, &[]);
        cpass.insert_debug_marker("Compute equirectangular to cubemap");
        cpass.dispatch_workgroups(cubemap_side, cubemap_side, 6); // Number of cells to run, the (x,y,z) size of item being processed
    }

    // Submits command encoder for processing
    queue.submit(Some(encoder.finish()));

    // Poll the device in a blocking manner so that our future resolves.
    device.poll(wgpu::Maintain::Wait);

    Some(cubemap)
}