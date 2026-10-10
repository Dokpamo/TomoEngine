struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) normal: vec3<f32>,
    @location(1) color: vec3<f32>,
};

@vertex
fn vs_main(@location(0) position: vec4<f32>, @location(1) normal: vec3<f32>, @location(2) color: vec3<f32>) -> VertexOutput {
    var result: VertexOutput;
    result.position = position;
    result.normal = normal;
    result.color = color;
    return result;
}

@fragment
fn fs_solid(input: VertexOutput) -> @location(0) vec4<f32> {
    let light = normalize(vec3<f32>(-0.4, 0.8, 0.6));
    let diffuse = max(dot(normalize(input.normal), light), 0.0);
    return vec4<f32>(input.color * (0.3 + 0.7 * diffuse), 1.0);
}

@fragment
fn fs_line(input: VertexOutput) -> @location(0) vec4<f32> {
    return vec4<f32>(input.color, 1.0);
}
