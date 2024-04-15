// Ported from https://github.com/KhronosGroup/glTF-Sample-Viewer/blob/main/source/shaders/ibl_filtering.frag
// Copyright Khronos Group
// Apache license 2.0
// Port WGSL Copyright Arturo Castro Prieto

@group(0)
@binding(0)
var envmap: texture_cube<f32>;

@group(0)
@binding(1)
var envmap_sampler: sampler;

@group(0)
@binding(2)
var output_faces: texture_storage_2d_array<rgba32float, write>;

struct RadianceData {
	mip_level: u32,
	max_mips: u32,
}
@group(1)
@binding(0)
var<uniform> radiance_data: RadianceData;

const INV_ATAN: vec2<f32> = vec2(0.1591, 0.3183);
const CP_UDIR = 0u;
const CP_VDIR = 1u;
const CP_FACEAXIS = 2u;
const M_PI = 3.1415926535897932384626433832795;
const M_INV_PI = 0.31830988618;
const NUM_SAMPLES = 128u;

const STRENGTH: f32 = 1.0;
const CONTRAST_CORRECTION: f32 = 1.;
const BRIGHTNESS_CORRECTION: f32 = 1.;
const SATURATION_CORRECTION: f32 = 1.;
const HUE_CORRECTION = 0.;
const ROOT: vec3<f32> = vec3(0.57735, 0.57735, 0.57735);
const FLIP_Y = false;

const LAMBERT = 0;
const GGX = 1;

fn face_2d_mapping(face: u32) -> array<vec3f, 3> {
    //XPOS face
	if(face==0u) {
		return array<vec3f, 3>(
		     vec3(0.,  0., -1.),   //u towards negative Z
		     vec3(0., -1.,  0.),   //v towards negative Y
		     vec3(1.,  0.,  0.)
        );  //pos X axis
    }
    //XNEG face
	if(face==1u) {
		return array<vec3f, 3>(
		      vec3(0.,  0.,  1.),   //u towards positive Z
		      vec3(0., -1.,  0.),   //v towards negative Y
		      vec3(-1.,  0., 0.)
        );  //neg X axis
    }
    //YPOS face
	if(face==2u) {
		return array<vec3f, 3>(
		     vec3(1., 0., 0.),     //u towards positive X
		     vec3(0., 0. , -1.),   //v towards negative Z
		     vec3(0., -1. , 0.)
        );  //neg Y axis
    }
    //YNEG face
	if(face==3u) {
		return array<vec3f, 3>(
		     vec3(1., 0., 0.),     //u towards positive X
		     vec3(0., 0., 1.),     //v towards positive Z
		     vec3(0., 1., 0.)
        );   //pos Y axis
    }
    //ZPOS face
	if(face==4u) {
		return array<vec3f, 3>(
		     vec3(1., 0., 0.),     //u towards positive X
		     vec3(0., -1., 0.),    //v towards negative Y
		     vec3(0., 0.,  1.)
        );   //pos Z axis
    }
    //ZNEG face
	if(face==5u) {
		return array<vec3f, 3>(
		     vec3(-1., 0., 0.),    //u towards negative X
		     vec3(0., -1., 0.),    //v towards negative Y
		     vec3(0., 0., -1.)
        );   //neg Z axis
    }

	return array<vec3f, 3>(
		vec3(-0., 0., 0.),    //u towards negative X
		vec3(0., -0., 0.),    //v towards negative Y
		vec3(0., 0., -0.)
	);   //ne
}

fn signed_uv_face_to_cubemap_xyz(uv: vec2f, face_idx: u32) -> vec3f{
	let coords = face_2d_mapping(face_idx);
	// Get current vector
	//generate x,y,z vector (xform 2d NVC coord to 3D vector)
	//U contribution
	let xyz_u = coords[0] * uv.x; // TODO: CP_UDIR but the compiler considers it dynamic indexing and fails
	//V contribution
	let xyz_v = coords[1] * uv.y; // CP_VDIR
	var xyz = xyz_u + xyz_v;
	//add face axis
	xyz = coords[2] + xyz; // CP_FACEAXIS
	//normalize vector
	return normalize(xyz);
}

fn uv_face_to_cubemap_xyz(uv: vec2<f32>, face_idx: u32) -> vec3f {
	if FLIP_Y {
		let nuv = vec2(uv.x, 1. - uv.y) * 2.0 - vec2(1.0);
		return signed_uv_face_to_cubemap_xyz(nuv, face_idx);
	}else{
		let nuv = uv * 2.0 - vec2(1.0);
		return signed_uv_face_to_cubemap_xyz(nuv, face_idx);
	}
}

fn radicalInverse_VdC(bits: u32) -> f32 {
	var b = bits;
	b = (b << 16u) | (b >> 16u);
	b = ((b & 0x55555555u) << 1u) | ((b & 0xAAAAAAAAu) >> 1u);
	b = ((b & 0x33333333u) << 2u) | ((b & 0xCCCCCCCCu) >> 2u);
	b = ((b & 0x0F0F0F0Fu) << 4u) | ((b & 0xF0F0F0F0u) >> 4u);
	b = ((b & 0x00FF00FFu) << 8u) | ((b & 0xFF00FF00u) >> 8u);
	return f32(b) * 2.3283064365386963e-10; // / 0x100000000
 }

// http://holger.dammertz.org/stuff/notes_HammersleyOnHemisphere.html
fn hammersley(i: u32, n: u32) -> vec2f {
     return vec2(f32(i)/f32(n), radicalInverse_VdC(i));
}

// TBN generates a tangent bitangent normal coordinate frame from the normal
// (the normal must be normalized)
fn generate_tbn(normal: vec3f) -> mat3x3<f32> {
    var bitangent = vec3(0.0, 1.0, 0.0);

    let NdotUp = dot(normal, vec3(0.0, 1.0, 0.0));
    let epsilon = 0.0000001;
    if (1.0 - abs(NdotUp) <= epsilon)
    {
        // Sampling +Y or -Y, so we need a more robust bitangent.
        if (NdotUp > 0.0)
        {
            bitangent = vec3(0.0, 0.0, 1.0);
        }
        else
        {
            bitangent = vec3(0.0, 0.0, -1.0);
        }
    }

    let tangent = normalize(cross(bitangent, normal));
    bitangent = cross(normal, tangent);

    return mat3x3(tangent, bitangent, normal);
}

struct MicrofacetDistributionSample {
	phi: f32,
	cos_theta: f32,
	sin_theta: f32,
	pdf: f32,
}

fn d_ggx(linear_roughness: f32, ndh: f32) -> f32{
	let a = ndh * linear_roughness;
    let k = linear_roughness / (1.0 - ndh * ndh + a * a);
    return k * k * (1.0 / M_PI);
}

fn importance_sample_ggx(e: vec2f, linear_roughness: f32, n: vec3f ) -> MicrofacetDistributionSample {
	let m = linear_roughness;

	let phi = 2. * M_PI * e.x;
	let cos_theta = saturate(sqrt( (1. - e.y) / ( 1. + (m*m - 1.) * e.y ) ));
	let sin_theta = sqrt( 1. - cos_theta * cos_theta );

	let pdf = d_ggx(linear_roughness, cos_theta);

	return MicrofacetDistributionSample (
		phi,
		cos_theta,
		sin_theta,
		pdf
	);
}

fn importance_sample_diffuse(e: vec2f, n: vec3f) -> MicrofacetDistributionSample {
	// Cosine weighted hemisphere sampling
    // http://www.pbr-book.org/3ed-2018/Monte_Carlo_Integration/2D_Sampling_with_Multidimensional_Transformations.html#Cosine-WeightedHemisphereSampling
    let cos_theta = sqrt(1.0 - e.y);
    let sin_theta = sqrt(e.y); // equivalent to `sqrt(1.0 - cosTheta*cosTheta)`;
	let phi = 2. * M_PI * e.x;

	let pdf = cos_theta / M_PI;


	return MicrofacetDistributionSample (
		phi,
		cos_theta,
		sin_theta,
		pdf
	);
}

fn importance_sample(sample: u32, linear_roughness: f32, n: vec3f, distribution: i32) -> vec4f {
	let xi = hammersley(sample, NUM_SAMPLES);
	var importance_sample: MicrofacetDistributionSample;
	if(distribution==LAMBERT) {
		importance_sample = importance_sample_diffuse(xi, n);
	}else if (distribution == GGX) {
		importance_sample = importance_sample_ggx(xi, linear_roughness, n);
	}else{
		// unrecheable
		importance_sample = importance_sample_ggx(xi, linear_roughness, n);
	}


	let h = vec3(
		importance_sample.sin_theta * cos( importance_sample.phi ),
		importance_sample.sin_theta * sin( importance_sample.phi ),
		importance_sample.cos_theta
	);

	let tbn = generate_tbn(n);
	return vec4(tbn * h, importance_sample.pdf);
}

fn quaternion_to_matrix(quat: vec4f) -> mat3x3<f32> {
    let cross = quat.yzx * quat.zxy;
    var square= quat.xyz * quat.xyz;
    let wimag = quat.w * quat.xyz;

    square = square.xyz + square.yzx;

    let diag = 0.5 - square;
    let a = (cross + wimag);
    let b = (cross - wimag);

    return mat3x3(
		2.0 * vec3(diag.x, b.z, a.y),
		2.0 * vec3(a.z, diag.y, b.x),
		2.0 * vec3(b.y, a.x, diag.z)
	);
}

fn correction(color: vec3f) -> vec3f {
	// Contrast
	var hdr = mix(vec3(0.18), color, CONTRAST_CORRECTION);

	// Brightness
	hdr = mix(vec3(0.), hdr, vec3(BRIGHTNESS_CORRECTION));

	// Saturation
	hdr = mix(vec3(dot(vec3(1.), hdr) * 0.3333), hdr, SATURATION_CORRECTION);

	// Hue
	let half_angle = 0.5 * radians(HUE_CORRECTION); // Hue is radians of 0 to 360 degree
	let rot_quat = vec4( (ROOT * sin(half_angle)), cos(half_angle));
	let rot_matrix = quaternion_to_matrix(rot_quat);
	hdr = rot_matrix * hdr;
	hdr = hdr * STRENGTH;

	return hdr;
}

fn compute_lod(pdf: f32) -> f32 {
	let resolution = f32(textureDimensions(envmap).x);

	// Compute Lod using inverse solid angle and pdf.
	// From Chapter 20.4 Mipmap filtered samples in GPU Gems 3.
	// http://http.developer.nvidia.com/GPUGems3/gpugems3_ch20.html
	// let sa_texel = 4.0 * M_PI / (6.0 * resolution * resolution);
	// let sa_sample = 1.0 / (f32(NUM_SAMPLES) * pdf + 0.0001);
	// let lod = 0.5 * log2(sa_sample / sa_texel);

	// https://cgg.mff.cuni.cz/~jaroslav/papers/2007-sketch-fis/Final_sap_0073.pdf
	let lod = 0.5 * log2( 6.0 * resolution * resolution / (f32(NUM_SAMPLES) * pdf));

	return lod;
}

@compute
@workgroup_size(1)
fn radiance(@builtin(global_invocation_id) global_id: vec3<u32>) {
	let resolution = f32(textureDimensions(output_faces).x);
    let texel = (vec2<f32>(global_id.xy) + vec2(0.5)) / resolution;
    let face = global_id.z;
	let roughness = f32(radiance_data.mip_level) / f32(radiance_data.max_mips - 1u);
	let linear_roughness = roughness * roughness;
    let v = uv_face_to_cubemap_xyz(texel, face);
	let n = v;

	var total_radiance = vec4(0.);
	for(var sample = 0u; sample < NUM_SAMPLES; sample += 1u){
		let importance_sample = importance_sample(sample, linear_roughness, n, GGX);
		let h = importance_sample.xyz;
		let pdf = importance_sample.w;
		var l = normalize(reflect(-v, h));
		l.y *= -1.;
		let ndl = dot(n, l);

		if (ndl > 0.){
			var mip_level = 0.0;
			if (roughness != 0.0) {
				mip_level = compute_lod(pdf);
			}
	        let pointRadiance = correction(textureSampleLevel(envmap, envmap_sampler, l, mip_level).rgb);
	        total_radiance += vec4(pointRadiance * ndl, ndl);
	    }
	}

	var color = vec4(0.);
	if (total_radiance.w == 0.){
		color = vec4(total_radiance.rgb, 1.);
	}else{
		color = vec4(total_radiance.rgb / total_radiance.w, 1.);
	}

    textureStore(
		output_faces,
		vec2<i32>(global_id.xy),
		i32(face),
		color
	);
}

@compute
@workgroup_size(1)
fn irradiance(@builtin(global_invocation_id) global_id: vec3<u32>){
	let resolution = f32(textureDimensions(output_faces).x);
    let texel = (vec2<f32>(global_id.xy) + vec2(0.5)) / resolution;
    let face = global_id.z;
    let v = uv_face_to_cubemap_xyz(texel, face);
	let n = v;

	var total_irradiance = vec4(0.);
	for(var sample = 0u; sample < NUM_SAMPLES; sample += 1u){
		let importance_sample = importance_sample(sample, 0., n, LAMBERT);
		var h = importance_sample.xyz;
		h.y *= -1.;
		let pdf = importance_sample.w;

		let lod = compute_lod(pdf);
		let diffuseSample = correction(textureSampleLevel(envmap, envmap_sampler, h, lod).rgb);
		total_irradiance += vec4(diffuseSample, 1.);
	}

	var color = vec4(0.);
	if (total_irradiance.w == 0.){
		color = vec4(total_irradiance.rgb, 1.);
	}else{
		color = vec4(total_irradiance.rgb / total_irradiance.w, 1.);
	}

    textureStore(
		output_faces,
		vec2<i32>(global_id.xy),
		i32(face),
		color
	);
}