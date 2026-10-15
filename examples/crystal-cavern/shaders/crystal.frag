vec4 ferravex_fragment(vec4 color, vec3 world, vec3 normal) {
    float wave = 0.88 + 0.12 * sin(u_params[0].x * 2.2 + world.y * 4.0);
    float facet = 0.92 + 0.08 * abs(normal.y);
    return vec4(color.rgb * wave * facet, color.a);
}
