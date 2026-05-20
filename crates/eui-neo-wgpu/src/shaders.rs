use crate::NeoWgpuShaders;

pub const NEO_RECT_SHADER: &str = r#"
struct Screen {
    size: vec4<f32>,
    backdrop_size: vec4<f32>,
    backdrop_rect: vec4<f32>,
};

@group(0) @binding(0)
var<uniform> screen: Screen;

@group(1) @binding(0)
var t_backdrop: texture_2d<f32>;
@group(1) @binding(1)
var s_backdrop: sampler;

struct VsIn {
    @location(0) position: vec2<f32>,
    @location(1) local_pos: vec2<f32>,
    @location(2) rect: vec4<f32>,
    @location(3) fill: vec4<f32>,
    @location(4) gradient_start: vec4<f32>,
    @location(5) gradient_end: vec4<f32>,
    @location(6) border: vec4<f32>,
    @location(7) params: vec4<f32>,
    @location(8) flags: vec4<f32>,
};

struct VsOut {
    @builtin(position) position: vec4<f32>,
    @location(0) local_pos: vec2<f32>,
    @location(1) rect: vec4<f32>,
    @location(2) fill: vec4<f32>,
    @location(3) gradient_start: vec4<f32>,
    @location(4) gradient_end: vec4<f32>,
    @location(5) border: vec4<f32>,
    @location(6) params: vec4<f32>,
    @location(7) flags: vec4<f32>,
};

@vertex
fn vs_main(input: VsIn) -> VsOut {
    var out: VsOut;
    let x = input.position.x / max(screen.size.x, 1.0) * 2.0 - 1.0;
    let y = 1.0 - (input.position.y / max(screen.size.y, 1.0) * 2.0);
    out.position = vec4<f32>(x, y, 0.0, 1.0);
    out.local_pos = input.local_pos;
    out.rect = input.rect;
    out.fill = input.fill;
    out.gradient_start = input.gradient_start;
    out.gradient_end = input.gradient_end;
    out.border = input.border;
    out.params = input.params;
    out.flags = input.flags;
    return out;
}

fn rounded_box_distance(point: vec2<f32>, half_size: vec2<f32>, radius: f32) -> f32 {
    let corner = abs(point) - half_size + vec2<f32>(radius, radius);
    return length(max(corner, vec2<f32>(0.0, 0.0))) + min(max(corner.x, corner.y), 0.0) - radius;
}

fn rand(co: vec2<f32>) -> f32 {
    return fract(sin(dot(co, vec2<f32>(12.9898, 78.233))) * 43758.5453);
}

fn backdrop_blur(uv: vec2<f32>, blur_amount: f32) -> vec3<f32> {
    let pixel_step = 1.0 / max(screen.backdrop_size.xy, vec2<f32>(1.0, 1.0));
    var blurred = textureSampleLevel(t_backdrop, s_backdrop, uv, 0.0).rgb;
    let repeats = mix(8.0, 24.0, clamp(blur_amount / 36.0, 0.0, 1.0));
    let tau = 6.28318530718;
    for (var index: i32 = 0; index < 24; index = index + 1) {
        let i = f32(index);
        if (i >= repeats) {
            break;
        }
        let angle = (i / repeats) * tau;
        let dir = vec2<f32>(cos(angle), sin(angle));
        let radius_a = blur_amount * (0.35 + 0.65 * rand(vec2<f32>(i, uv.x + uv.y)));
        let uv_a = clamp(
            uv + dir * radius_a * pixel_step,
            pixel_step * 0.5,
            vec2<f32>(1.0, 1.0) - pixel_step * 0.5
        );
        blurred += textureSampleLevel(t_backdrop, s_backdrop, uv_a, 0.0).rgb;

        let angle_b = angle + (0.5 * tau / repeats);
        let dir_b = vec2<f32>(cos(angle_b), sin(angle_b));
        let radius_b = blur_amount * (0.20 + 0.80 * rand(vec2<f32>(i + 2.0, uv.x + uv.y + 24.0)));
        let uv_b = clamp(
            uv + dir_b * radius_b * pixel_step,
            pixel_step * 0.5,
            vec2<f32>(1.0, 1.0) - pixel_step * 0.5
        );
        blurred += textureSampleLevel(t_backdrop, s_backdrop, uv_b, 0.0).rgb;
    }
    return blurred / (repeats * 2.0 + 1.0);
}

@fragment
fn fs_main(input: VsOut) -> @location(0) vec4<f32> {
    let radius = clamp(input.params.x, 0.0, min(input.rect.z, input.rect.w) * 0.5);
    let border_width = clamp(input.params.y, 0.0, min(input.rect.z, input.rect.w) * 0.5);
    let opacity = clamp(input.params.z, 0.0, 1.0);
    let use_gradient = input.params.w > 0.5;
    let is_shadow = input.flags.y > 0.5;
    let center = input.rect.xy + input.rect.zw * 0.5;
    let distance_to_edge = rounded_box_distance(input.local_pos - center, input.rect.zw * 0.5, radius);
    let edge_width = max(fwidth(distance_to_edge), 0.75);
    if (is_shadow) {
        let blur = max(input.params.y, edge_width);
        let shadow_alpha = 1.0 - smoothstep(-blur, blur, distance_to_edge);
        if (shadow_alpha <= 0.0 || opacity <= 0.0) {
            discard;
        }
        return vec4<f32>(input.fill.rgb, input.fill.a * shadow_alpha * opacity);
    }
    let shape_alpha = 1.0 - smoothstep(-edge_width, edge_width, distance_to_edge);
    if (shape_alpha <= 0.0 || opacity <= 0.0) {
        discard;
    }

    let gradient_amount = select(
        clamp((input.local_pos.y - input.rect.y) / max(input.rect.w, 1.0), 0.0, 1.0),
        clamp((input.local_pos.x - input.rect.x) / max(input.rect.z, 1.0), 0.0, 1.0),
        input.flags.x < 0.5
    );
    var fill = select(input.fill, mix(input.gradient_start, input.gradient_end, gradient_amount), use_gradient);
    let blur_scale = max(
        screen.size.z / max(screen.size.x, 1.0),
        screen.size.w / max(screen.size.y, 1.0)
    );
    let blur_amount = max(input.flags.z, 0.0) * max(blur_scale, 1.0);
    if (blur_amount > 0.0) {
        let backdrop_uv = clamp(
            (input.position.xy - screen.backdrop_rect.xy) / max(screen.backdrop_rect.zw, vec2<f32>(1.0, 1.0)),
            vec2<f32>(0.0, 0.0),
            vec2<f32>(1.0, 1.0)
        );
        let blurred = backdrop_blur(backdrop_uv, blur_amount);
        fill = vec4<f32>(mix(blurred, fill.rgb, fill.a), 1.0);
    }
    let border_alpha = select(
        0.0,
        smoothstep(-border_width - edge_width, -border_width + edge_width, distance_to_edge),
        border_width > 0.0
    );
    let color = mix(fill, input.border, border_alpha);
    return vec4<f32>(color.rgb, color.a * shape_alpha * opacity);
}
"#;

pub const NEO_CAPTURE_SHADER: &str = r#"
struct Screen {
    size: vec4<f32>,
    backdrop_size: vec4<f32>,
    backdrop_rect: vec4<f32>,
};

@group(0) @binding(0)
var<uniform> screen: Screen;

@group(1) @binding(0)
var t_source: texture_2d<f32>;
@group(1) @binding(1)
var s_source: sampler;

@fragment
fn fs_main(in: FullscreenOutput) -> @location(0) vec4<f32> {
    let source_uv = clamp(
        (screen.backdrop_rect.xy + in.uv * screen.backdrop_rect.zw) / max(screen.size.xy, vec2<f32>(1.0, 1.0)),
        vec2<f32>(0.0, 0.0),
        vec2<f32>(1.0, 1.0)
    );
    return textureSample(t_source, s_source, source_uv);
}
"#;

pub const NEO_POLYGON_SHADER: &str = r#"
struct Screen {
    size: vec2<f32>,
    backdrop_size: vec2<f32>,
};

@group(0) @binding(0)
var<uniform> screen: Screen;

struct VsIn {
    @location(0) position: vec2<f32>,
    @location(1) color: vec4<f32>,
};

struct VsOut {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
};

@vertex
fn vs_main(input: VsIn) -> VsOut {
    var out: VsOut;
    let x = input.position.x / max(screen.size.x, 1.0) * 2.0 - 1.0;
    let y = 1.0 - (input.position.y / max(screen.size.y, 1.0) * 2.0);
    out.position = vec4<f32>(x, y, 0.0, 1.0);
    out.color = input.color;
    return out;
}

@fragment
fn fs_main(input: VsOut) -> @location(0) vec4<f32> {
    return input.color;
}
"#;

pub const NEO_IMAGE_SHADER: &str = r#"
struct Screen {
    size: vec2<f32>,
    backdrop_size: vec2<f32>,
};

@group(0) @binding(0)
var<uniform> screen: Screen;

@group(1) @binding(0)
var t_image: texture_2d<f32>;
@group(1) @binding(1)
var s_image: sampler;

struct VsIn {
    @location(0) position: vec2<f32>,
    @location(1) local_pos: vec2<f32>,
    @location(2) rect: vec4<f32>,
    @location(3) uv: vec2<f32>,
    @location(4) tint: vec4<f32>,
    @location(5) params: vec4<f32>,
};

struct VsOut {
    @builtin(position) position: vec4<f32>,
    @location(0) local_pos: vec2<f32>,
    @location(1) rect: vec4<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) tint: vec4<f32>,
    @location(4) params: vec4<f32>,
};

@vertex
fn vs_main(input: VsIn) -> VsOut {
    var out: VsOut;
    let x = input.position.x / max(screen.size.x, 1.0) * 2.0 - 1.0;
    let y = 1.0 - (input.position.y / max(screen.size.y, 1.0) * 2.0);
    out.position = vec4<f32>(x, y, 0.0, 1.0);
    out.local_pos = input.local_pos;
    out.rect = input.rect;
    out.uv = input.uv;
    out.tint = input.tint;
    out.params = input.params;
    return out;
}

fn rounded_box_distance(point: vec2<f32>, half_size: vec2<f32>, radius: f32) -> f32 {
    let corner = abs(point) - half_size + vec2<f32>(radius, radius);
    return length(max(corner, vec2<f32>(0.0, 0.0))) + min(max(corner.x, corner.y), 0.0) - radius;
}

@fragment
fn fs_main(input: VsOut) -> @location(0) vec4<f32> {
    let radius = clamp(input.params.x, 0.0, min(input.rect.z, input.rect.w) * 0.5);
    let opacity = clamp(input.params.y, 0.0, 1.0);
    let center = input.rect.xy + input.rect.zw * 0.5;
    let distance_to_edge = rounded_box_distance(input.local_pos - center, input.rect.zw * 0.5, radius);
    let edge_width = max(fwidth(distance_to_edge), 0.75);
    let shape_alpha = 1.0 - smoothstep(-edge_width, edge_width, distance_to_edge);
    if (shape_alpha <= 0.0 || opacity <= 0.0) {
        discard;
    }

    let sampled = textureSample(t_image, s_image, input.uv);
    let tint = vec4<f32>(input.tint.rgb, input.tint.a * opacity);
    return vec4<f32>(sampled.rgb * tint.rgb, sampled.a * tint.a * shape_alpha);
}
"#;

pub const fn default_neo_wgpu_shaders() -> NeoWgpuShaders<'static> {
    NeoWgpuShaders {
        rect: NEO_RECT_SHADER,
        polygon: NEO_POLYGON_SHADER,
        image: NEO_IMAGE_SHADER,
        capture: NEO_CAPTURE_SHADER,
    }
}
