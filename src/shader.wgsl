struct Params {
    // Rectangle de destination en coordonnées normalisées : (gauche, haut, droite, bas).
    dst: vec4<f32>,
    // Rectangle source dans l'image *orientée*, en [0, 1] : (gauche, haut, droite, bas).
    src: vec4<f32>,
    // Orientation EXIF (1 à 8).
    orientation: u32,
};

@group(0) @binding(0) var tex: texture_2d<f32>;
@group(0) @binding(1) var samp: sampler;
@group(0) @binding(2) var<uniform> params: Params;

struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

// Quad en triangle strip : 4 sommets, coins (0,0) (1,0) (0,1) (1,1) en espace écran.
@vertex
fn vs_main(@builtin(vertex_index) i: u32) -> VsOut {
    let c = vec2<f32>(f32(i & 1u), f32(i >> 1u));
    var out: VsOut;
    out.pos = vec4<f32>(mix(params.dst.xy, params.dst.zw, c), 0.0, 1.0);
    // Coordonnée dans l'image orientée -> coordonnée dans l'image stockée.
    let d = mix(params.src.xy, params.src.zw, c);
    let u = d.x;
    let v = d.y;
    switch params.orientation {
        case 2u: { out.uv = vec2<f32>(1.0 - u, v); }
        case 3u: { out.uv = vec2<f32>(1.0 - u, 1.0 - v); }
        case 4u: { out.uv = vec2<f32>(u, 1.0 - v); }
        case 5u: { out.uv = vec2<f32>(v, u); }
        case 6u: { out.uv = vec2<f32>(v, 1.0 - u); }
        case 7u: { out.uv = vec2<f32>(1.0 - v, 1.0 - u); }
        case 8u: { out.uv = vec2<f32>(1.0 - v, u); }
        default: { out.uv = vec2<f32>(u, v); }
    }
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    return textureSample(tex, samp, in.uv);
}
