// Instanced sprite shader with per-pixel isometric depth.
//
// Every instance is expanded into two quads (12 vertices): the left quad spans u in [0, split],
// the right quad u in [split, 1]. The depth function (see rendering/iso.rs) is evaluated per
// fragment and written to the depth buffer, which yields correct isometric occlusion without
// CPU-side painter sorting for opaque sprites.

struct Globals {
    view_proj: mat4x4<f32>,
    ambient: vec4<f32>,
    // x: time in seconds, y: iso depth range, z: alpha-test threshold, w: wind lean in pixels
    params: vec4<f32>,
    // x: wind sway amplitude, y: wind time base in seconds, z: snow cover 0..1, w: wetness 0..1
    environment: vec4<f32>,
    // x: screen direction the wind blows towards (-1 left .. 1 right), y: strength of painted
    // cast shadows 0..1, zw: unused
    wind: vec4<f32>,
};

@group(0) @binding(0) var<uniform> globals: Globals;
@group(1) @binding(0) var sprite_texture: texture_2d<f32>;
@group(1) @binding(1) var sprite_sampler: sampler;

const FLAG_WIND: u32 = 1u;
const FLAG_WATER: u32 = 2u;
const FLAG_EMISSIVE: u32 = 4u;
const FLAG_FLIP_X: u32 = 8u;
const FLAG_GROUND: u32 = 16u;
const FLAG_DEPTH_MAX: u32 = 32u;
const FLAG_TRODDEN: u32 = 64u;
const FLAG_SNOW_ON_GREEN: u32 = 128u;
const FLAG_SNOW_ON_ROOF: u32 = 256u;
const FLAG_SHADOW_MARK: u32 = 512u;

// Texture alphas (of 255) that mark pixels of map sprites (docs/assets.md): snow settles there,
// or a painted cast shadow darkens them.
const SNOW_MARK: f32 = 254.0;
const SHADOW_MARK: f32 = 253.0;
// How cast shadows are painted: this colour at this opacity over the ground, in sRGB.
const SHADOW_SRGB: vec3<f32> = vec3<f32>(26.0, 18.0, 8.0) / 255.0;
const SHADOW_OPACITY: f32 = 72.0 / 255.0;

struct InstanceIn {
    @location(0) position: vec2<f32>,
    @location(1) size: vec2<f32>,
    @location(2) anchor: vec2<f32>,
    @location(3) uv_rect: vec4<f32>,
    @location(4) color: vec4<f32>,
    @location(5) depth_tent: vec4<f32>,
    @location(6) depth_flat: vec2<f32>,
    @location(7) flags: u32,
    @location(8) phase: f32,
};

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) local_u: f32,
    @location(3) rel_y: f32,
    @location(4) world: vec2<f32>,
    @location(5) @interpolate(flat) depth_tent: vec4<f32>,
    @location(6) @interpolate(flat) depth_flat: vec2<f32>,
    @location(7) @interpolate(flat) flags: u32,
    @location(8) rel_x: f32,
    // Ground-cover sway (FLAG_WIND with FLAG_GROUND): x/y the u range of the sprite in the
    // atlas, z the shift of u per pixel of height above the tile diamond.
    @location(9) @interpolate(flat) shear: vec4<f32>,
};

// Height (px) of plants over which the ground-cover sway reaches its full displacement.
const COVER_SWAY_HEIGHT: f32 = 40.0;

// Horizontal displacement (px) of the top of a FLAG_WIND sprite: gusts that push downwind
// (evenly to both sides in calm air) and travel across the map with the wind, a faster flutter
// and the steady lean.
fn wind_displacement(inst_phase: f32, position_x: f32) -> f32 {
    let t = globals.environment.y;
    let strength = globals.environment.x;
    let dir = globals.wind.x;
    let travel = select(-1.0, 1.0, dir > 0.0);
    let wave = sin(t * 1.6 + inst_phase - travel * position_x * 0.011);
    let gust = mix(wave, sign(dir) * (0.5 + 0.5 * wave), abs(dir));
    let flutter = 0.35 * sin(t * 3.7 + inst_phase * 1.3);
    return (gust + flutter) * strength + globals.params.w;
}

// Corner pattern of one quad as two triangles.
fn quad_corner(i: u32) -> vec2<f32> {
    switch i {
        case 0u: { return vec2<f32>(0.0, 0.0); }
        case 1u: { return vec2<f32>(1.0, 0.0); }
        case 2u: { return vec2<f32>(0.0, 1.0); }
        case 3u: { return vec2<f32>(0.0, 1.0); }
        case 4u: { return vec2<f32>(1.0, 0.0); }
        default: { return vec2<f32>(1.0, 1.0); }
    }
}

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32, inst: InstanceIn) -> VsOut {
    let quad = vertex_index / 6u;
    let corner = quad_corner(vertex_index % 6u);
    let split = clamp(inst.depth_tent.w, 0.0, 1.0);
    var u: f32;
    if quad == 0u {
        u = corner.x * split;
    } else {
        u = split + corner.x * (1.0 - split);
    }
    let v = corner.y;

    var world = inst.position + (vec2<f32>(u, v) - inst.anchor) * inst.size;
    let rel_y = world.y - inst.position.y;

    let rel_x = world.x - inst.position.x;

    var tex_u = u;
    if (inst.flags & FLAG_FLIP_X) != 0u {
        tex_u = 1.0 - u;
    }

    var shear = vec4<f32>(min(inst.uv_rect.x, inst.uv_rect.z), max(inst.uv_rect.x, inst.uv_rect.z), 0.0, 0.0);
    if (inst.flags & FLAG_WIND) != 0u {
        let d = wind_displacement(inst.phase, inst.position.x);
        if (inst.flags & FLAG_GROUND) != 0u {
            // Ground cover: the quad stays, the fragment shader shears the plants.
            var du = (inst.uv_rect.z - inst.uv_rect.x) / max(inst.size.x, 1.0);
            if (inst.flags & FLAG_FLIP_X) != 0u {
                du = -du;
            }
            shear.z = d / COVER_SWAY_HEIGHT * du;
        } else {
            world.x += d * (1.0 - v) * clamp(inst.size.y / 48.0, 0.3, 2.0);
        }
    }

    var out: VsOut;
    out.clip = globals.view_proj * vec4<f32>(world, 0.0, 1.0);
    out.uv = vec2<f32>(mix(inst.uv_rect.x, inst.uv_rect.z, tex_u), mix(inst.uv_rect.y, inst.uv_rect.w, v));
    out.color = inst.color;
    out.local_u = u;
    out.rel_y = rel_y;
    out.world = world;
    out.depth_tent = inst.depth_tent;
    out.depth_flat = inst.depth_flat;
    out.flags = inst.flags;
    out.rel_x = rel_x;
    out.shear = shear;
    return out;
}

fn iso_depth(in: VsOut) -> f32 {
    let flat_d = in.depth_flat.x + in.depth_flat.y * in.rel_y;
    let split = in.depth_tent.w;
    var tent: f32;
    if in.local_u <= split {
        let t = in.local_u / max(split, 1e-6);
        tent = mix(in.depth_tent.x, in.depth_tent.y, t);
    } else {
        let t = (in.local_u - split) / max(1.0 - split, 1e-6);
        tent = mix(in.depth_tent.y, in.depth_tent.z, t);
    }
    var d = min(flat_d, tent);
    if (in.flags & FLAG_DEPTH_MAX) != 0u {
        d = max(flat_d, tent);
    }
    return clamp(1.0 - d / globals.params.y, 0.0, 1.0);
}

fn hash2(p: vec2<f32>) -> f32 {
    return fract(sin(dot(p, vec2<f32>(127.1, 311.7))) * 43758.5453);
}

// Smooth value noise in [0, 1].
fn value_noise(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let a = hash2(i);
    let b = hash2(i + vec2<f32>(1.0, 0.0));
    let c = hash2(i + vec2<f32>(0.0, 1.0));
    let d = hash2(i + vec2<f32>(1.0, 1.0));
    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}

// Blends `c` towards snow where the patchy cover for `snow` (0..1) lies at world point `w`;
// `share` scales the result (0..1).
fn snow_over(c: vec3<f32>, w: vec2<f32>, snow: f32, share: f32) -> vec3<f32> {
    // Drifts appear first where the noise is low, everything is white at 1.
    let n = value_noise(w * vec2<f32>(0.027, 0.043)) * 0.6 + value_noise(w * 0.11) * 0.3 + value_noise(w * 0.37) * 0.1;
    let cover = smoothstep(0.0, 0.4, snow * 1.6 - n * 0.6);
    let luma = dot(c, vec3<f32>(0.3, 0.59, 0.11));
    let snow_rgb = vec3<f32>(0.80, 0.85, 0.93) * (0.9 + 0.45 * luma);
    return mix(c, snow_rgb, cover * 0.95 * share);
}

// Snow cover and wetness on the tile diamond around the anchor of FLAG_GROUND sprites.
fn ground_weather(c: vec4<f32>, in: VsOut) -> vec4<f32> {
    if abs(in.rel_x) / 32.0 + abs(in.rel_y) / 16.0 > 1.02 {
        return c;
    }
    let wet = globals.environment.w;
    var rgb = c.rgb * (1.0 - 0.26 * wet) * mix(vec3<f32>(1.0), vec3<f32>(0.94, 0.97, 1.03), wet);
    var snow = globals.environment.z;
    if (in.flags & FLAG_TRODDEN) != 0u {
        snow = snow * 0.4;
    }
    if snow > 0.0 {
        rgb = snow_over(rgb, in.world, snow, 1.0);
    }
    return vec4<f32>(rgb, c.a);
}

// Snow on the green pixels (lawns, hedges, leaves) of FLAG_SNOW_ON_GREEN sprites.
fn green_snow(c: vec4<f32>, in: VsOut) -> vec4<f32> {
    let snow = globals.environment.z;
    let green = c.g - max(c.r, c.b);
    if snow <= 0.0 || green <= 0.0 {
        return c;
    }
    let share = smoothstep(0.0, 0.05, green);
    return vec4<f32>(snow_over(c.rgb, in.world, snow, share), c.a);
}

// The marks at texture coordinate `uv`: x the share of snow-marked texels, y the share of
// shadow-marked texels among the four texels bilinear filtering reads, weighted like the filter;
// z is 1 if all four are opaque (marked or not). The alphas come exact from the full-size
// texture, so filtering and mipmaps (which average marked and plain texels) cannot turn the edge
// of one mark into the other.
fn marks(uv: vec2<f32>) -> vec3<f32> {
    let size = vec2<f32>(textureDimensions(sprite_texture));
    let a = textureGather(3, sprite_texture, sprite_sampler, uv) * 255.0;
    let f = fract(uv * size - 0.5);
    // Gather order: x (u0, v1), y (u1, v1), z (u1, v0), w (u0, v0).
    let w = vec4<f32>((1.0 - f.x) * f.y, f.x * f.y, f.x * (1.0 - f.y), (1.0 - f.x) * (1.0 - f.y));
    let one = vec4<f32>(1.0);
    let zero = vec4<f32>(0.0);
    let snow = dot(w, select(zero, one, abs(a - SNOW_MARK) < vec4<f32>(0.5)));
    let shadow = dot(w, select(zero, one, abs(a - SHADOW_MARK) < vec4<f32>(0.5)));
    let solid = select(0.0, 1.0, min(min(a.x, a.y), min(a.z, a.w)) > SHADOW_MARK - 0.5);
    return vec3<f32>(snow, shadow, solid);
}

// Snow on the surfaces a FLAG_SNOW_ON_ROOF sprite marks (roofs, flat tops); `share` is the
// snow-marked share of the fragment ([`marks`]).
fn roof_snow(c: vec4<f32>, share: f32, in: VsOut) -> vec4<f32> {
    let snow = globals.environment.z;
    if snow <= 0.0 || share <= 0.0 {
        return c;
    }
    return vec4<f32>(snow_over(c.rgb, in.world, snow, share), c.a);
}

// A painted cast shadow covering `share` of the fragment (texture colour `rgb`, linear) weakened
// to `strength` (1 = as painted): undoes the painter's blend in sRGB and blends again.
fn lift_shadow(rgb: vec3<f32>, share: f32, strength: f32) -> vec3<f32> {
    let s = pow(max(rgb, vec3<f32>(0.0)), vec3<f32>(1.0 / 2.2));
    let a = SHADOW_OPACITY * share;
    let lit = (s - SHADOW_SRGB * a) / (1.0 - a);
    let b = a * strength;
    let shown = lit * (1.0 - b) + SHADOW_SRGB * b;
    return pow(clamp(shown, vec3<f32>(0.0), vec3<f32>(1.0)), vec3<f32>(2.2));
}

// Texture coordinate of a fragment: ground cover samples its plants from where the wind moved
// them away, by the pixel's height above the back edges of the tile diamond (the diamond itself
// and everything at ground level stay in place). The horizontal distance to the diamond grows
// twice as fast as that height, so a sheared pixel never samples the ground decal.
fn sway_uv(in: VsOut) -> vec2<f32> {
    let height = max(0.0, -in.rel_y - 16.0 * (1.0 - abs(in.rel_x) / 32.0));
    let u = clamp(in.uv.x - height * in.shear.z, in.shear.x, in.shear.y);
    return vec2<f32>(u, in.uv.y);
}

fn shade(in: VsOut) -> vec4<f32> {
    // Sampled unconditionally (uniform control flow); the shear is 0 for other sprites.
    let uv = sway_uv(in);
    let tex = textureSample(sprite_texture, sprite_sampler, uv);
    var base = tex;
    var m = vec3<f32>(0.0);
    if (in.flags & (FLAG_SNOW_ON_ROOF | FLAG_SHADOW_MARK)) != 0u {
        m = marks(uv);
        // Marked pixels are opaque: their alpha is only the mark.
        if m.z > 0.5 {
            base.a = 1.0;
        }
        let strength = globals.wind.y;
        if (in.flags & FLAG_SHADOW_MARK) != 0u && m.y > 0.0 && strength < 1.0 {
            base = vec4<f32>(lift_shadow(base.rgb, m.y, strength), base.a);
        }
    }
    var c = base * in.color;
    if (in.flags & FLAG_WATER) != 0u {
        let t = globals.params.x;
        let w = sin(in.world.x * 0.045 + t * 1.3) * sin(in.world.y * 0.09 - t * 0.9);
        let sparkle = smoothstep(0.82, 1.0, w);
        c = vec4<f32>(c.rgb * (0.96 + 0.06 * w) + vec3<f32>(sparkle * 0.18), c.a);
    }
    if (in.flags & FLAG_GROUND) != 0u {
        c = ground_weather(c, in);
    }
    if (in.flags & FLAG_SNOW_ON_GREEN) != 0u {
        c = green_snow(c, in);
    }
    if (in.flags & FLAG_SNOW_ON_ROOF) != 0u {
        c = roof_snow(c, m.x, in);
    }
    if (in.flags & FLAG_EMISSIVE) == 0u {
        c = vec4<f32>(c.rgb * globals.ambient.rgb, c.a);
    }
    return c;
}

struct FsOut {
    @location(0) color: vec4<f32>,
    @builtin(frag_depth) depth: f32,
};

// Opaque pass: alpha test (or alpha-to-coverage with MSAA), depth write on.
@fragment
fn fs_opaque(in: VsOut) -> FsOut {
    let c = shade(in);
    if c.a < globals.params.z {
        discard;
    }
    var out: FsOut;
    out.color = c;
    out.depth = iso_depth(in);
    return out;
}

// Transparent pass: blended, depth test on, depth write off.
@fragment
fn fs_transparent(in: VsOut) -> FsOut {
    let c = shade(in);
    if c.a < 0.004 {
        discard;
    }
    var out: FsOut;
    out.color = c;
    out.depth = iso_depth(in);
    return out;
}
