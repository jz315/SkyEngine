[[vk::binding(0)]] RWTexture2D<float4> output_tex;
[[vk::binding(1)]] cbuffer _ {
    float4 output_size;
    float4 triangle_color;
};

float edge(float2 a, float2 b, float2 p) {
    return (p.x - a.x) * (b.y - a.y) - (p.y - a.y) * (b.x - a.x);
}

[numthreads(8, 8, 1)]
void main(in uint2 px : SV_DispatchThreadID) {
    if (px.x >= uint(output_size.x) || px.y >= uint(output_size.y)) {
        return;
    }

    float2 uv = (float2(px) + 0.5) * output_size.zw;
    float2 p = uv * 2.0 - 1.0;
    p.y = -p.y;

    float2 a = float2(0.0, 0.68);
    float2 b = float2(-0.72, -0.54);
    float2 c = float2(0.72, -0.54);

    float w0 = edge(b, c, p);
    float w1 = edge(c, a, p);
    float w2 = edge(a, b, p);
    bool inside =
        (w0 >= 0.0 && w1 >= 0.0 && w2 >= 0.0) ||
        (w0 <= 0.0 && w1 <= 0.0 && w2 <= 0.0);

    float grid = ((uint(px.x) / 32u + uint(px.y) / 32u) & 1u) ? 0.035 : 0.0;
    float3 bg = lerp(float3(0.09, 0.11, 0.16), float3(0.16, 0.20, 0.29), uv.y) + grid;
    if (inside) {
        float3 weights = abs(float3(w0, w1, w2));
        float3 bary = weights / max(weights.x + weights.y + weights.z, 0.0001);
        float3 hot = triangle_color.rgb * (0.7 + 0.45 * bary);
        output_tex[px] = float4(hot, 1.0);
    } else {
        output_tex[px] = float4(bg, 1.0);
    }
}
