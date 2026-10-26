uniform sampler2D u_texture;
uniform float u_aa;
uniform vec4 u_edge_fade;
varying PAINT_PRECISION vec2 v_uv;
varying PAINT_PRECISION vec4 v_color;
varying PAINT_PRECISION vec4 v_shape;
varying PAINT_PRECISION vec2 v_params;
varying PAINT_PRECISION float v_logical_y;
varying PAINT_PRECISION float v_logical_x;
uniform vec4 u_round_clip;
uniform float u_round_radius;
float round_coverage() {
    if (u_round_clip.z <= 0.0) return 1.0;
    vec2 half_size = u_round_clip.zw * 0.5;
    vec2 p = vec2(v_logical_x, v_logical_y) - u_round_clip.xy - half_size;
    vec2 q = abs(p) - half_size + vec2(u_round_radius);
    float d = length(max(q, vec2(0.0))) + min(max(q.x, q.y), 0.0) - u_round_radius;
    return 1.0 - smoothstep(-1.0, 0.0, d);
}
float edge_coverage() {
    float rc = round_coverage();
    if (u_edge_fade.y <= u_edge_fade.x) return rc;
    return rc * smoothstep(u_edge_fade.x, u_edge_fade.y, v_logical_y) *
        (1.0 - smoothstep(u_edge_fade.z, u_edge_fade.w, v_logical_y));
}
void main() {
    if (v_params.x < 0.0) {
        gl_FragColor = texture2D(u_texture, v_uv) * v_color * vec4(1.0, 1.0, 1.0, edge_coverage());
        return;
    }
    float blur = max(-v_params.y, 0.0);
    vec2 half_size = v_shape.zw - vec2(blur);
    float radius = min(v_params.x, min(half_size.x, half_size.y));
    vec2 q = abs(v_shape.xy) - half_size + vec2(radius);
    // Interior fast path: away from every edge and corner the coverage of a
    // fill or shadow is exactly 1, so skip length() and smoothstep.
    float inner = max(q.x, q.y);
    if (v_params.y <= 0.0 && inner <= min(0.0, radius - (blur > 0.0 ? blur : u_aa))) {
        gl_FragColor = texture2D(u_texture, v_uv) * v_color * vec4(1.0, 1.0, 1.0, edge_coverage());
        return;
    }
    float d = length(max(q, vec2(0.0))) + min(inner, 0.0) - radius;
    float alpha = 1.0 - smoothstep(-u_aa, 0.0, d);
    if (blur > 0.0) {
        alpha = 1.0 - smoothstep(-blur, blur, d);
    }
    if (v_params.y > 0.0) {
        alpha *= smoothstep(-v_params.y - u_aa, -v_params.y, d);
    }
    gl_FragColor = texture2D(u_texture, v_uv) * v_color * vec4(1.0, 1.0, 1.0, alpha * edge_coverage());
}
