use std::{ptr, ffi::CString};
use libktx_rs_sys::{ktxTexture2_Create, ktxTextureCreateStorageEnum_KTX_TEXTURE_CREATE_ALLOC_STORAGE, ktxTexture1_Create, ktxTexture};
/* use anyhow::Result; */

const GL_RGBA32F: u32 = 0x8814;
const GL_RGBA16F: u32 = 0x881A;
const VK_FORMAT_R32G32B32A32_SFLOAT: u32 = 109;
const VK_FORMAT_R16G16B16A16_SFLOAT: u32 = 97;

pub trait ToApi {
    fn to_gl(self) -> u32;
    fn to_vulkan(self) -> u32;
    fn to_wgsl_storage_str(self) -> &'static str ;
    fn to_wgsl_texture_str(self) -> &'static str;
}

impl ToApi for wgpu::TextureFormat {
    fn to_gl(self) -> u32 {
        match self {
            wgpu::TextureFormat::Rgba32Float => GL_RGBA32F,
            wgpu::TextureFormat::Rgba16Float => GL_RGBA16F,
            _ => todo!()
        }
    }

    fn to_vulkan(self) -> u32 {
        match self {
            wgpu::TextureFormat::Rgba32Float => VK_FORMAT_R32G32B32A32_SFLOAT,
            wgpu::TextureFormat::Rgba16Float => VK_FORMAT_R16G16B16A16_SFLOAT,
            _ => todo!()
        }
    }

    fn to_wgsl_storage_str(self) -> &'static str {
        match self {
            wgpu::TextureFormat::Rgba32Float => "rgba32float",
            wgpu::TextureFormat::Rgba16Float => "rgba16float",
            _ => todo!()
        }
    }

    fn to_wgsl_texture_str(self) -> &'static str {
        match self {
            wgpu::TextureFormat::Rgba32Float => "f32",
            wgpu::TextureFormat::Rgba16Float => "f32",
            _ => todo!()
        }
    }
}


enum KtxVersion {
    _1,
    _2,
}

fn write_cubemap_to_ktx(cubemap_data: &[u8], format: wgpu::TextureFormat, cubemap_side: u32, cubemap_levels: u32, output_file: &str, ktx_version: KtxVersion) {
    let bytes_per_pixel = format
        .block_copy_size(Some(wgpu::TextureAspect::All))
        .unwrap() as usize;

    let c_output_file = CString::new(output_file).unwrap();
    let mut create_info = libktx_rs_sys::ktxTextureCreateInfo {
        baseWidth: cubemap_side,
        baseHeight: cubemap_side,
        baseDepth: 1,
        numDimensions: 2,
        numLevels: cubemap_levels,
        numLayers: 1,
        numFaces: 6,
        generateMipmaps: false,
        glInternalformat: format.to_gl(),
        vkFormat: format.to_vulkan(),
        isArray: false,
        pDfd: ptr::null_mut(),
    };
    let texture: *mut ktxTexture;
    unsafe{
        match ktx_version {
            KtxVersion::_1 => {
                let mut texture_ktx1 = ptr::null_mut();
                ktxTexture1_Create(&mut create_info, ktxTextureCreateStorageEnum_KTX_TEXTURE_CREATE_ALLOC_STORAGE, &mut texture_ktx1);
                texture = texture_ktx1 as *mut ktxTexture;

            }
            KtxVersion::_2 => {
                let mut texture_ktx2 = ptr::null_mut();
                ktxTexture2_Create(&mut create_info, ktxTextureCreateStorageEnum_KTX_TEXTURE_CREATE_ALLOC_STORAGE, &mut texture_ktx2);
                texture = texture_ktx2 as *mut ktxTexture;
            }
        }

        let vtbl = &*(*texture).vtbl;
        let mut prev_end = 0;
        for level in 0..cubemap_levels {
            let level_side = (cubemap_side >> level) as usize;
            let face_size = level_side * level_side * bytes_per_pixel;
            for (face_idx, face) in cubemap_data[prev_end..]
                .chunks(level_side * level_side * bytes_per_pixel)
                .enumerate()
                .take(6)
            {
                (vtbl.SetImageFromMemory.unwrap())(texture, level, 0, face_idx as u32, face.as_ptr(), face_size);
            }
            prev_end += level_side * level_side * bytes_per_pixel * 6;
        }
        (vtbl.WriteToNamedFile.unwrap())(texture, c_output_file.as_ptr());
        (vtbl.Destroy.unwrap())(texture);
    }
}

// Writes the data of a cubemap as downloaded from GPU to a KTX2
pub fn write_cubemap_to_ktx2(cubemap_data: &[u8], format: wgpu::TextureFormat, cubemap_side: u32, cubemap_levels: u32, output_file: &str) {
    write_cubemap_to_ktx(cubemap_data, format, cubemap_side, cubemap_levels, output_file, KtxVersion::_2)
}