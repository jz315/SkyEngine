// WickedEngine-inspired SSGI scene composite.
// Adds the final indirect diffuse buffer back to the visible scene color.

@group(0) @binding(0)
var t_indirect_diffuse: texture_2d<f32>;
@group(0) @binding(1)
var t_composite_scene_color: texture_2d<f32>;

@fragment
fn fs_scene_composite(input: FullscreenOutput) -> @location(0) vec4<f32> {
    let dims = textureDimensions(t_composite_scene_color);
    let pixel = clamp(
        vec2<i32>(input.position.xy),
        vec2<i32>(0),
        vec2<i32>(dims) - vec2<i32>(1),
    );
    let scene_color = textureLoad(t_composite_scene_color, pixel, 0);
    let indirect = textureLoad(t_indirect_diffuse, pixel, 0).rgb;
    return vec4<f32>(scene_color.rgb + indirect, scene_color.a);
}
