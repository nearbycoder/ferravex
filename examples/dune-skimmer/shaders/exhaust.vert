vec3 ferravex_vertex(vec3 position) {
    position.y *= 1.0 + 0.08 * sin(u_params[0].x * 20.0 + position.y * 7.0);
    return position;
}
vec3 ferravex_normal(vec3 normal) {
    return normal;
}
