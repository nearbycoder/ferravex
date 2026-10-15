vec4 ferravex_fragment(vec4 color, vec3 world, vec3 normal) {
    vec3 heat = vec3(1.0, 0.9 + 0.1 * sin(u_params[0].x * 11.0), 0.95);
    return vec4(color.rgb * heat, color.a);
}
