use image::{DynamicImage, ImageBuffer};
use clap::Parser;
use thiserror::Error;
use wgpu::{ImageDataLayout, Origin3d, TextureDescriptor};
use std::fs::read;
use zune_hdr::HdrDecoder;

use crate::texture::write_cubemap_to_ktx2;

const NAME: &str = env!("CARGO_PKG_NAME");
const VERSION: &str = env!("CARGO_PKG_VERSION");


// #===================#
// #=== BOILERPLATE ===#

#[derive(Parser)]
#[command(name = NAME)]
#[command(version = VERSION)]
#[command(about = "Command line tool to bake your HDRi maps for use in Bevy game engine", long_about = None)]
struct Cli {
    /// The source folder that contains texture tiles (nx.png, px.png, ny.png, py.png, nz.png, pz.png)
    source: Option<String>,
}

/// Custom error type
#[derive(Debug, Error)]
enum Error {

    #[error("{}", .0)]
    ImageSizeError(imagesize::ImageError),

    #[error("{}", .0)]
    ImageError(image::ImageError),

    #[error("The source files are not the same size")]
    InvalidSize,

    #[error("Error requesting GPU adapter")]
    NoGPUFound,
}
impl From<image::ImageError> for Error {
    fn from(value: image::ImageError) -> Self {
        Error::ImageError(value)
    }
}
impl From<imagesize::ImageError> for Error {
    fn from(value: imagesize::ImageError) -> Self {
        Error::ImageSizeError(value)
    }
}


// #=====================#
// #=== MAIN FUNCTION ===#

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    let mut path = String::from(".");

    if let Some(s) = cli.source {
        path = format!("{s}");
    }

    if let Err(e) = process_hdr(&path).await {
        println!("{}", e.to_string())
    }
}


// #========================#
// #=== IMAGE PROCESSING ===#
/*
fn create_tilemap(source: &str) -> Result<(), Error> {

    let size = imagesize::size(format!("{source}/nx.png"))?;
    let first_image_dimensions = (size.width as u32, size.height as u32);
    let (img_width, img_height) = first_image_dimensions;

    let mut combined_img: RgbaImage = ImageBuffer::new(img_width, img_height * 6);

    for (i, img_path) in ["nx.png", "px.png", "py.png", "ny.png", "nz.png", "pz.png"].iter().enumerate() {
        println!("Processing {source}/skybox_tilemap.png ...");
        let mut img = image::open(format!("{source}/{img_path}"))?;
        if img.dimensions() != first_image_dimensions { return Err(Error::InvalidSize); }

        if *img_path == "py.png" || *img_path == "ny.png" {
            img = img.fliph().flipv();
        }

        let top = img_height * i as u32;
        for y in 0..img_height {
            for x in 0..img_width {
                let pixel = img.get_pixel(x, y);
                combined_img.put_pixel(x, top + y, Rgba([pixel[0], pixel[1], pixel[2], pixel[3]]));
            }
        }
    }

    combined_img.save(format!("{source}/skybox_tilemap.png"))?;
    println!("Combined image saved to {source}/skybox_tilemap.png");
    Ok(())
}
*/
mod cubemap;
mod ibl;
mod shader_src;
mod texture;
mod mipmap;

async fn process_hdr(source: &str) -> Result<(), Error> {

    // Get wgpu instance
    let instance = wgpu::Instance::default();

    // Look for dedicated GPU
    let adapter = instance.enumerate_adapters(wgpu::Backends::all()).into_iter().find(|adapter| adapter.get_info().device_type == wgpu::DeviceType::DiscreteGpu);

    // Look for high performance GPU in case of no dedicated GPU
    let adapter = adapter.or(instance.request_adapter(&wgpu::RequestAdapterOptions { power_preference: wgpu::PowerPreference::HighPerformance, ..Default::default()}).await);

    // Return with error if no GPU found
    let Some(adapter) = adapter else { return Err(Error::NoGPUFound) };

    // Get access to physical GPU
    let (device, queue) = adapter.request_device(&wgpu::DeviceDescriptor {
        label: None,
        required_features: wgpu::Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES | wgpu::Features::PUSH_CONSTANTS,
        required_limits: wgpu::Limits::default(),
    }, None).await.unwrap();

    // Load HDRi
    let contents = read(source).unwrap();
    let mut data = HdrDecoder::new(contents);

    // Decode HDRi
    let pixel_buffer: Vec<f32> = data.decode().unwrap();
    let (width, height) = data.get_dimensions().unwrap();

    // Add alpha
    let pixel_buffer: Vec<f32> = pixel_buffer.chunks(3).flat_map(|c| [c[0], c[1], c[2], 1.0]).collect();

    let Some(buffer) = ImageBuffer::from_vec(width as u32, height as u32, pixel_buffer) else { return Err(Error::InvalidSize)};
    let dyn_image = DynamicImage::ImageRgba32F(buffer);

    let cubemap_side = 1024;

    // Convert dyn_image to cubemap
    let cubemap = cubemap::equirectangular_to_cubemap(
        &device,
        &queue,
        &dyn_image,
        cubemap_side,
        wgpu::TextureFormat::Rgba16Float,
        true
    ).await.unwrap();

    // Generate mipmaps for the environment map
    let env_map = mipmap::generate_mipmaps(&device, &queue, &cubemap);

    // Create a path from the argument
    let path = match source.rsplit_once('/') {
        Some((a, _)) => String::from(a),
        None => String::from("."),
    };

    // Download environment map data
    let env_map_data = download_cubemap(&device, &queue, &env_map).await.unwrap();
    write_cubemap_to_ktx2(&env_map_data, wgpu::TextureFormat::Rgba16Float, cubemap_side, env_map.mip_level_count(), &format!("{path}/skybox.ktx2"));

    // Hardcode parameters
    let bake_parameters = ibl::BakeParameters {
        num_samples: 128,
        strength: 1.0,
        contrast_correction: 1.0,
        brightness_correction: 1.0,
        saturation_correction: 1.0,
        hue_correction: 0.0,
    };

    // Calculate radiance
    let radiance = ibl::radiance(&device, &queue, &env_map, cubemap_side, &bake_parameters).await.unwrap();

    // Download radiance data
    let radiance_data = download_cubemap(&device, &queue, &radiance).await.unwrap();
    write_cubemap_to_ktx2(&radiance_data, wgpu::TextureFormat::Rgba16Float, cubemap_side, radiance.mip_level_count(), &format!("{path}/specular_map.ktx2"));

    // Calculate irradiance
    let irradiance = ibl::irradiance(&device, &queue, &env_map, cubemap_side, &bake_parameters).await.unwrap();

    // Download irradiance data
    let irradiance_data = download_cubemap(&device, &queue, &irradiance).await.unwrap();
    write_cubemap_to_ktx2(&irradiance_data, wgpu::TextureFormat::Rgba16Float, cubemap_side, 1, &format!("{path}/diffuse_map.ktx2"));

    Ok(())
}



// Downloads the data of a cubemap in GPU memory to a Vec<f32>. It returns the data
// and the number of levels that it downloaded
async fn download_cubemap(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    cubemap: &wgpu::Texture,
) -> Option<Vec<u8>>
{
    let mut result = vec![];
    let bytes_per_pixel = cubemap.format()
        .block_copy_size(Some(wgpu::TextureAspect::All))
        .unwrap();

    // Will copy data from texture on GPU to staging buffer on CPU.
    let staging_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: cubemap.width() as u64 * cubemap.height() as u64 * 6 * bytes_per_pixel as u64,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });


    let aux_texture = if cubemap.mip_level_count() > 1 {
        Some(device.create_texture(&TextureDescriptor {
            label: Some("Aux padded texture"),
            size: wgpu::Extent3d{
                width: wgpu::COPY_BYTES_PER_ROW_ALIGNMENT / bytes_per_pixel,
                height: wgpu::COPY_BYTES_PER_ROW_ALIGNMENT / bytes_per_pixel,
                depth_or_array_layers: 6,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: cubemap.format(),
            usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[]
        }))
    }else{
        None
    };

    for level in 0..cubemap.mip_level_count() {
        let level_side = cubemap.width() >> level;
        let mut encoder =
            device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        let (cubemap, level) = if level_side * bytes_per_pixel < wgpu::COPY_BYTES_PER_ROW_ALIGNMENT {
            encoder.copy_texture_to_texture(
                wgpu::ImageCopyTextureBase {
                    texture: cubemap,
                    mip_level: level,
                    origin: Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All
                },
                wgpu::ImageCopyTextureBase {
                    texture: aux_texture.as_ref().unwrap(),
                    mip_level: 0,
                    origin: Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All
                },
                wgpu::Extent3d { width: level_side, height: level_side, depth_or_array_layers: 6 }
            );
            (aux_texture.as_ref().unwrap(), 0)
        }else{
            (cubemap, level)
        };

        let bytes_per_row = (level_side * bytes_per_pixel).max(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
        encoder.copy_texture_to_buffer(
            wgpu::ImageCopyTextureBase {
                texture: cubemap,
                mip_level: level,
                origin: Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All
            },
            wgpu::ImageCopyBufferBase{
                buffer: &staging_buffer,
                layout: ImageDataLayout{
                    offset: 0,
                    bytes_per_row: Some(bytes_per_row),
                    rows_per_image: Some(level_side),
                }
            },
            wgpu::Extent3d { width: bytes_per_row / bytes_per_pixel, height: level_side, depth_or_array_layers: 6 }
        );

        // Submits command encoder for processing
        queue.submit(Some(encoder.finish()));


        // Note that we're not calling `.await` here.
        // TODO: spawn and start next copy?
        let level_bytes = bytes_per_row as u64 * level_side as u64 * 6;
        let buffer_slice = staging_buffer.slice(..level_bytes);
        // Sets the buffer up for mapping, sending over the result of the mapping back to us when it is finished.
        let (sender, receiver) = futures_intrusive::channel::shared::oneshot_channel();
        buffer_slice.map_async(wgpu::MapMode::Read, move |v| sender.send(v).unwrap());

        // Poll the device in a blocking manner so that our future resolves.
        device.poll(wgpu::Maintain::Wait);

        // Awaits until `buffer_future` can be read from
        if let Some(Ok(())) = receiver.receive().await {
            // Gets contents of buffer
            let data = buffer_slice.get_mapped_range();
            // Since contents are got in bytes, this converts these bytes back to u32
            if level_side * bytes_per_pixel < wgpu::COPY_BYTES_PER_ROW_ALIGNMENT {
                // We are using the auxiliary padded texture to download so we need to copy row by row
                for row in data
                    .chunks(aux_texture.as_ref().unwrap().width() as usize * bytes_per_pixel as usize)
                    .take(level_side as usize * 6)
                {
                    result.extend(&row[..level_side as usize * bytes_per_pixel as usize]);
                }
            }else{
                result.extend_from_slice(&data);
            }

            // With the current interface, we have to make sure all mapped views are
            // dropped before we unmap the buffer.
            drop(data);
            staging_buffer.unmap(); // Unmaps buffer from memory
        }else{
            return None
        }
    }

    // Returns data from buffer
    Some(result)
}

