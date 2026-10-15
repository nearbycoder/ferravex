#version 300 es
precision highp float;
layout(location=0) in vec3 position;
layout(location=1) in vec3 normal;
layout(location=2) in vec4 color;
layout(location=3) in mat4 model;
uniform mat4 viewProjection;
uniform mat4 lightProjection;
out vec3 worldPosition;
out vec3 surfaceNormal;
out vec4 surfaceColor;
out vec4 shadowPosition;
void main(){
 vec4 world=model*vec4(position,1.0);
 worldPosition=world.xyz;
 surfaceNormal=transpose(inverse(mat3(model)))*normal;
 surfaceColor=color;
 shadowPosition=lightProjection*world;
 gl_Position=viewProjection*world;
 gl_Position.z=gl_Position.z*2.0-gl_Position.w;
}
