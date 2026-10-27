#version 300 es
precision highp float;
in vec3 worldPosition;
in vec3 surfaceNormal;
in vec4 surfaceColor;
in vec4 shadowPosition;
uniform vec3 eye;
uniform sampler2D shadowMap;
uniform float fogDensity;
uniform int visualLook;
uniform bool shadowsEnabled;
uniform float shadowTexel;
uniform int shadowRadius;
// Muzzle flash point light: xyz world position, w strength (0 = off).
uniform vec4 muzzleLight;
// Alpha carries an emissive mask for the host's bloom pass: 1 is ordinary
// lit surface, lower values glow. Custom shaders that write 1 never bloom.
out vec4 outputColor;
const vec3 SUN = normalize(vec3(-0.5, 0.85, 0.4));
const vec3 SUN_COLOR = vec3(1.0, 0.86, 0.66);
// Keep in step with the sky pass in platform.js so fog meets the horizon.
vec3 horizon(vec3 dir) {
  float sun = pow(max(dot(dir, SUN), 0.0), 6.0);
  return mix(vec3(0.74, 0.79, 0.80), vec3(0.93, 0.84, 0.70), sun * 0.55);
}
vec3 grade(vec3 color) {
  if (visualLook == 3) { float y = dot(color, vec3(0.2126, 0.7152, 0.0722)); return y * vec3(0.82, 0.94, 1.0); }
  if (visualLook == 2) return min(vec3(1.0), vec3(color.r * 0.87, color.g * 1.02, color.b * 1.08 + color.r * 0.015));
  if (visualLook == 1) return min(vec3(1.0), vec3(color.r * 1.06 + color.g * 0.02, color.g * 0.98, color.b * 0.88));
  return color;
}
void main() {
  vec3 toEye = eye - worldPosition;
  float distance = length(toEye);
  vec3 view = toEye / max(distance, 1e-4);
  // Atmospheric perspective: a little haze even with the fog slider at zero.
  float fog = clamp(1.0 - exp(-distance * (fogDensity * 1.4 + 0.0035)), 0.0, 0.72);
  vec3 fogColor = horizon(-view);
  if (surfaceColor.a < 0.99) {
    // Emissive: muzzle flash, tracers and embers ignore lighting and glow.
    vec3 glow = mix(surfaceColor.rgb * 1.15, fogColor, fog * 0.35);
    outputColor = vec4(grade(min(glow, vec3(1.0))), 0.08);
    return;
  }
  vec3 normal = normalize(surfaceNormal);
  float diffuse = max(dot(normal, SUN), 0.0);
  vec3 projected = shadowPosition.xyz / shadowPosition.w * 0.5 + 0.5;
  float visible = 1.0;
  if (shadowsEnabled && all(greaterThanEqual(projected, vec3(0.0))) && all(lessThanEqual(projected, vec3(1.0)))) {
    float bias = max(0.0015 * (1.0 - dot(normal, SUN)), 0.0004);
    visible = 0.0;
    float samples = 0.0;
    for (int x = -1; x <= 1; x++) for (int y = -1; y <= 1; y++) {
      if (abs(x) > shadowRadius || abs(y) > shadowRadius) continue;
      float depth = texture(shadowMap, projected.xy + vec2(x, y) * shadowTexel).r;
      visible += projected.z - bias <= depth ? 1.0 : 0.0;
      samples += 1.0;
    }
    visible /= max(samples, 1.0);
  }
  // Cool sky fill from above, warm sandstone bounce from below.
  float up = 0.5 + 0.5 * normal.y;
  vec3 ambient = mix(vec3(0.33, 0.30, 0.27), vec3(0.47, 0.53, 0.59), up);
  // Walls and crates darken toward the ground, a cheap contact occlusion.
  float occlusion = normal.y > 0.6 ? 1.0 : mix(0.72, 1.0, smoothstep(0.0, 1.4, worldPosition.y));
  vec3 albedo = surfaceColor.rgb;
  vec3 lit = albedo * (ambient * occlusion + SUN_COLOR * 0.66 * diffuse * visible);
  // Soft sky rim keeps silhouettes readable against shadowed stone.
  float rim = pow(1.0 - max(dot(normal, view), 0.0), 3.0);
  lit += vec3(0.55, 0.62, 0.70) * rim * 0.10 * up;
  if (muzzleLight.w > 0.0) {
    vec3 toLight = muzzleLight.xyz - worldPosition;
    float d2 = dot(toLight, toLight);
    float facing = max(dot(normal, toLight * inversesqrt(max(d2, 1e-4))), 0.0);
    lit += albedo * vec3(1.0, 0.72, 0.38) * muzzleLight.w * facing * 2.4 / (1.0 + d2 * 0.9);
  }
  vec3 color = mix(lit, fogColor, fog);
  outputColor = vec4(grade(min(color, vec3(1.0))), 1.0);
}
