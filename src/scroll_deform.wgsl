struct Uniform {
    bounds: vec4f,
    scale_anchor: vec4f,
    motion: vec4f,
    background: vec4f,
}
@group(0) @binding(0) var<uniform> p: Uniform;
@group(0) @binding(1) var source: texture_2d<f32>;
@group(0) @binding(2) var linear_sampler: sampler;
struct VertexOutput { @builtin(position) position: vec4f, @location(0) uv: vec2f }
@vertex fn vertex(@builtin(vertex_index) index: u32) -> VertexOutput {
    let positions = array<vec2f, 3>(vec2f(-1., -1.), vec2f(3., -1.), vec2f(-1., 3.));
    let pos = positions[index];
    var out: VertexOutput;
    out.position = vec4f(pos, 0., 1.);
    out.uv = vec2f(pos.x * 0.5 + 0.5, 0.5 - pos.y * 0.5);
    return out;
}
@fragment fn capture(in: VertexOutput) -> @location(0) vec4f {
    let location = clamp(vec2i(p.bounds.xy + in.uv * p.bounds.zw), vec2i(0), vec2i(p.motion.zw) - vec2i(1));
    return textureLoad(source, location, 0);
}
@fragment fn warp(in: VertexOutput) -> @location(0) vec4f {
    let local = (in.position.xy - p.bounds.xy) / p.bounds.zw;
    let anchor = p.scale_anchor.zw;
    let uv = anchor + (local - anchor - vec2f(0., p.motion.x / p.bounds.w)) / p.scale_anchor.xy;
    // Sample before branching so implicit texture derivatives remain uniform.
    let warped = textureSample(source, linear_sampler, clamp(uv, vec2f(0.), vec2f(1.)));
    let original = textureSample(source, linear_sampler, local);
    let inside = all(uv >= vec2f(0.)) && all(uv <= vec2f(1.));
    let color = select(p.background, warped, inside);
    return mix(original, color, p.motion.y);
}
