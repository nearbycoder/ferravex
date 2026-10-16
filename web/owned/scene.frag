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
out vec4 outputColor;
void main(){
 vec3 normal=normalize(surfaceNormal);
 vec3 light=normalize(vec3(-0.5,0.85,0.4));
 float diffuse=max(dot(normal,light),0.0);
 vec3 projected=shadowPosition.xyz/shadowPosition.w*0.5+0.5;
 float visible=1.0;
 if(shadowsEnabled&&all(greaterThanEqual(projected,vec3(0.0)))&&all(lessThanEqual(projected,vec3(1.0)))){
  float bias=max(0.0015*(1.0-dot(normal,light)),0.0004);visible=0.0;
  for(int x=-1;x<=1;x++)for(int y=-1;y<=1;y++){
   float depth=texture(shadowMap,projected.xy+vec2(x,y)/1024.0).r;
   visible+=projected.z-bias<=depth?1.0/9.0:0.0;
  }
 }
 float hemispheric=0.5+0.5*normal.y;
 vec3 ambient=mix(vec3(0.29,0.32,0.34),vec3(0.54,0.57,0.58),hemispheric);
 vec3 lit=surfaceColor.rgb*(ambient+vec3(0.62,0.57,0.47)*diffuse*visible);
 float fog=1.0-exp(-length(worldPosition-eye)*fogDensity);
 vec3 color=mix(lit,vec3(0.66,0.72,0.73),clamp(fog,0.0,0.65));
 if(visualLook==3){float y=dot(color,vec3(0.2126,0.7152,0.0722));color=y*vec3(0.82,0.94,1.0);}
 else if(visualLook==2)color=min(vec3(1.0),vec3(color.r*0.87,color.g*1.02,color.b*1.08+color.r*0.015));
 else if(visualLook==1)color=min(vec3(1.0),vec3(color.r*1.06+color.g*0.02,color.g*0.98,color.b*0.88));
 outputColor=vec4(color,surfaceColor.a);
}
