@group(0)
@binding(0)
var equirectangular: texture_storage_2d<rgba32float, read>;

@group(0)
@binding(1)
var cubemap_faces: texture_storage_2d_array<rgba32float, write>;


const INV_ATAN: vec2<f32> = vec2(0.1591, 0.3183);
const CP_UDIR = 0u;
const CP_VDIR = 1u;
const CP_FACEAXIS = 2u;
const FLIP_Y = false;

fn sample_spherical_map(v: vec3f) -> vec2f
{
    var uv: vec2f = vec2<f32>(atan2(v.z, v.x), asin(v.y));
    uv *= INV_ATAN;
    uv += 0.5;
    return uv;
}

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

@compute
@workgroup_size(1)
fn equirectangular_to_cubemap(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let texel = (vec2<f32>(global_id.xy) + vec2(0.5)) / f32(textureDimensions(cubemap_faces).x);
    let face = global_id.z;
    let v = uv_face_to_cubemap_xyz(texel, face);
    let uv = vec2<i32>(sample_spherical_map(v) * vec2<f32>(textureDimensions(equirectangular)));
    let color = textureLoad(equirectangular, uv);
    textureStore(
		cubemap_faces,
		vec2<i32>(global_id.xy),
		i32(face),
		color
	);
}
