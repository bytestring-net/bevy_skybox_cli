use crate::texture::ToApi;
use std::{borrow::Cow, cell::OnceCell};

static RE_CONSTANTS: &str = r"const[ \t]+([A-Z][A-Z0-9_]*)[ \t]*(:)?[ \t]*([^ \t=]+)?[ \t]*=[ \t]*([^ \t;]*);";
const CONST_RE: OnceCell<regex::Regex> = OnceCell::new();

static RE_TEXTURE_STORAGE: &str = r"var ([a-z0-9_]+): texture_storage_([^<]+)[ \t]*<[ \t]*([^,]*)[ \t]*,[ \t]*(read|write)[ \t]*>;";
const TEXTURE_STORAGE_RE: OnceCell<regex::Regex> = OnceCell::new();

static RE_TEXTURE_CUBE: &str = r"var ([a-z0-9_]+): texture_cube[ \t]*<[ \t]*([^>]*)[ \t]*>;";
const TEXTURE_CUBE_RE: OnceCell<regex::Regex> = OnceCell::new();

#[test]
fn test_texture_storage_re() {
    let captures = TEXTURE_STORAGE_RE
        .get_or_init(|| regex::Regex::new(RE_TEXTURE_STORAGE).unwrap())
        .captures("var lut: texture_storage_2d<rgba32float, write>;");
    assert!(captures.is_some());
    let captures = captures.as_ref().unwrap();
    assert_eq!(&captures[1], "lut");
    assert_eq!(&captures[2], "2d");
    assert_eq!(&captures[3], "rgba32float");
    assert_eq!(&captures[4], "write");


    let captures = TEXTURE_STORAGE_RE
        .get_or_init(|| regex::Regex::new(RE_TEXTURE_STORAGE).unwrap())
        .captures("var lut: texture_storage_2d_array<rgba32float, read>;");
    assert!(captures.is_some());
    let captures = captures.as_ref().unwrap();
    assert_eq!(&captures[1], "lut");
    assert_eq!(&captures[2], "2d_array");
    assert_eq!(&captures[3], "rgba32float");
    assert_eq!(&captures[4], "read");
}

#[test]
fn test_texture_cube_re() {
    let captures = TEXTURE_CUBE_RE
        .get_or_init(|| regex::Regex::new(RE_TEXTURE_CUBE).unwrap())
        .captures("var envmap: texture_cube<f32>;");
    assert!(captures.is_some());
    let captures = captures.as_ref().unwrap();
    assert_eq!(&captures[1], "envmap");
    assert_eq!(&captures[2], "f32");
}


#[test]
fn test_constant_re() {
    let captures = CONST_RE
        .get_or_init(|| regex::Regex::new(RE_CONSTANTS).unwrap())
        .captures("const ENVIRONMENT_SCALE: f32 = 2.0;");
    assert!(captures.is_some());
    let captures = captures.as_ref().unwrap();
    assert_eq!(&captures[1], "ENVIRONMENT_SCALE");
    assert_eq!(&captures[2], ":");
    assert_eq!(&captures[3], "f32");
    assert_eq!(&captures[4], "2.0");
    let captures = CONST_RE
        .get_or_init(|| regex::Regex::new(RE_CONSTANTS).unwrap())
        .captures("const ENVIRONMENT_SCALE = 2.0;");
    assert!(captures.is_some());
    let captures = captures.as_ref().unwrap();
    assert_eq!(&captures[1], "ENVIRONMENT_SCALE");
    assert_eq!(captures.get(2), None);
    assert_eq!(captures.get(3), None);
    assert_eq!(&captures[4], "2.0");
}

// Sets constants values in the shader source
pub fn set_constants(shader_src: &str, constants: &[(&str, Cow<str>)]) -> String {
    let mut new_shader_src = String::new();
    for line in shader_src.lines() {
        let captures = CONST_RE
            .get_or_init(|| regex::Regex::new(RE_CONSTANTS).unwrap())
            .captures(line);
        if let Some(captures) = captures {
            let const_name = &captures[1];
            let ty = captures.get(3).map(|ty| ty.as_str());
            let new_value = constants.iter()
                .find(|(name, _)| *name == const_name)
                .map(|(_, value)| value);
            if let Some(new_value) = new_value {
                let new_line = if let Some(ty) = ty {
                    format!("const {const_name}: {ty} = {new_value};")
                }else{
                    format!("const {const_name} = {new_value};")
                };

                new_shader_src += &new_line;
            }else{
                new_shader_src += line;
            }
        }else{
            new_shader_src += line;
        }
        new_shader_src += "\n";
    }

    new_shader_src
}

// Changes the texture format in the shader source
pub fn set_texture_format(shader_src: &str, texture_formats: &[(&str, wgpu::TextureFormat)]) -> String {
    let mut new_shader_src = String::new();

    for line in shader_src.lines() {
        let captures = TEXTURE_STORAGE_RE
            .get_or_init(|| regex::Regex::new(RE_TEXTURE_STORAGE).unwrap())
            .captures(line);
        if let Some(captures) = captures {
            let texture_name = &captures[1];
            let new_ty = texture_formats.iter()
                .find(|(name, _)| *name == texture_name)
                .map(|(_, format)| format.to_wgsl_storage_str());
            if let Some(new_ty) = new_ty {
                let tex_storage = &captures[2];
                let rw = &captures[4];
                let new_line = format!("var {texture_name}: texture_storage_{tex_storage}<{new_ty}, {rw}>;");
                new_shader_src += &new_line;
            }else{
                new_shader_src += line;
            }
        }else{
            let captures = TEXTURE_CUBE_RE
                .get_or_init(|| regex::Regex::new(RE_TEXTURE_CUBE).unwrap())
                .captures(line);

            if let Some(captures) = captures {
                let texture_name = &captures[1];
                let new_ty = texture_formats.iter()
                    .find(|(name, _)| *name == texture_name)
                    .map(|(_, format)| format.to_wgsl_texture_str());
                if let Some(new_ty) = new_ty {
                    let new_line = format!("var {texture_name}: texture_cube<{new_ty}>;");
                    new_shader_src += &new_line;
                }else{
                    new_shader_src += line;
                }
            }else{
                new_shader_src += line;
            }
        }
        new_shader_src += "\n";
    }

    new_shader_src
}