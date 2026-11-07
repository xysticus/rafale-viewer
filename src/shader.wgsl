struct Params {
    // Rectangle de destination en coordonnées normalisées : (gauche, haut, droite, bas).
    dst: vec4<f32>,
    // Rectangle source dans l'image *orientée*, en [0, 1] : (gauche, haut, droite, bas).
    src: vec4<f32>,
    // Orientation EXIF (1 à 8).
    orientation: u32,
    // Hauteur / largeur de la fenêtre, et rotation du quad autour de son centre
    // (cosinus, sinus ; 0, 0 = aucune) : ligne du Live GND.
    aspect: f32,
    rotation: vec2<f32>,
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
    var p = mix(params.dst.xy, params.dst.zw, c);
    if any(params.rotation != vec2<f32>(0.0, 0.0)) {
        // Rotation dans le repère des pixels (y vers le bas), proportions de la fenêtre comprises.
        let m = (params.dst.xy + params.dst.zw) * 0.5;
        let r = (p - m) * vec2<f32>(1.0, -params.aspect);
        let q = vec2<f32>(
            r.x * params.rotation.x - r.y * params.rotation.y,
            r.x * params.rotation.y + r.y * params.rotation.x,
        );
        p = m + q * vec2<f32>(1.0, -1.0 / params.aspect);
    }
    out.pos = vec4<f32>(p, 0.0, 1.0);
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
