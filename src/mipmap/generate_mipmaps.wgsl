@group(0)
@binding(0)
var input: texture_storage_2d_array<rgba32float, read>;

@group(0)
@binding(1)
var output: texture_storage_2d_array<rgba32float, write>;


fn floor_frac(x: f32, floor: ptr<function, i32>, frac: ptr<function, f32>) {
    let ffloor = floor(x);
    *floor = i32(ffloor);
    *frac = x - ffloor;
}

@compute
@workgroup_size(1)
fn generate_mipmaps(@builtin(global_invocation_id) global_id: vec3<u32>) {
	let face = i32(global_id.z);
	let out_uv = vec2<i32>(global_id.xy);
    let in_uv = out_uv * 2;
	let fin_uv = (vec2<f32>(out_uv) + vec2(0.5)) / vec2<f32>(textureDimensions(output)) * vec2<f32>(textureDimensions(input));
    let x = fin_uv.x - 0.5;
    let y = fin_uv.y - 0.5;
    var fx1: f32;
    var px: i32;
    floor_frac(x, &px, &fx1);
    var fy1: f32;
    var py: i32;
    floor_frac(x, &py, &fy1);

    // Linear
    let fx2 = 1. - fx1;
    let fy2 = 1. - fy1;
    let p11 = textureLoad(input, in_uv + vec2(0, 0), face);
    let p21 = textureLoad(input, in_uv + vec2(1, 0), face);
    let p12 = textureLoad(input, in_uv + vec2(0, 1), face);
    let p22 = textureLoad(input, in_uv + vec2(1, 1), face);
    let color = p11 * fx2 * fy2 + p21 * fx1 * fy2 + p12 * fx2 * fy1 + p22 * fx1 * fy1;
    textureStore(output, out_uv, face, color);
}
