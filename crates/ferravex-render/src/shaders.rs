//! Custom materials with explicit GLSL (OpenGL/WebGL) and MSL (Metal) sources.
use crate::prelude::*;
use macroquad::miniquad::{Backend, Comparison, ShaderSource, UniformDesc, UniformType};

pub use macroquad::material::{Material, MaterialParams};

/// Both shader languages are required so custom effects remain portable.
pub struct ShaderSources<'a> {
    pub vertex_glsl: &'a str,
    pub fragment_glsl: &'a str,
    pub metal: &'a str,
}

/// Compile the appropriate source for the active GPU backend.
/// Call from the initialized Macroquad event-loop thread.
pub fn load_shader(sources: ShaderSources<'_>, params: MaterialParams) -> Result<Material, String> {
    // SAFETY: Macroquad exposes backend information through this API only.
    // The borrow ends immediately; no renderer state is retained or mutated.
    let backend = unsafe { get_internal_gl().quad_context.info().backend };
    let source = match backend {
        Backend::OpenGl => ShaderSource::Glsl {
            vertex: sources.vertex_glsl,
            fragment: sources.fragment_glsl,
        },
        Backend::Metal => ShaderSource::Msl {
            program: sources.metal,
        },
    };
    load_material(source, params).map_err(|error| format!("Shader compilation failed: {error}"))
}

#[derive(Clone, Copy)]
pub struct LitSettings {
    pub light_direction: Vec3,
    pub camera_position: Vec3,
    pub fog_color: Color,
    pub fog_start: f32,
    pub fog_end: f32,
    pub ambient: f32,
}

impl Default for LitSettings {
    fn default() -> Self {
        Self {
            light_direction: vec3(-0.4, 0.85, 0.3),
            camera_position: vec3(12.0, 10.0, 16.0),
            fog_color: Color::from_rgba(14, 22, 35, 255),
            fog_start: 20.0,
            fog_end: 55.0,
            ambient: 0.38,
        }
    }
}

/// A small vertex-lit material with distance fog, with no texture assets.
/// Use normals-bearing meshes from [`crate::mesh`] or your own meshes.
pub struct LitMaterial(pub Material);

impl LitMaterial {
    pub fn new() -> Result<Self, String> {
        load_shader(
            ShaderSources {
                vertex_glsl: LIT_VERTEX_GLSL,
                fragment_glsl: LIT_FRAGMENT_GLSL,
                metal: LIT_METAL,
            },
            MaterialParams {
                uniforms: vec![
                    UniformDesc::new("Light", UniformType::Float4),
                    UniformDesc::new("Eye", UniformType::Float4),
                    UniformDesc::new("FogColor", UniformType::Float4),
                    UniformDesc::new("FogRange", UniformType::Float4),
                ],
                pipeline_params: PipelineParams {
                    depth_test: Comparison::LessOrEqual,
                    depth_write: true,
                    ..Default::default()
                },
                ..Default::default()
            },
        )
        .map(Self)
    }

    pub fn begin(&self, settings: LitSettings) {
        self.0.set_uniform(
            "Light",
            settings
                .light_direction
                .normalize_or_zero()
                .extend(settings.ambient.clamp(0.0, 1.0)),
        );
        self.0
            .set_uniform("Eye", settings.camera_position.extend(1.0));
        self.0.set_uniform(
            "FogColor",
            Vec4::new(
                settings.fog_color.r,
                settings.fog_color.g,
                settings.fog_color.b,
                settings.fog_color.a,
            ),
        );
        self.0.set_uniform(
            "FogRange",
            vec4(
                settings.fog_start,
                (settings.fog_end - settings.fog_start).max(0.001),
                0.0,
                0.0,
            ),
        );
        gl_use_material(&self.0);
    }

    pub fn end(&self) {
        gl_use_default_material();
    }
}

pub const LIT_VERTEX_GLSL: &str = r#"#version 100
attribute vec3 position;
attribute vec2 texcoord;
attribute vec4 color0;
attribute vec4 normal;
uniform mat4 Model;
uniform mat4 Projection;
uniform vec4 Light;
uniform vec4 Eye;
uniform vec4 FogRange;
varying lowp vec4 color;
varying lowp float fog;
void main() {
    vec4 world = Model * vec4(position, 1.0);
    vec3 n = normalize((Model * vec4(normal.xyz, 0.0)).xyz);
    float shade = Light.w + (1.0 - Light.w) * max(dot(n, Light.xyz), 0.0);
    color = vec4(color0.rgb / 255.0 * shade, color0.a / 255.0);
    fog = clamp((distance(world.xyz, Eye.xyz) - FogRange.x) / FogRange.y, 0.0, 1.0);
    gl_Position = Projection * world;
}"#;

pub const LIT_FRAGMENT_GLSL: &str = r#"#version 100
precision mediump float;
varying lowp vec4 color;
varying lowp float fog;
uniform vec4 FogColor;
void main() { gl_FragColor = vec4(mix(color.rgb, FogColor.rgb, fog), color.a); }
"#;

pub const LIT_METAL: &str = r#"
#include <metal_stdlib>
using namespace metal;
struct Vertex {
    float3 position [[attribute(0)]];
    float2 texcoord [[attribute(1)]];
    float4 color0 [[attribute(2)]];
    float4 normal [[attribute(3)]];
};
struct Uniforms {
    float4x4 Model;
    float4x4 Projection;
    float4 _Time;
    float4 Light;
    float4 Eye;
    float4 FogColor;
    float4 FogRange;
};
struct RasterizerData {
    float4 position [[position]];
    float4 color [[user(locn0)]];
    float fog [[user(locn1)]];
};
vertex RasterizerData vertexShader(Vertex v [[stage_in]], constant Uniforms& u [[buffer(0)]]) {
    RasterizerData out;
    float4 world = u.Model * float4(v.position, 1.0);
    float3 n = normalize((u.Model * float4(v.normal.xyz, 0.0)).xyz);
    float shade = u.Light.w + (1.0 - u.Light.w) * max(dot(n, u.Light.xyz), 0.0);
    out.position = u.Projection * world;
    out.color = float4(v.color0.rgb / 255.0 * shade, v.color0.a / 255.0);
    out.fog = clamp((distance(world.xyz, u.Eye.xyz) - u.FogRange.x) / u.FogRange.y, 0.0, 1.0);
    return out;
}
fragment float4 fragmentShader(RasterizerData in [[stage_in]], constant Uniforms& u [[buffer(0)]]) {
    return float4(mix(in.color.rgb, u.FogColor.rgb, in.fog), in.color.a);
}
"#;
