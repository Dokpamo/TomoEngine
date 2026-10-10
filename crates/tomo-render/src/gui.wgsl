// SPDX-License-Identifier: MIT
struct VertexOut {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
}
@group(0) @binding(0) var image: texture_2d<f32>;
@group(0) @binding(1) var image_sampler: sampler;
@vertex fn vs_main(@location(0) position: vec2<f32>, @location(1) uv: vec2<f32>, @location(2) color: vec4<f32>) -> VertexOut {
    var output: VertexOut;
    output.position = vec4<f32>(position, 0.0, 1.0);
    output.uv = uv;
    output.color = color;
    return output;
}
@fragment fn fs_main(input: VertexOut) -> @location(0) vec4<f32> {
    let gamma = input.color * textureSample(image, image_sampler, input.uv);
    let linear = select(gamma.rgb / 12.92, pow((gamma.rgb + vec3<f32>(0.055)) / 1.055, vec3<f32>(2.4)), gamma.rgb > vec3<f32>(0.04045));
    return vec4<f32>(linear, gamma.a);
}
